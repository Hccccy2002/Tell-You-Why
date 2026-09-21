use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

// Retain a tray navigation request until the main page is ready to receive it.
#[derive(Default)]
pub struct NavigationState(Mutex<Option<String>>);

impl NavigationState {
    pub(crate) fn clear(&self) {
        if let Ok(mut pending) = self.0.lock() {
            *pending = None;
        }
    }

    fn acknowledge(&self, id: &str) -> Result<(), String> {
        let mut pending = self.0.lock().map_err(|_| "页面导航暂不可用")?;
        if pending.as_deref() == Some(id) {
            *pending = None;
        }
        Ok(())
    }
}

pub(crate) fn open_study_center(app: &AppHandle) -> Result<(), String> {
    *app.state::<NavigationState>()
        .0
        .lock()
        .map_err(|_| "页面导航暂不可用")? = Some(uuid::Uuid::new_v4().to_string());
    crate::desktop::show_window(app);
    app.emit_to("main", "window-navigation", ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn take_window_navigation(state: State<'_, NavigationState>) -> Result<Option<Value>, String> {
    Ok(state
        .0
        .lock()
        .map_err(|_| "页面导航暂不可用")?
        .as_ref()
        .map(|id| json!({"id":id,"entry":{"kind":"study"}})))
}

#[tauri::command]
pub fn acknowledge_window_navigation(
    state: State<'_, NavigationState>,
    id: String,
) -> Result<(), String> {
    state.acknowledge(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledgement_does_not_drop_a_newer_tray_request() {
        let state = NavigationState::default();
        *state.0.lock().unwrap() = Some("new".into());
        state.acknowledge("old").unwrap();
        assert_eq!(state.0.lock().unwrap().as_deref(), Some("new"));
        state.acknowledge("new").unwrap();
        assert!(state.0.lock().unwrap().is_none());
    }

    #[test]
    fn reset_discards_pending_navigation() {
        let state = NavigationState::default();
        *state.0.lock().unwrap() = Some("pending".into());
        state.clear();
        assert!(state.0.lock().unwrap().is_none());
    }

    #[test]
    fn app_config_creates_only_the_main_window() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let windows = config["app"]["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0]["label"], "main");
    }
}
