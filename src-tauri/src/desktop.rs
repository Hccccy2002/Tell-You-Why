use crate::auto_hide;
use crate::models::{AppSettings, ReminderPreset, WindowState};
use crate::AppState;
use chrono::{Datelike, Local, Timelike, Utc};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tauri_plugin_notification::NotificationExt;

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let shortcut = app
        .state::<AppState>()
        .database
        .settings()
        .ok()
        .map_or_else(
            || "Alt+Shift+Y".to_string(),
            |settings| settings.global_shortcut,
        );
    if app.global_shortcut().register(shortcut.as_str()).is_err() {
        let _ = app.emit_to(
            "main",
            "desktop-notice",
            "默认快捷键已被其他应用占用，请在通用设置中更换。",
        );
    }
    create_tray(app)?;
    if let Some(window) = app.get_webview_window("main") {
        restore_window_state(&window);
        if let Ok(settings) = app.state::<AppState>().database.settings() {
            let _ = window.set_always_on_top(settings.always_on_top);
            app.state::<AppState>()
                .auto_hide
                .set_enabled(settings.auto_hide_on_mouse_leave);
        }
    }
    auto_hide::start_monitor(app.handle().clone());
    start_reminder_scheduler(app.handle().clone());
    Ok(())
}

fn create_tray(app: &App) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(
        app,
        "toggle",
        "显示 / 隐藏 Tell You Why",
        true,
        None::<&str>,
    )?;
    let next = MenuItem::with_id(app, "next", "下一条知识", true, None::<&str>)?;
    let study = MenuItem::with_id(app, "study", "打开学习中心", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "暂停 30 分钟", true, None::<&str>)?;
    let today = MenuItem::with_id(app, "today", "今天不再提醒", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&toggle, &study, &next, &pause, &today, &settings, &exit],
    )?;
    let mut tray = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Tell You Why")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_window(app),
            "study" => {
                let _ = crate::navigation::open_study_center(app);
            }
            "next" => {
                show_window(app);
                let _ = app.emit_to("main", "request-next-card", ());
            }
            "pause" => pause_from_tray(app, false),
            "today" => pause_from_tray(app, true),
            "settings" => {
                show_window(app);
                let _ = app.emit_to("main", "open-settings", ());
            }
            "exit" => {
                let state = app.state::<AppState>();
                let Ok(_permit) = state.persistence_gate.try_operation() else {
                    return;
                };
                state.exiting.store(true, Ordering::SeqCst);
                let _ = crate::study_commands::pause_active(&state.database);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = save_window_state_under_permit(&window);
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = save_window_state(&window);
            window.state::<AppState>().auto_hide.window_hidden();
            let _ = window.hide();
        } else {
            show_window(app);
        }
    }
}

pub(crate) fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        ensure_window_visible(&window);
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        window.state::<AppState>().auto_hide.window_shown();
    }
}

fn pause_from_tray(app: &AppHandle, today: bool) {
    let state = app.state::<AppState>();
    let Ok(_permit) = state.persistence_gate.try_operation() else {
        return;
    };
    if let Ok(mut settings) = state.database.settings() {
        let until = if today {
            let tomorrow = Local::now().date_naive().succ_opt();
            tomorrow
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .and_then(|value| value.and_local_timezone(Local).single())
                .map(|value| value.with_timezone(&Utc))
        } else {
            Some(Utc::now() + chrono::Duration::minutes(30))
        };
        if let Some(until) = until {
            settings.pause_until = Some(until.to_rfc3339());
            let _ = state.database.save_settings(&settings);
        }
    }
}

pub fn handle_close_requested(window: &WebviewWindow, api: &tauri::CloseRequestApi) {
    let state = window.state::<AppState>();
    if state.exiting.load(Ordering::SeqCst) {
        return;
    }
    api.prevent_close();
    if let Ok(_permit) = state.persistence_gate.try_operation() {
        if crate::study_commands::pause_active(&state.database).is_err() {
            return;
        }
        let _ = save_window_state_under_permit(window);
        if !state.database.close_tip_shown().unwrap_or(true) {
            let shortcut = state
                .database
                .settings()
                .map(|settings| settings.global_shortcut)
                .unwrap_or_else(|_| "Alt+Shift+Y".to_string());
            let _ = window
                .app_handle()
                .notification()
                .builder()
                .title("Tell You Why")
                .body(format!("应用已收至系统托盘，可用 {shortcut} 再次打开。"))
                .show();
            let _ = state.database.mark_close_tip_shown();
        }
    }
    window.state::<AppState>().auto_hide.window_hidden();
    let _ = window.hide();
}

pub(crate) fn hide_window_for_auto_hide(
    window: &WebviewWindow,
    request: auto_hide::AutoHideRequest,
) {
    let state = window.state::<AppState>();
    let Ok(_permit) = state.persistence_gate.try_operation() else {
        state.auto_hide.protect_for_interaction();
        return;
    };
    let _ = save_window_state_under_permit(window);
    let Some(snapshot) = auto_hide::window_pointer_snapshot(window.app_handle(), window) else {
        state.auto_hide.protect_for_interaction();
        return;
    };
    if !state.auto_hide.confirm_hide(request, snapshot) {
        return;
    }
    if window.hide().is_ok() {
        if !state.database.auto_hide_tip_shown().unwrap_or(true) {
            let shortcut = state
                .database
                .settings()
                .map(|settings| settings.global_shortcut)
                .unwrap_or_else(|_| "Alt+Shift+Y".to_string());
            let notification_result = window
                .app_handle()
                .notification()
                .builder()
                .title("Tell You Why")
                .body(format!(
                    "已自动收至系统托盘，可用 {shortcut} 恢复；可在通用设置关闭此功能。"
                ))
                .show();
            mark_tip_after_notification_success(notification_result, || {
                let _ = state.database.mark_auto_hide_tip_shown();
            });
        }
    } else {
        state.auto_hide.window_shown();
    }
}

fn mark_tip_after_notification_success<T, E>(
    notification_result: Result<T, E>,
    mark_tip: impl FnOnce(),
) {
    if notification_result.is_ok() {
        mark_tip();
    }
}

pub fn save_window_state(window: &WebviewWindow) -> Result<(), String> {
    let state = window.state::<AppState>();
    let _permit = state
        .persistence_gate
        .try_operation()
        .map_err(|error| error.to_string())?;
    save_window_state_under_permit(window)
}

fn save_window_state_under_permit(window: &WebviewWindow) -> Result<(), String> {
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let monitor_name = window
        .current_monitor()
        .ok()
        .flatten()
        .and_then(|monitor| monitor.name().cloned());
    window
        .state::<AppState>()
        .database
        .save_window_state(&WindowState {
            x: position.x,
            y: position.y,
            width: size.width.max(360),
            height: size.height.max(440),
            monitor_name,
        })
        .map_err(|error| error.to_string())
}

fn restore_window_state(window: &WebviewWindow) {
    if let Ok(Some(saved)) = window.state::<AppState>().database.window_state() {
        let _ = window.set_size(PhysicalSize::new(
            saved.width.max(360),
            saved.height.max(440),
        ));
        let visible = window.available_monitors().ok().is_some_and(|monitors| {
            monitors.iter().any(|monitor| {
                let position = monitor.position();
                let size = monitor.size();
                let right = position.x + size.width as i32;
                let bottom = position.y + size.height as i32;
                saved.x + 80 > position.x
                    && saved.y + 60 > position.y
                    && saved.x < right - 40
                    && saved.y < bottom - 40
            })
        });
        if visible {
            let _ = window.set_position(PhysicalPosition::new(saved.x, saved.y));
        } else {
            let _ = window.center();
        }
    }
}

pub(crate) fn ensure_window_visible(window: &WebviewWindow) {
    let visible = window.outer_position().ok().is_some_and(|position| {
        window.available_monitors().ok().is_some_and(|monitors| {
            monitors.iter().any(|monitor| {
                let origin = monitor.position();
                let size = monitor.size();
                position.x + 80 > origin.x
                    && position.y + 60 > origin.y
                    && position.x < origin.x + size.width as i32 - 40
                    && position.y < origin.y + size.height as i32 - 40
            })
        })
    });
    if !visible {
        let _ = window.center();
    }
}

fn start_reminder_scheduler(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        let state = app.state::<AppState>();
        if state.exiting.load(Ordering::SeqCst) {
            break;
        }
        let Ok(_permit) = state.persistence_gate.try_operation() else {
            continue;
        };
        let settings = match state.database.settings() {
            Ok(settings) => settings,
            Err(_) => continue,
        };
        if should_remind(&settings, state.database.reminder_marker().ok().flatten())
            && !foreground_is_fullscreen()
        {
            let marker = Local::now().format("%Y-%m-%d-%H:%M").to_string();
            let shown = show_reminder_notification(&app);
            if shown {
                let _ = state.database.save_reminder_marker(&marker);
            }
        }
    });
}

#[cfg(target_os = "windows")]
fn show_reminder_notification(app: &AppHandle) -> bool {
    let mut notification = notify_rust::Notification::new();
    notification
        .summary("Tell You Why")
        .body("有空时，来看看一个值得想一想的问题。");
    if !tauri::is_dev() {
        notification.app_id(&app.config().identifier);
    }
    match notification.show() {
        Ok(handle) => {
            let app = app.clone();
            std::thread::spawn(move || {
                let _ = handle.wait_for_response(
                    move |response: &notify_rust::NotificationResponse| {
                        if response.is_default_action() {
                            show_window(&app);
                        }
                    },
                );
            });
            true
        }
        Err(_) => false,
    }
}

#[cfg(not(target_os = "windows"))]
fn show_reminder_notification(app: &AppHandle) -> bool {
    app.notification()
        .builder()
        .title("Tell You Why")
        .body("有空时，来看看一个值得想一想的问题。")
        .show()
        .is_ok()
}

fn should_remind(settings: &AppSettings, last_marker: Option<String>) -> bool {
    if settings.reminder_preset == ReminderPreset::Manual {
        return false;
    }
    if settings
        .pause_until
        .as_ref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|until| until.with_timezone(&Utc) > Utc::now())
    {
        return false;
    }
    let now = Local::now();
    if !settings
        .weekdays
        .contains(&now.weekday().number_from_monday())
    {
        return false;
    }
    let time = format!("{:02}:{:02}", now.hour(), now.minute());
    if in_quiet_hours(&time, &settings.quiet_start, &settings.quiet_end) {
        return false;
    }
    let scheduled = settings.reminder_times.iter().any(|value| value == &time);
    let marker = now.format("%Y-%m-%d-%H:%M").to_string();
    scheduled && last_marker.as_deref() != Some(marker.as_str())
}

fn in_quiet_hours(now: &str, start: &str, end: &str) -> bool {
    if start == end {
        return false;
    }
    if start < end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}

#[cfg(target_os = "windows")]
fn foreground_is_fullscreen() -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, IsWindowVisible,
    };

    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() || !IsWindowVisible(window).as_bool() {
            return false;
        }
        let mut window_rect = RECT::default();
        if GetWindowRect(window, &mut window_rect).is_err() {
            return false;
        }
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            window_rect.left <= info.rcMonitor.left
                && window_rect.top <= info.rcMonitor.top
                && window_rect.right >= info.rcMonitor.right
                && window_rect.bottom >= info.rcMonitor.bottom
        } else {
            false
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn foreground_is_fullscreen() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn overnight_quiet_hours_are_supported() {
        assert!(in_quiet_hours("23:00", "18:00", "09:00"));
        assert!(in_quiet_hours("08:30", "18:00", "09:00"));
        assert!(!in_quiet_hours("12:00", "18:00", "09:00"));
    }

    #[test]
    fn auto_hide_tip_is_marked_only_after_a_successful_notification() {
        let marks = Cell::new(0);

        mark_tip_after_notification_success(Err::<(), ()>(()), || marks.set(marks.get() + 1));
        assert_eq!(marks.get(), 0);

        mark_tip_after_notification_success(Ok::<(), ()>(()), || marks.set(marks.get() + 1));
        assert_eq!(marks.get(), 1);
    }
}
