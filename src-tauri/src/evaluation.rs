//! Desktop evaluation jobs use independent files/databases, never the learning database.
use crate::{db::ProviderProfileRecord, AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Suite {
    Review,
    Top5,
    Model,
}
impl Suite {
    fn title(self) -> &'static str {
        match self {
            Self::Review => "Agent 流程评测",
            Self::Top5 => "RAG Top 5 评测",
            Self::Model => "真实模型评测",
        }
    }
    fn total(self) -> usize {
        match self {
            Self::Review => 10,
            Self::Top5 => 40,
            Self::Model => 9,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: Suite,
    pub title: String,
    pub status: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub completed: usize,
    pub total: usize,
    pub message: String,
    pub error: Option<String>,
    pub model: Option<String>,
}
struct Registry {
    jobs: Vec<Job>,
    active: Option<(String, Arc<AtomicBool>)>,
}
pub struct EvaluationManager {
    root: PathBuf,
    registry: Mutex<Registry>,
}
pub struct EvaluationState(pub Result<Arc<EvaluationManager>, String>);
impl EvaluationState {
    fn manager(&self) -> Result<&Arc<EvaluationManager>, String> {
        self.0.as_ref().map_err(Clone::clone)
    }
}

pub(crate) fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| format!("无法读取评测文件：{e}"))?)
        .map_err(|e| format!("评测文件格式错误：{e}"))
}
pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let temp = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    fs::write(&temp, bytes)
        .and_then(|_| fs::rename(&temp, path))
        .map_err(|e| format!("无法保存评测记录：{e}"))
}
fn valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id)
}

impl EvaluationManager {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let mut jobs = vec![];
        for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let id = entry.file_name().to_string_lossy().to_string();
            if !valid_id(&id) || !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            let path = entry.path().join("job.json");
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            let Ok(mut job) = serde_json::from_slice::<Job>(&bytes) else {
                continue;
            };
            if job.id != id {
                continue;
            }
            if ["running", "cancelling"].contains(&job.status.as_str()) {
                job.status = "interrupted".into();
                job.message = "上次退出时评测尚未完成，可查看已有结果或重新运行。".into();
                job.finished_at = Some(chrono::Utc::now().to_rfc3339());
                write_json(&path, &job)?;
            }
            jobs.push(job);
        }
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(Self {
            root,
            registry: Mutex::new(Registry { jobs, active: None }),
        })
    }
    fn folder(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("无效的评测记录".into());
        }
        Ok(self.root.join(id))
    }
    fn snapshot(&self) -> Result<Vec<Job>, String> {
        let registry = self.registry.lock().map_err(|e| e.to_string())?;
        Ok(registry.jobs.iter().take(50).cloned().collect())
    }
    fn create(
        self: &Arc<Self>,
        kind: Suite,
        model: Option<String>,
    ) -> Result<(Job, Control), String> {
        let mut registry = self.registry.lock().map_err(|e| e.to_string())?;
        if registry.active.is_some() {
            return Err("已有评测正在运行，请等待完成或先停止。".into());
        }
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            title: kind.title().into(),
            status: "running".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            completed: 0,
            total: kind.total(),
            message: "正在准备评测…".into(),
            error: None,
            model,
        };
        let dir = self.folder(&job.id)?;
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        write_json(&dir.join("job.json"), &job)?;
        let cancel = Arc::new(AtomicBool::new(false));
        registry.active = Some((job.id.clone(), cancel.clone()));
        registry.jobs.insert(0, job.clone());
        Ok((
            job.clone(),
            Control {
                dir,
                cancel,
                manager: self.clone(),
                id: job.id,
            },
        ))
    }
    fn update(&self, id: &str, change: impl FnOnce(&mut Job)) -> Result<(), String> {
        let mut registry = self.registry.lock().map_err(|e| e.to_string())?;
        let job = registry
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or("找不到评测记录")?;
        change(job);
        write_json(&self.folder(id)?.join("job.json"), job)
    }
    fn finish(&self, id: &str, outcome: Result<(), String>) -> Result<(), String> {
        let mut registry = self.registry.lock().map_err(|e| e.to_string())?;
        let cancelled = registry
            .active
            .as_ref()
            .is_some_and(|(active, flag)| active == id && flag.load(Ordering::SeqCst));
        let job = registry
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or("找不到评测记录")?;
        job.status = if cancelled {
            "cancelled"
        } else if outcome.is_ok() {
            "completed"
        } else {
            "failed"
        }
        .into();
        job.message = match job.status.as_str() {
            "completed" => "评测已完成",
            "cancelled" => "评测已停止",
            _ => "评测未完成",
        }
        .into();
        job.error = if cancelled { None } else { outcome.err() };
        job.finished_at = Some(chrono::Utc::now().to_rfc3339());
        let saved = write_json(&self.folder(id)?.join("job.json"), job);
        registry.active = None;
        saved
    }
    fn cancel(&self, id: &str) -> Result<(), String> {
        let mut registry = self.registry.lock().map_err(|e| e.to_string())?;
        let Some((active, cancel)) = &registry.active else {
            return Ok(());
        };
        if active != id {
            return Err("该评测当前未运行".into());
        }
        cancel.store(true, Ordering::SeqCst);
        let job = registry
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or("找不到评测记录")?;
        job.status = "cancelling".into();
        job.message = "正在停止；已发出的模型请求结束后保存结果。".into();
        write_json(&self.folder(id)?.join("job.json"), job)
    }
    fn document(&self, id: &str) -> Result<Value, String> {
        let registry = self.registry.lock().map_err(|e| e.to_string())?;
        let job = registry
            .jobs
            .iter()
            .find(|j| j.id == id)
            .ok_or("找不到评测记录")?;
        let path = self.folder(id)?.join("report.json");
        let mut report = if path.exists() {
            Some(read_json(&path)?)
        } else {
            None
        };
        if job.kind == Suite::Model {
            if let Some(report) = report.as_mut() {
                if let Err(error) = crate::evaluation_review::decorate(&self.folder(id)?, report) {
                    report["human_review"] = json!({"error":error});
                }
            }
        }
        Ok(json!({"job":job,"report":report}))
    }
    fn save_review(&self, request: crate::evaluation_review::SaveReview) -> Result<Value, String> {
        let id = request.id.clone();
        {
            // Serialize review writes and reject a still-changing execution report.
            let registry = self.registry.lock().map_err(|e| e.to_string())?;
            let job = registry
                .jobs
                .iter()
                .find(|j| j.id == id)
                .ok_or("找不到评测记录")?;
            if job.kind != Suite::Model || ["running", "cancelling"].contains(&job.status.as_str())
            {
                return Err("请等待真实模型评测结束后再复核。".into());
            }
            let dir = self.folder(&id)?;
            let report = read_json(&dir.join("report.json"))?;
            crate::evaluation_review::save(&dir, &report, request)?;
        }
        self.document(&id)
    }
}

#[derive(Clone)]
pub(crate) struct Control {
    pub dir: PathBuf,
    pub cancel: Arc<AtomicBool>,
    manager: Arc<EvaluationManager>,
    id: String,
}
impl Control {
    pub fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::SeqCst) {
            Err("评测已停止".into())
        } else {
            Ok(())
        }
    }
    pub fn progress(&self, completed: usize, total: usize, message: &str) -> Result<(), String> {
        self.check()?;
        self.manager.update(&self.id, |job| {
            job.completed = completed.min(total);
            job.total = total;
            job.message = message.into();
        })
    }
}

pub(crate) struct Python {
    executable: PathBuf,
    service: PathBuf,
    data: PathBuf,
    script: PathBuf,
    models: PathBuf,
}
impl Python {
    fn benchmark(
        &self,
        root: &Path,
        output: Option<&Path>,
        request: &Value,
    ) -> Result<Value, String> {
        let mut command = Command::new(&self.executable);
        crate::pdf_runtime::configure(&mut command, &self.service, &self.data, &self.models);
        command
            .args(["-X", "utf8", "-u"])
            .arg(
                self.script
                    .parent()
                    .ok_or("评测路径无效")?
                    .join("textbook/benchmark.py"),
            )
            .arg("--root")
            .arg(root)
            .arg("--data")
            .arg(&self.data)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(output) = output {
            command.arg("--out").arg(output);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("无法启动题库组件：{e}"))?;
        let payload = serde_json::to_vec(request).map_err(|e| e.to_string())?;
        if let Err(error) = child
            .stdin
            .take()
            .ok_or("题库输入不可用")?
            .write_all(&payload)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.to_string());
        }
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("题库组件异常退出，请检查本地 Python 环境".into());
        }
        let reply: Value =
            serde_json::from_slice(&output.stdout).map_err(|_| "题库组件返回格式错误")?;
        if reply["ok"] != true {
            return Err(reply["error"].as_str().unwrap_or("题库操作失败").into());
        }
        Ok(reply["result"].clone())
    }
    pub(crate) fn discover() -> Result<Self, String> {
        let paths = crate::pdf_runtime::Layout::discover()?;
        let script = paths.evals.join("panel.py");
        if !script.is_file() {
            return Err("评测组件缺失，请重新安装完整包。".into());
        }
        Ok(Self {
            executable: paths.python,
            service: paths.service,
            data: paths.data,
            models: paths.models,
            script,
        })
    }
    pub fn run(&self, action: &str, control: &Control, cancellable: bool) -> Result<(), String> {
        if cancellable {
            control.check()?;
        }
        let log_path = control.dir.join(format!("{action}.log"));
        let log = fs::File::create(&log_path).map_err(|e| e.to_string())?;
        let mut command = Command::new(&self.executable);
        crate::pdf_runtime::configure(&mut command, &self.service, &self.data, &self.models);
        command
            .args(["-X", "utf8", "-u"])
            .arg(&self.script)
            .arg(action)
            .arg("--out")
            .arg(&control.dir)
            .arg("--data")
            .arg(&self.data)
            .env("PYTHONPATH", self.service.join("src"))
            .env("PYTHONUTF8", "1")
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| format!("无法启动评测：{e}"))?;
        let progress = control.dir.join("progress.json");
        let preparing_total = if control.dir.join("selection.json").exists() {
            read_json(&control.dir.join("selection.json"))?["case_ids"]
                .as_array()
                .map_or(9, Vec::len)
        } else {
            9
        };
        let mut last = Value::Null;
        loop {
            if cancellable && control.check().is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return Err("评测已停止".into());
            }
            if cancellable && progress.exists() {
                if let Ok(value) = read_json(&progress) {
                    if value != last {
                        let done = value["completed"].as_u64().unwrap_or(0) as usize;
                        let total = value["total"].as_u64().unwrap_or(1) as usize;
                        let preparing = action == "prepare-model";
                        let message = if preparing {
                            format!("正在准备教材摘录 {done} / {total}，尚未开始模型评测…")
                        } else {
                            value["message"].as_str().unwrap_or("正在评测…").to_string()
                        };
                        if let Err(error) = control.progress(
                            if preparing { 0 } else { done },
                            if preparing { preparing_total } else { total },
                            &message,
                        ) {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err(error);
                        }
                        last = value;
                    }
                }
            }
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(_)) => {
                    let log = fs::read_to_string(&log_path).unwrap_or_default();
                    let last_lines = log
                        .lines()
                        .rev()
                        .take(4)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("\n");
                    return Err(format!("评测执行失败：{last_lines}"));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(150)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error.to_string());
                }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartEvaluation {
    pub kind: Suite,
    pub provider: Option<String>,
    pub region: Option<String>,
    #[serde(default)]
    pub benchmark: Option<Value>,
}

#[tauri::command]
pub async fn evaluation_benchmark(
    request: Value,
    state: State<'_, EvaluationState>,
) -> Result<Value, String> {
    if !matches!(
        request["action"].as_str(),
        Some("read" | "save" | "freeze" | "source-page" | "bind-source")
    ) {
        return Err("无效的题库操作".into());
    }
    if request.to_string().len() > 500_000 {
        return Err("题库请求过大".into());
    }
    let manager = state.manager()?.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = manager.registry.lock().map_err(|e| e.to_string())?;
        Python::discover()?.benchmark(&manager.root.join("benchmark"), None, &request)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn evaluation_benchmark_export(
    release: String,
    app: tauri::AppHandle,
    state: State<'_, EvaluationState>,
) -> Result<Option<String>, String> {
    let manager = state.manager()?.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dataset = {
            let _guard = manager.registry.lock().map_err(|e| e.to_string())?;
            Python::discover()?.benchmark(
                &manager.root.join("benchmark"),
                None,
                &json!({"action":"export","release":release}),
            )?
        };
        let mut dialog = app
            .dialog()
            .file()
            .set_title("导出封存评测题库")
            .add_filter("JSON", &["json"])
            .set_file_name(format!(
                "{}.json",
                dataset["dataset_version"]
                    .as_str()
                    .unwrap_or("human-benchmark")
            ));
        if let Some(window) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        let Some(selected) = dialog.blocking_save_file() else {
            return Ok(None);
        };
        let path = selected.into_path().map_err(|_| "请选择本地文件路径")?;
        fs::write(
            &path,
            serde_json::to_vec_pretty(&dataset).map_err(|e| e.to_string())?,
        )
        .map_err(|_| "无法保存题库，请检查目录权限")?;
        Ok(Some(path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn evaluation_list(state: State<'_, EvaluationState>) -> Result<Value, String> {
    Ok(json!({"jobs":state.manager()?.snapshot()?,"unavailable_reason":Python::discover().err()}))
}
#[tauri::command]
pub fn evaluation_read(id: String, state: State<'_, EvaluationState>) -> Result<Value, String> {
    state.manager()?.document(&id)
}
#[tauri::command]
pub fn evaluation_cancel(id: String, state: State<'_, EvaluationState>) -> Result<(), String> {
    state.manager()?.cancel(&id)
}

#[tauri::command]
pub fn evaluation_save_review(
    request: crate::evaluation_review::SaveReview,
    state: State<'_, EvaluationState>,
) -> Result<Value, String> {
    state.manager()?.save_review(request)
}

#[tauri::command]
pub fn evaluation_start(
    request: StartEvaluation,
    state: State<'_, EvaluationState>,
    app_state: State<'_, AppState>,
) -> Result<Job, String> {
    if request.kind == Suite::Review && request.benchmark.is_some() {
        return Err("Agent 流程使用受控场景，不使用人工题库".into());
    }
    let python = Python::discover()?;
    let profile: Option<ProviderProfileRecord> = if request.kind == Suite::Model {
        let _permit = app_state.persistence_gate.try_operation()?;
        let provider = request.provider.as_deref().ok_or("请选择模型通道")?;
        let region = request.region.as_deref().ok_or("请选择模型区域")?;
        let profile = app_state
            .database
            .provider_profile(provider, region)
            .map_err(|e| e.to_string())?
            .filter(|p| p.connection_verified && p.key_last4.is_some())
            .ok_or("请先在模型设置中配置并测试通道")?;
        crate::providers::ProviderContext::from_registry(provider, region, &profile.model)
            .map_err(|e| e.to_string())?;
        Some(profile)
    } else {
        None
    };
    let (job, control) = state.manager()?.create(
        request.kind,
        profile
            .as_ref()
            .map(|p| format!("{} / {} / {}", p.provider_id, p.region, p.model)),
    )?;
    let manager = state.manager()?.clone();
    let id = job.id.clone();
    let http = app_state.http.clone();
    let secrets = app_state.secrets.clone();
    tauri::async_runtime::spawn(async move {
        let result = tauri::async_runtime::spawn_blocking(move || {
            tauri::async_runtime::block_on(async {
                if let Some(mut selection) = request.benchmark {
                    if !selection.is_object() {
                        return Err("无效的题库选择".into());
                    }
                    selection["action"] = json!("prepare");
                    selection["kind"] = json!(request.kind);
                    control.check()?;
                    let prepared = {
                        let _guard = control.manager.registry.lock().map_err(|e| e.to_string())?;
                        python.benchmark(
                            &control.manager.root.join("benchmark"),
                            Some(&control.dir),
                            &selection,
                        )?
                    };
                    let count = prepared["case_ids"].as_array().ok_or("缺少评测题目")?.len();
                    control.progress(0, count, "已绑定封存题库，正在准备评测…")?;
                }
                match request.kind {
                    Suite::Review => {
                        let result = crate::review_eval::evaluate_desktop(&control).await;
                        if control.dir.join("runs.json").exists() {
                            python.run("score-review", &control, false)?;
                        }
                        result
                    }
                    Suite::Top5 => {
                        python.run("top5", &control, true)?;
                        let count = if control.dir.join("selection.json").exists() {
                            read_json(&control.dir.join("selection.json"))?["case_ids"]
                                .as_array()
                                .ok_or("缺少评测题目")?
                                .len()
                        } else {
                            40
                        };
                        control.progress(count, count, "评测完成")
                    }
                    Suite::Model => {
                        python.run("prepare-model", &control, true)?;
                        let result = crate::evaluation_live::run(
                            &control,
                            profile.ok_or("缺少模型配置")?,
                            http,
                            secrets,
                        )
                        .await;
                        if control.dir.join("runs.json").exists() {
                            python.run("score-model", &control, false)?;
                        }
                        result
                    }
                }
            })
        })
        .await
        .map_err(|_| "评测执行异常中断，已保存的结果可在历史记录中查看。".to_string())
        .and_then(|v| v);
        let _ = manager.finish(&id, result);
    });
    Ok(job)
}

#[tauri::command]
pub async fn evaluation_export(
    id: String,
    app: tauri::AppHandle,
    state: State<'_, EvaluationState>,
) -> Result<Option<String>, String> {
    let bytes =
        serde_json::to_vec_pretty(&state.manager()?.document(&id)?).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app
            .dialog()
            .file()
            .set_title("导出评测报告")
            .add_filter("JSON", &["json"])
            .set_file_name(format!("evaluation-{id}.json"));
        if let Some(window) = app.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        let Some(selected) = dialog.blocking_save_file() else {
            return Ok(None);
        };
        let path = selected.into_path().map_err(|_| "请选择本地文件路径")?;
        fs::write(&path, bytes).map_err(|_| "无法保存报告，请检查目录权限")?;
        Ok(Some(path.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "local indexed PDF and Python runtime; isolated synthetic labels, no model API"]
    fn desktop_benchmark_snapshot_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(EvaluationManager::open(dir.path().to_path_buf()).unwrap());
        let root = dir.path().join("benchmark");
        let python = Python::discover().unwrap();
        let document = python
            .benchmark(&root, None, &json!({"action":"read"}))
            .unwrap();
        assert_eq!(document["dataset"]["cases"].as_array().unwrap().len(), 40);
        let kb = document["sources"][0]["kb"]
            .as_str()
            .expect("需要至少一份已发布的本地 PDF");
        let bound = python
            .benchmark(
                &root,
                None,
                &json!({"action":"bind-source","revision":0,"kb":kb}),
            )
            .unwrap();
        let page = python
            .benchmark(&root, None, &json!({"action":"source-page","page":1}))
            .unwrap();
        let evidence = page["blocks"][0].clone();
        assert!(evidence.is_object());
        let quote = evidence["quote"].as_str().unwrap();
        let case = json!({"id":"smoke_case","question":format!("请解释：{}",quote.chars().take(100).collect::<String>()),
            "reference_answer":quote,"acceptance_criteria":"自动化测试，不构成质量判断", "split":"regression", "kind":"single", "expected_behavior":"answer", "evidence":[evidence],
            "review":{"human_reviewer":"自动化联调样例，非真实人工评审","notes":"仅验证封存和快照，不构成事实质量结论"}});
        let saved = python
            .benchmark(
                &root,
                None,
                &json!({"action":"save","revision":bound["revision"],"case":case,"confirm_human":true}),
            )
            .unwrap();
        let frozen = python
            .benchmark(
                &root,
                None,
                &json!({"action":"freeze","revision":saved["revision"]}),
            )
            .unwrap();
        let release = &frozen["releases"][0]["id"];
        let (job, control) = manager.create(Suite::Top5, None).unwrap();
        let selection = python
            .benchmark(
                &root,
                Some(&control.dir),
                &json!({"action":"prepare","release":release,"split":"regression","kind":"top5"}),
            )
            .unwrap();
        assert_eq!(selection["case_ids"], json!([case["id"]]));
        assert_eq!(
            read_json(&control.dir.join("dataset.json")).unwrap()["cases"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let export = python
            .benchmark(&root, None, &json!({"action":"export","release":release}))
            .unwrap();
        assert_eq!(
            export,
            read_json(&control.dir.join("dataset.json")).unwrap()
        );
        python.run("top5", &control, true).unwrap();
        manager.finish(&job.id, Ok(())).unwrap();
        let document = manager.document(&job.id).unwrap();
        assert_eq!(document["report"]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(document["report"]["benchmark"]["release"], *release);
        assert_eq!(document["report"]["raw"]["human_reviewed"], 1);
        assert_eq!(document["job"]["total"], 1);

        // Real local evidence preparation, then a synthetic answer: never send an API request.
        let (model_job, model_control) = manager.create(Suite::Model, None).unwrap();
        python
            .benchmark(
                &root,
                Some(&model_control.dir),
                &json!({"action":"prepare","release":release,"split":"regression","kind":"model"}),
            )
            .unwrap();
        python.run("prepare-model", &model_control, true).unwrap();
        let retrieval_path = model_control.dir.join("retrieval.json");
        let retrieval = read_json(&retrieval_path).unwrap();
        use sha2::{Digest, Sha256};
        write_json(&model_control.dir.join("runs.json"), &json!({
            "binding":{"dataset_sha256":retrieval["dataset_sha256"],"input_sha256":format!("{:x}", Sha256::digest(fs::read(&retrieval_path).unwrap())),"ids":["smoke_case"]},
            "cases":[{"id":"smoke_case","state":"completed","draft":{"status":"insufficient","reason":"自动化测试输出，非真实模型回答"}}],"agent":null
        })).unwrap();
        write_json(&model_control.dir.join("requests.json"), &json!([])).unwrap();
        python.run("score-model", &model_control, false).unwrap();
        manager.finish(&model_job.id, Ok(())).unwrap();
        let model_doc = manager.document(&model_job.id).unwrap();
        assert_eq!(
            model_doc["report"]["rows"][0]["details"]["evaluation"]["expected_behavior"],
            "answer"
        );
        assert_eq!(model_doc["report"]["human_review"]["reviewed"], 0);
        let reviewed = manager
            .save_review(crate::evaluation_review::SaveReview {
                id: model_job.id,
                case_id: "smoke_case".into(),
                report_sha256: model_doc["report"]["human_review"]["report_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                expected_revision: 0,
                review: crate::evaluation_review::ReviewInput {
                    reviewer: "自动化联调样例，非真实人工评审".into(),
                    correct: Some(false),
                    complete: Some(false),
                    grounded: Some(false),
                    behavior_appropriate: Some(false),
                    notes: "测试误拒答计分与导出，不构成质量结论".into(),
                },
            })
            .unwrap();
        assert_eq!(reviewed["report"]["human_review"]["reviewed"], 1);
        assert_eq!(reviewed["report"]["rows"][0]["status"], "failed");
        assert_eq!(
            reviewed["report"]["raw"]["report"]["quality_reviews"]["human"]["behavior_appropriate"],
            0.0
        );
    }
    #[test]
    fn human_reviews_survive_restart_and_export_without_changing_execution() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(EvaluationManager::open(dir.path().to_path_buf()).unwrap());
        let (job, control) = manager.create(Suite::Model, None).unwrap();
        let original = crate::evaluation_review::tests::report();
        let path = control.dir.join("report.json");
        write_json(&path, &original).unwrap();
        let bytes = fs::read(&path).unwrap();
        let request = || {
            let mut request = crate::evaluation_review::tests::request(&original);
            request.id = job.id.clone();
            request
        };
        assert!(manager.save_review(request()).is_err());
        manager.finish(&job.id, Ok(())).unwrap();
        manager.save_review(request()).unwrap();
        let reopened = EvaluationManager::open(dir.path().to_path_buf()).unwrap();
        let exported = reopened.document(&job.id).unwrap();
        assert_eq!(exported["report"]["human_review"]["reviewed"], 1);
        assert_eq!(
            exported["report"]["raw"]["human_review"]["rows"][0]["method"],
            "human"
        );
        assert_eq!(exported["job"]["status"], "completed");
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn jobs_are_exclusive_persistent_and_recover_without_restarting_requests() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(EvaluationManager::open(dir.path().to_path_buf()).unwrap());
        let (job, control) = manager.create(Suite::Review, None).unwrap();
        assert!(manager.create(Suite::Top5, None).is_err());
        control.progress(3, 10, "第三题").unwrap();
        assert!(manager.document("../secret").is_err());
        let recovered = EvaluationManager::open(dir.path().to_path_buf()).unwrap();
        assert_eq!(recovered.snapshot().unwrap()[0].status, "interrupted");
        assert_eq!(recovered.snapshot().unwrap()[0].completed, 3);
        manager.cancel(&job.id).unwrap();
        assert!(control.check().is_err());
        manager.finish(&job.id, Err("停止".into())).unwrap();
        assert_eq!(manager.snapshot().unwrap()[0].status, "cancelled");
        assert!(manager.create(Suite::Review, None).is_ok());
    }
    #[test]
    fn reports_distinguish_execution_failure_from_success() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(EvaluationManager::open(dir.path().to_path_buf()).unwrap());
        let (job, control) = manager.create(Suite::Review, None).unwrap();
        write_json(&control.dir.join("report.json"), &json!({"rows":[]})).unwrap();
        manager.finish(&job.id, Err("测试故障".into())).unwrap();
        let document = manager.document(&job.id).unwrap();
        assert_eq!(document["job"]["status"], "failed");
        assert_eq!(document["job"]["error"], "测试故障");
        assert!(document["report"]["rows"].is_array());
    }
    #[test]
    #[ignore = "runs the desktop offline review adapter and Python scorer"]
    fn desktop_evaluation_review_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(EvaluationManager::open(dir.path().to_path_buf()).unwrap());
        let (job, control) = manager.create(Suite::Review, None).unwrap();
        tauri::async_runtime::block_on(crate::review_eval::evaluate_desktop(&control)).unwrap();
        Python::discover()
            .unwrap()
            .run("score-review", &control, false)
            .unwrap();
        manager.finish(&job.id, Ok(())).unwrap();
        let doc = manager.document(&job.id).unwrap();
        assert_eq!(doc["report"]["metrics"][0]["value"], "10 / 10");
        assert_eq!(doc["report"]["rows"].as_array().unwrap().len(), 10);
    }

    #[test]
    #[ignore = "replays a saved report locally; no model requests or original report changes"]
    fn desktop_human_review_replay() {
        let fixture = PathBuf::from(
            std::env::var_os("TELLWHY_REVIEW_FIXTURE").expect("saved report path required"),
        );
        let root = PathBuf::from(
            std::env::var_os("TELLWHY_REVIEW_OUTPUT").expect("isolated output required"),
        );
        let original = read_json(&fixture).unwrap();
        let manager = Arc::new(EvaluationManager::open(root.clone()).unwrap());
        let (job, control) = manager
            .create(Suite::Model, Some("本地报告回放测试".into()))
            .unwrap();
        write_json(&control.dir.join("report.json"), &original).unwrap();
        manager.finish(&job.id, Ok(())).unwrap();
        let unreviewed = manager.document(&job.id).unwrap();
        assert_eq!(unreviewed["report"]["human_review"]["total"], 8);
        write_json(&root.join("unreviewed.json"), &unreviewed).unwrap();
        let request = |expected_revision, complete| crate::evaluation_review::SaveReview {
            id: job.id.clone(),
            case_id: "d01".into(),
            report_sha256: unreviewed["report"]["human_review"]["report_sha256"]
                .as_str()
                .unwrap()
                .into(),
            expected_revision,
            review: crate::evaluation_review::ReviewInput {
                reviewer: "自动化测试样例（非人工结论）".into(),
                correct: Some(false),
                complete,
                grounded: Some(true),
                behavior_appropriate: None,
                notes: "仅验证保存与统计流程，不构成对真实回答的人工评价。".into(),
            },
        };
        let draft = manager.save_review(request(0, None)).unwrap();
        assert_eq!(draft["report"]["human_review"]["reviewed"], 0);
        write_json(&root.join("draft.json"), &draft).unwrap();
        manager.save_review(request(1, Some(true))).unwrap();
        let reopened = EvaluationManager::open(root.clone()).unwrap();
        let completed = reopened.document(&job.id).unwrap();
        assert_eq!(completed["report"]["human_review"]["reviewed"], 1);
        assert_eq!(
            completed["report"]["raw"]["report"]["quality_reviews"]["human"]["correct"],
            0.0
        );
        assert_eq!(
            completed["report"]["raw"]["human_review"]["rows"][0]["method"],
            "human"
        );
        assert_eq!(
            read_json(&control.dir.join("report.json")).unwrap(),
            original
        );
        write_json(&root.join("completed.json"), &completed).unwrap();
        println!("Human review replay: {}", control.dir.display());
    }

    #[test]
    #[ignore = "opt-in desktop real-model evaluation, uses the approved profile and at most 40 API calls"]
    fn desktop_evaluation_model_smoke() {
        let profile_path = std::env::var_os("TELLWHY_PANEL_PROFILE_DB")
            .expect("explicit profile snapshot required");
        let root = std::env::var_os("TELLWHY_PANEL_OUTPUT")
            .expect("explicit evaluation output directory required");
        let profile = crate::db::Database::new(profile_path.into())
            .provider_profiles()
            .unwrap()
            .into_iter()
            .find(|p| p.provider_id == "deepseek" && p.connection_verified && p.key_last4.is_some())
            .expect("verified DeepSeek profile required");
        let manager = Arc::new(EvaluationManager::open(root.into()).unwrap());
        let (job, control) = manager
            .create(Suite::Model, Some(profile.model.clone()))
            .unwrap();
        let python = Python::discover().unwrap();
        python.run("prepare-model", &control, true).unwrap();
        let result = tauri::async_runtime::block_on(crate::evaluation_live::run(
            &control,
            profile,
            Arc::new(crate::providers::RestrictedHttpClient::new().unwrap()),
            Arc::new(crate::secret_store::WindowsCredentialStore),
        ));
        manager.finish(&job.id, result.clone()).unwrap();
        if control.dir.join("runs.json").exists() {
            python.run("score-model", &control, false).unwrap();
        }
        println!("Desktop model report: {}", control.dir.display());
        result.unwrap();
        let doc = manager.document(&job.id).unwrap();
        assert_eq!(doc["report"]["rows"].as_array().unwrap().len(), 9);
        let requests = read_json(&control.dir.join("requests.json")).unwrap();
        assert!(requests.as_array().unwrap().len() <= 40);
        assert_eq!(
            doc["report"]["metrics"].as_array().unwrap().last().unwrap()["value"],
            "待复核"
        );
    }
}
