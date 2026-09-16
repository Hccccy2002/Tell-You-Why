//! Windows resources travel with the EXE; mutable PDF data stays per user.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug)]
pub(crate) struct Layout {
    pub service: PathBuf,
    pub python: PathBuf,
    pub data: PathBuf,
    pub models: PathBuf,
    pub evals: PathBuf,
}
impl Layout {
    fn defaults(resources: &Path, local_data: &Path, checkout: Option<&Path>) -> Self {
        let bundle = resources.join("pdf-runtime");
        match checkout.filter(|_| !bundle.is_dir()) {
            Some(root) => Self {
                service: root.join("rag-service"),
                python: root.join("rag-service/.venv/Scripts/python.exe"),
                data: root.join("data"),
                models: root.join("data/models"),
                evals: root.join("evals"),
            },
            None => Self {
                service: bundle.join("rag-service"),
                python: bundle.join("python/python.exe"),
                models: bundle.join("models"),
                evals: bundle.join("evals"),
                data: local_data.join("com.tellyouwhy.desktop/pdf-data"),
            },
        }
    }

    pub fn discover() -> Result<Self, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let local = std::env::var_os("LOCALAPPDATA").ok_or("无法确定本机用户数据目录")?;
        let checkout = if cfg!(debug_assertions) {
            Path::new(env!("CARGO_MANIFEST_DIR")).parent()
        } else {
            None
        };
        let mut paths = Self::defaults(
            exe.parent().ok_or("无法确定安装目录")?,
            Path::new(&local),
            checkout,
        );
        if let Some(v) = std::env::var_os("TELLWHY_RAG_SERVICE") {
            paths.service = v.into();
            paths.python = paths.service.join(".venv/Scripts/python.exe");
        }
        if let Some(v) = std::env::var_os("TELLWHY_PYTHON") {
            paths.python = v.into();
        }
        if let Some(v) = std::env::var_os("TELLWHY_KB_DATA") {
            paths.data = v.into();
        }
        if let Some(v) = std::env::var_os("TELLWHY_KB_MODELS") {
            paths.models = v.into();
        }
        if let Some(v) = std::env::var_os("TELLWHY_EVALS") {
            paths.evals = v.into();
        }
        if !paths.python.is_file() || !paths.service.join("src/tellwhy_kb/desktop.py").is_file() {
            return Err(
                "PDF 处理组件缺失。安装版请重新安装完整包；源码开发请按 README 准备 PDF 环境。"
                    .into(),
            );
        }
        Ok(paths)
    }
}

pub(crate) fn configure(command: &mut Command, service: &Path, data: &Path, models: &Path) {
    command
        .current_dir(service)
        .env_remove("PYTHONHOME")
        .env("PYTHONPATH", service.join("src"))
        .env("PYTHONNOUSERSITE", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONUTF8", "1")
        .env("TELLWHY_MODEL_CACHE", data.join("runtime-cache"))
        .env("TELLWHY_KB_MODELS", models)
        .env("HF_HUB_OFFLINE", "1")
        .env("TRANSFORMERS_OFFLINE", "1")
        .env("PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK", "True");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_never_falls_back_to_build_machine() {
        let d = tempfile::tempdir().unwrap();
        let paths = Layout::defaults(d.path(), Path::new("user-data"), None);
        assert_eq!(paths.python, d.path().join("pdf-runtime/python/python.exe"));
        assert!(paths.data.starts_with("user-data"));
        assert!(!paths.data.starts_with(d.path()));
    }
    #[test]
    fn bundle_with_unicode_and_spaces_takes_precedence_over_checkout() {
        let d = tempfile::tempdir().unwrap();
        let install = d.path().join("中文 安装路径");
        std::fs::create_dir_all(install.join("pdf-runtime")).unwrap();
        let paths = Layout::defaults(
            &install,
            Path::new("user-data"),
            Some(Path::new("build-checkout")),
        );
        assert_eq!(paths.models, install.join("pdf-runtime/models"));
        assert_eq!(paths.evals, install.join("pdf-runtime/evals"));
    }
    #[test]
    fn development_checkout_keeps_existing_venv_and_data() {
        let d = tempfile::tempdir().unwrap();
        let paths = Layout::defaults(
            d.path(),
            Path::new("user-data"),
            Some(Path::new("checkout")),
        );
        assert_eq!(
            paths.python,
            Path::new("checkout/rag-service/.venv/Scripts/python.exe")
        );
        assert_eq!(paths.data, Path::new("checkout/data"));
    }
}
