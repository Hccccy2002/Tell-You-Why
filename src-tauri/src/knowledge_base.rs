use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const REPLY_PREFIX: &str = "TELLWHY_DESKTOP:";

#[derive(Clone)]
pub(crate) struct Runtime {
    service: PathBuf,
    python: PathBuf,
    data: PathBuf,
    models: PathBuf,
}

impl Runtime {
    pub(crate) async fn call_controlled(
        &self,
        request: Value,
        control: crate::harness::execution::ExecutionControl,
    ) -> Result<Value, crate::harness::tools::ToolError> {
        use crate::harness::{
            process,
            tools::{ErrorCode, ToolError},
        };
        // Only the review agent's read operations use process-tree cancellation.
        // Import workers have their own durable job and cancellation protocol.
        if !matches!(
            request["op"].as_str(),
            Some("evidence" | "learning_version" | "learning_units")
        ) {
            return Err(ToolError::new(
                ErrorCode::PermissionDenied,
                "当前资料执行器仅允许复习读取操作",
            ));
        }
        let mut command = process::python_command(
            &self.python,
            "import runpy\nrunpy.run_module('tellwhy_kb.desktop', run_name='__main__')",
        );
        command
            .current_dir(&self.service)
            .env("PYTHONPATH", self.service.join("src"))
            .env("PYTHONUTF8", "1")
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK", "True");
        let output = process::execute(command, self.payload(request), control.clone()).await?;
        control.check()?;
        if !output.status.success() {
            return Err(ToolError::new(
                ErrorCode::DependencyFailed,
                "本地资料组件异常退出",
            ));
        }
        parse_reply(&output.stdout).map_err(|_| {
            ToolError::new(
                ErrorCode::InvalidResult,
                "资料服务未返回有效结果，请检查资料版本与状态",
            )
        })
    }
    pub(crate) fn discover() -> Result<Self, String> {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let service = std::env::var_os("TELLWHY_RAG_SERVICE")
            .map(PathBuf::from)
            .unwrap_or_else(|| checkout.join("rag-service"));
        let python = std::env::var_os("TELLWHY_PYTHON")
            .map(PathBuf::from)
            .unwrap_or_else(|| service.join(".venv/Scripts/python.exe"));
        if !python.is_file() || !service.join("src/tellwhy_kb/desktop.py").is_file() {
            return Err(
                "本机 PDF 处理组件未就绪。请按项目 README 安装 Python 运行环境后重试。".into(),
            );
        }
        let data = std::env::var_os("TELLWHY_KB_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| service.parent().unwrap_or(checkout).join("data"));
        let models = std::env::var_os("TELLWHY_KB_MODELS")
            .map(PathBuf::from)
            .unwrap_or_else(|| data.join("models"));
        Ok(Self {
            service,
            python,
            data,
            models,
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.python);
        command
            .args(["-X", "utf8", "-u", "-m", "tellwhy_kb.desktop"])
            .current_dir(&self.service)
            .env("PYTHONPATH", self.service.join("src"))
            .env("PYTHONUTF8", "1")
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK", "True")
            .stdin(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        command
    }

    fn payload(&self, request: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "data_root": self.data, "models_root": self.models, "request": request
        }))
        .expect("JSON request is serializable")
    }

    pub(crate) fn call(&self, request: Value) -> Result<Value, String> {
        let mut child = self
            .command()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("无法启动 PDF 处理组件：{e}"))?;
        child
            .stdin
            .take()
            .ok_or("无法连接 PDF 处理组件")?
            .write_all(&self.payload(request))
            .map_err(|e| e.to_string())?;
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        parse_reply(&output.stdout).map_err(|error| {
            if output.status.success() {
                error
            } else {
                format!("{error}（进程退出码 {:?}）", output.status.code())
            }
        })
    }

    fn spawn_import(
        &self,
        kb: &str,
        job: &str,
        permit: ImportPermit,
        errors: Arc<Mutex<Option<String>>>,
    ) -> Result<(), String> {
        validate_id(kb)?;
        validate_id(job)?;
        self.call(json!({"op": "launch_ready", "kb": kb, "job": job}))?;
        let log_path = self
            .data
            .join("knowledge-bases")
            .join(kb)
            .join("work")
            .join(job)
            .join("desktop-worker.log");
        let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
        // Disk-backed output lets the worker finish even after a full app exit.
        let mut worker_runtime = self.clone();
        let windowless_python = self.python.with_file_name("pythonw.exe");
        if cfg!(windows) && windowless_python.is_file() {
            // multiprocessing inherits this executable, keeping OCR workers windowless too.
            worker_runtime.python = windowless_python;
        }
        let mut child = worker_runtime
            .command()
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log)
            .spawn()
            .map_err(|e| format!("无法启动导入任务：{e}"))?;
        child
            .stdin
            .take()
            .ok_or("无法连接导入任务")?
            .write_all(&self.payload(json!({"op":"run", "kb":kb, "job":job})))
            .map_err(|e| e.to_string())?;
        std::thread::spawn(move || {
            let _permit = permit;
            let outcome = child.wait().map_err(|e| e.to_string()).and_then(|_| {
                let bytes = std::fs::read(log_path).map_err(|e| e.to_string())?;
                parse_reply(&bytes)
            });
            if let Err(error) = outcome {
                if let Ok(mut value) = errors.lock() {
                    *value = Some(error);
                }
            }
        });
        Ok(())
    }
}

fn parse_reply(bytes: &[u8]) -> Result<Value, String> {
    let text = String::from_utf8_lossy(bytes);
    let response = text
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(REPLY_PREFIX))
        .ok_or("PDF 处理组件未返回结果，可查看任务日志后重试")?;
    let value: Value = serde_json::from_str(response).map_err(|e| e.to_string())?;
    if value["ok"] == true {
        Ok(value["data"].clone())
    } else {
        Err(value["error"].as_str().unwrap_or("PDF 处理失败").to_owned())
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 120
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("无效的知识库或任务标识".into());
    }
    Ok(())
}

#[derive(Default)]
pub struct KnowledgeBaseState {
    busy: Arc<AtomicBool>,
    errors: Arc<Mutex<Option<String>>>,
}

struct ImportPermit(Arc<AtomicBool>);
impl Drop for ImportPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
impl KnowledgeBaseState {
    fn acquire(&self) -> Result<ImportPermit, String> {
        self.busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "已有导入任务正在启动或处理，请稍候")?;
        if let Ok(mut error) = self.errors.lock() {
            *error = None;
        }
        Ok(ImportPermit(self.busy.clone()))
    }
}

#[derive(Deserialize, serde::Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReadRequest {
    Catalog {},
    Inspect {
        pdf: String,
    },
    Chapters {
        kb: String,
    },
    Browse {
        kb: String,
        chapter: Option<String>,
        offset: usize,
    },
    Search {
        kb: String,
        query: String,
        mode: String,
        chapter: Option<String>,
    },
    Page {
        kb: String,
        page: u32,
        version: Option<String>,
    },
}

#[tauri::command]
pub async fn kb_read(
    state: tauri::State<'_, KnowledgeBaseState>,
    request: ReadRequest,
) -> Result<Value, String> {
    let busy = state.busy.clone();
    let errors = state.errors.clone();
    let catalog = matches!(request, ReadRequest::Catalog {});
    tauri::async_runtime::spawn_blocking(move || {
        let mut result =
            Runtime::discover()?.call(serde_json::to_value(request).map_err(|e| e.to_string())?)?;
        if catalog {
            result["import_running"] =
                json!(result["import_running"] == true || busy.load(Ordering::SeqCst));
            result["launch_error"] = json!(errors.lock().map_err(|e| e.to_string())?.clone());
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn kb_import(
    state: tauri::State<'_, KnowledgeBaseState>,
    pdf: String,
    first_page: u32,
) -> Result<Value, String> {
    let permit = state.acquire()?;
    let errors = state.errors.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = Runtime::discover()?;
        let result = runtime.call(json!({"op":"prepare", "pdf":pdf, "first_page":first_page}))?;
        if result["reused"] != true {
            runtime.spawn_import(
                result["kb"].as_str().ok_or("缺少知识库标识")?,
                result["job"].as_str().ok_or("缺少任务标识")?,
                permit,
                errors,
            )?;
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn kb_resume(
    state: tauri::State<'_, KnowledgeBaseState>,
    kb: String,
    job: String,
) -> Result<(), String> {
    let permit = state.acquire()?;
    let errors = state.errors.clone();
    tauri::async_runtime::spawn_blocking(move || {
        Runtime::discover()?.spawn_import(&kb, &job, permit, errors)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn kb_pause(kb: String, job: String) -> Result<Value, String> {
    validate_id(&kb)?;
    validate_id(&job)?;
    tauri::async_runtime::spawn_blocking(move || {
        Runtime::discover()?.call(json!({"op":"cancel", "kb":kb, "job":job}))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn controlled_runtime_uses_framed_protocol_and_rejects_write_operations() {
        tauri::async_runtime::block_on(async {
            let folder = tempfile::tempdir().unwrap();
            let package = folder.path().join("src/tellwhy_kb");
            std::fs::create_dir_all(&package).unwrap();
            std::fs::write(package.join("__init__.py"), "").unwrap();
            std::fs::write(package.join("desktop.py"),"import sys,json\npayload=json.load(sys.stdin)\nprint('TELLWHY_DESKTOP:'+json.dumps({'ok':True,'data':payload['request']}),flush=True)\n").unwrap();
            let runtime = Runtime {
                service: folder.path().to_owned(),
                python: Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .join("rag-service/.venv/Scripts/python.exe"),
                data: folder.path().join("data"),
                models: folder.path().join("models"),
            };
            let scope = crate::harness::execution::ExecutionScope::new(10_000);
            let request = json!({"op":"learning_version","kb":"book","version":"v1"});
            assert_eq!(
                runtime
                    .call_controlled(request.clone(), scope.control.clone())
                    .await
                    .unwrap(),
                request
            );
            scope.finish().await.unwrap();
            let scope = crate::harness::execution::ExecutionScope::new(10_000);
            assert_eq!(
                runtime
                    .call_controlled(
                        json!({"op":"cancel","kb":"book","job":"existing-import"}),
                        scope.control.clone()
                    )
                    .await
                    .unwrap_err()
                    .code,
                crate::harness::tools::ErrorCode::PermissionDenied
            );
            scope.finish().await.unwrap();
        });
    }

    #[test]
    fn framed_protocol_ignores_model_logs_and_reports_failures() {
        assert_eq!(
            parse_reply(b"model log\nTELLWHY_DESKTOP:{\"ok\":true,\"data\":{\"pages\":436}}\n")
                .unwrap()["pages"],
            436
        );
        assert!(
            parse_reply(b"TELLWHY_DESKTOP:{\"ok\":false,\"error\":\"bad pdf\"}")
                .unwrap_err()
                .contains("bad pdf")
        );
        assert!(parse_reply(b"crash").is_err());
    }

    #[test]
    fn import_permit_rejects_double_click_and_releases_after_failure() {
        let state = KnowledgeBaseState::default();
        let first = state.acquire().unwrap();
        assert!(state.acquire().is_err());
        drop(first);
        assert!(state.acquire().is_ok());
    }

    #[test]
    fn paths_and_read_commands_are_restricted() {
        for id in ["../source", "C:\\tmp", "a/b", "", "."] {
            assert!(validate_id(id).is_err());
        }
        assert!(validate_id("computer-organization").is_ok());
        assert!(
            serde_json::from_value::<ReadRequest>(json!({"op":"run","kb":"x","job":"y"})).is_err()
        );
        assert!(
            serde_json::from_value::<ReadRequest>(json!({"op":"catalog","data_root":"other"}))
                .is_err()
        );
    }
}
