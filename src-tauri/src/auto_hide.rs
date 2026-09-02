use crate::{desktop, AppState};
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, WebviewWindow};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const LEAVE_DELAY: Duration = Duration::from_millis(700);
const SHOW_GUARD: Duration = Duration::from_millis(1_000);
const INTERACTION_GUARD: Duration = Duration::from_millis(900);
const EDGE_TOLERANCE_PHYSICAL: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoHidePhase {
    Disabled,
    WaitingForEntry,
    Armed,
    LeavePending,
    HideRequested,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AutoHideRequest {
    epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowPointerSnapshot {
    pub visible: bool,
    pub minimized: bool,
    pub pointer_inside: bool,
    pub mouse_button_pressed: bool,
}

#[derive(Debug)]
struct AutoHideMachine {
    enabled: bool,
    suspended: bool,
    phase: AutoHidePhase,
    outside_since: Option<Instant>,
    protected_until: Option<Instant>,
    epoch: u64,
}

impl Default for AutoHideMachine {
    fn default() -> Self {
        Self {
            enabled: false,
            suspended: false,
            phase: AutoHidePhase::Disabled,
            outside_since: None,
            protected_until: None,
            epoch: 0,
        }
    }
}

impl AutoHideMachine {
    fn advance_epoch(&mut self) -> u64 {
        self.epoch = self.epoch.wrapping_add(1);
        self.epoch
    }

    fn set_enabled(&mut self, enabled: bool, now: Instant) {
        self.advance_epoch();
        self.enabled = enabled;
        self.outside_since = None;
        if enabled {
            self.phase = AutoHidePhase::WaitingForEntry;
            self.protected_until = Some(now + SHOW_GUARD);
        } else {
            self.phase = AutoHidePhase::Disabled;
            self.protected_until = None;
        }
    }

    fn window_shown(&mut self, now: Instant) {
        if !self.enabled {
            return;
        }
        self.advance_epoch();
        self.phase = AutoHidePhase::WaitingForEntry;
        self.outside_since = None;
        self.protected_until = Some(now + SHOW_GUARD);
    }

    fn window_hidden(&mut self) {
        if !self.enabled {
            return;
        }
        self.advance_epoch();
        self.phase = AutoHidePhase::Hidden;
        self.outside_since = None;
        self.protected_until = None;
    }

    fn protect_for(&mut self, now: Instant, duration: Duration) {
        if !self.enabled {
            return;
        }
        self.protected_until = Some(now + duration);
        self.cancel_pending_leave();
    }

    fn set_suspended(&mut self, suspended: bool) {
        self.suspended = suspended;
        if suspended {
            self.cancel_pending_leave();
        }
    }

    fn cancel_pending_leave(&mut self) {
        self.outside_since = None;
        if matches!(
            self.phase,
            AutoHidePhase::LeavePending | AutoHidePhase::HideRequested
        ) {
            self.advance_epoch();
            self.phase = AutoHidePhase::Armed;
        }
    }

    fn tick(
        &mut self,
        now: Instant,
        visible: bool,
        minimized: bool,
        pointer_inside: bool,
        mouse_button_pressed: bool,
    ) -> Option<AutoHideRequest> {
        if !self.enabled {
            self.phase = AutoHidePhase::Disabled;
            return None;
        }

        if !visible || minimized {
            self.window_hidden();
            return None;
        }

        if self.phase == AutoHidePhase::Hidden {
            self.window_shown(now);
        }

        if self.phase == AutoHidePhase::WaitingForEntry && pointer_inside {
            self.phase = AutoHidePhase::Armed;
        }

        let protected = self
            .protected_until
            .is_some_and(|protected_until| now < protected_until);
        if self.suspended || protected || mouse_button_pressed {
            self.cancel_pending_leave();
            return None;
        }
        self.protected_until = None;

        match self.phase {
            AutoHidePhase::Disabled | AutoHidePhase::HideRequested | AutoHidePhase::Hidden => None,
            AutoHidePhase::WaitingForEntry => None,
            AutoHidePhase::Armed => {
                if pointer_inside {
                    None
                } else {
                    self.phase = AutoHidePhase::LeavePending;
                    self.outside_since = Some(now);
                    None
                }
            }
            AutoHidePhase::LeavePending => {
                if pointer_inside {
                    self.phase = AutoHidePhase::Armed;
                    self.outside_since = None;
                    return None;
                }
                if self
                    .outside_since
                    .is_some_and(|outside_since| now.duration_since(outside_since) >= LEAVE_DELAY)
                {
                    let request = AutoHideRequest {
                        epoch: self.advance_epoch(),
                    };
                    self.phase = AutoHidePhase::HideRequested;
                    self.outside_since = None;
                    Some(request)
                } else {
                    None
                }
            }
        }
    }

    fn confirm_hide(
        &mut self,
        request: AutoHideRequest,
        now: Instant,
        snapshot: WindowPointerSnapshot,
    ) -> bool {
        if !self.enabled {
            self.phase = AutoHidePhase::Disabled;
            return false;
        }
        if self.phase != AutoHidePhase::HideRequested || self.epoch != request.epoch {
            return false;
        }
        if !snapshot.visible || snapshot.minimized {
            self.window_hidden();
            return false;
        }
        let protected = self
            .protected_until
            .is_some_and(|protected_until| now < protected_until);
        if self.suspended || protected || snapshot.pointer_inside || snapshot.mouse_button_pressed {
            self.advance_epoch();
            self.phase = AutoHidePhase::Armed;
            self.outside_since = None;
            return false;
        }

        self.advance_epoch();
        self.phase = AutoHidePhase::Hidden;
        self.outside_since = None;
        self.protected_until = None;
        true
    }
}

#[derive(Debug, Default)]
pub struct AutoHideController {
    machine: Mutex<AutoHideMachine>,
}

impl AutoHideController {
    fn with_machine<T>(&self, callback: impl FnOnce(&mut AutoHideMachine) -> T) -> T {
        let mut machine = self
            .machine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        callback(&mut machine)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.with_machine(|machine| machine.set_enabled(enabled, Instant::now()));
    }

    pub fn is_enabled(&self) -> bool {
        self.with_machine(|machine| machine.enabled)
    }

    pub fn window_shown(&self) {
        self.with_machine(|machine| machine.window_shown(Instant::now()));
    }

    pub fn window_hidden(&self) {
        self.with_machine(AutoHideMachine::window_hidden);
    }

    pub fn protect_for_interaction(&self) {
        self.with_machine(|machine| {
            machine.protect_for(Instant::now(), INTERACTION_GUARD);
        });
    }

    pub fn set_suspended(&self, suspended: bool) {
        self.with_machine(|machine| machine.set_suspended(suspended));
    }

    fn tick(
        &self,
        visible: bool,
        minimized: bool,
        pointer_inside: bool,
        mouse_button_pressed: bool,
    ) -> Option<AutoHideRequest> {
        self.with_machine(|machine| {
            machine.tick(
                Instant::now(),
                visible,
                minimized,
                pointer_inside,
                mouse_button_pressed,
            )
        })
    }

    pub(crate) fn confirm_hide(
        &self,
        request: AutoHideRequest,
        snapshot: WindowPointerSnapshot,
    ) -> bool {
        self.with_machine(|machine| machine.confirm_hide(request, Instant::now(), snapshot))
    }
}

pub fn start_monitor(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("tell-you-why-auto-hide".to_string())
        .spawn(move || loop {
            std::thread::sleep(POLL_INTERVAL);
            let state = app.state::<AppState>();
            if state.exiting.load(Ordering::SeqCst) {
                break;
            }
            if !state.auto_hide.is_enabled() {
                continue;
            }
            let Some(window) = app.get_webview_window("main") else {
                continue;
            };
            let Some(snapshot) = window_pointer_snapshot(&app, &window) else {
                state.auto_hide.protect_for_interaction();
                continue;
            };

            if let Some(request) = state.auto_hide.tick(
                snapshot.visible,
                snapshot.minimized,
                snapshot.pointer_inside,
                snapshot.mouse_button_pressed,
            ) {
                desktop::hide_window_for_auto_hide(&window, request);
            }
        });
}

pub(crate) fn window_pointer_snapshot(
    app: &AppHandle,
    window: &WebviewWindow,
) -> Option<WindowPointerSnapshot> {
    let visible = window.is_visible().ok()?;
    let minimized = window.is_minimized().ok()?;
    let pointer_inside = if visible && !minimized {
        let cursor = app.cursor_position().ok()?;
        let position = window.outer_position().ok()?;
        let size = window.outer_size().ok()?;
        point_inside_window(
            cursor.x,
            cursor.y,
            position.x,
            position.y,
            size.width,
            size.height,
        )
    } else {
        false
    };
    Some(WindowPointerSnapshot {
        visible,
        minimized,
        pointer_inside,
        mouse_button_pressed: any_mouse_button_pressed(),
    })
}

fn key_state_is_pressed(state: i16) -> bool {
    state < 0
}

#[cfg(target_os = "windows")]
fn any_mouse_button_pressed() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
    };

    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2]
        .into_iter()
        .any(|button| {
            // The high-order bit is set while the physical button is currently down.
            key_state_is_pressed(unsafe { GetAsyncKeyState(i32::from(button.0)) })
        })
}

#[cfg(not(target_os = "windows"))]
fn any_mouse_button_pressed() -> bool {
    false
}

fn point_inside_window(
    cursor_x: f64,
    cursor_y: f64,
    window_x: i32,
    window_y: i32,
    window_width: u32,
    window_height: u32,
) -> bool {
    let tolerance = EDGE_TOLERANCE_PHYSICAL;
    let left = window_x as f64 - tolerance;
    let top = window_y as f64 - tolerance;
    let right = window_x as f64 + window_width as f64 + tolerance;
    let bottom = window_y as f64 + window_height as f64 + tolerance;
    cursor_x >= left && cursor_x <= right && cursor_y >= top && cursor_y <= bottom
}

#[cfg(test)]
mod tests {
    use super::{
        key_state_is_pressed, point_inside_window, AutoHideMachine, AutoHidePhase, AutoHideRequest,
        WindowPointerSnapshot, INTERACTION_GUARD, LEAVE_DELAY, SHOW_GUARD,
    };
    use std::time::{Duration, Instant};

    fn enabled_machine(start: Instant) -> AutoHideMachine {
        let mut machine = AutoHideMachine::default();
        machine.set_enabled(true, start);
        machine
    }

    fn visible_snapshot(pointer_inside: bool) -> WindowPointerSnapshot {
        WindowPointerSnapshot {
            visible: true,
            minimized: false,
            pointer_inside,
            mouse_button_pressed: false,
        }
    }

    fn request_after_leave(
        machine: &mut AutoHideMachine,
        start: Instant,
    ) -> (AutoHideRequest, Instant) {
        let armed_at = start + SHOW_GUARD;
        assert!(machine.tick(armed_at, true, false, true, false).is_none());
        let left_at = armed_at + Duration::from_millis(1);
        assert!(machine.tick(left_at, true, false, false, false).is_none());
        let requested_at = left_at + LEAVE_DELAY;
        let request = machine
            .tick(requested_at, true, false, false, false)
            .expect("elapsed leave creates a hide request");
        assert_eq!(machine.phase, AutoHidePhase::HideRequested);
        (request, requested_at)
    }

    #[test]
    fn disabled_machine_never_hides() {
        let start = Instant::now();
        let mut machine = AutoHideMachine::default();
        assert!(machine
            .tick(start + Duration::from_secs(10), true, false, false, false)
            .is_none());
        assert_eq!(machine.phase, AutoHidePhase::Disabled);
    }

    #[test]
    fn window_must_be_entered_before_a_timed_leave_can_hide_it() {
        let start = Instant::now();
        let mut machine = enabled_machine(start);
        let after_guard = start + SHOW_GUARD;

        assert!(machine
            .tick(after_guard, true, false, false, false)
            .is_none());
        assert_eq!(machine.phase, AutoHidePhase::WaitingForEntry);

        assert!(machine
            .tick(
                after_guard + Duration::from_millis(1),
                true,
                false,
                true,
                false,
            )
            .is_none());
        let left_at = after_guard + Duration::from_millis(2);
        assert!(machine.tick(left_at, true, false, false, false).is_none());
        assert!(machine
            .tick(
                left_at + LEAVE_DELAY - Duration::from_millis(1),
                true,
                false,
                false,
                false,
            )
            .is_none());
        let requested_at = left_at + LEAVE_DELAY;
        let request = machine
            .tick(requested_at, true, false, false, false)
            .expect("full leave delay creates a request");
        assert_eq!(machine.phase, AutoHidePhase::HideRequested);
        assert!(machine.confirm_hide(request, requested_at, visible_snapshot(false)));
        assert_eq!(machine.phase, AutoHidePhase::Hidden);
        assert!(!machine.confirm_hide(request, requested_at, visible_snapshot(false)));
    }

    #[test]
    fn reentering_or_suspending_cancels_the_pending_leave() {
        let start = Instant::now();
        let mut machine = enabled_machine(start);
        let after_guard = start + SHOW_GUARD;
        machine.tick(after_guard, true, false, true, false);
        machine.tick(
            after_guard + Duration::from_millis(1),
            true,
            false,
            false,
            false,
        );

        assert!(machine
            .tick(after_guard + LEAVE_DELAY, true, false, true, false)
            .is_none());
        assert_eq!(machine.phase, AutoHidePhase::Armed);

        machine.tick(
            after_guard + LEAVE_DELAY + Duration::from_millis(1),
            true,
            false,
            false,
            false,
        );
        machine.set_suspended(true);
        assert!(machine
            .tick(after_guard + LEAVE_DELAY * 2, true, false, false, false)
            .is_none());
        assert_eq!(machine.phase, AutoHidePhase::Armed);

        machine.set_suspended(false);
        let resumed_at = after_guard + LEAVE_DELAY * 2 + Duration::from_millis(1);
        assert!(machine
            .tick(resumed_at, true, false, false, false)
            .is_none());
        assert!(machine
            .tick(
                resumed_at + LEAVE_DELAY - Duration::from_millis(1),
                true,
                false,
                false,
                false,
            )
            .is_none());
        assert!(machine
            .tick(resumed_at + LEAVE_DELAY, true, false, false, false)
            .is_some());
    }

    #[test]
    fn stale_hide_request_is_invalidated_by_suspension_or_disabling() {
        let start = Instant::now();
        let mut suspended_machine = enabled_machine(start);
        let (suspended_request, requested_at) = request_after_leave(&mut suspended_machine, start);
        suspended_machine.set_suspended(true);
        assert!(!suspended_machine.confirm_hide(
            suspended_request,
            requested_at,
            visible_snapshot(false),
        ));
        assert_eq!(suspended_machine.phase, AutoHidePhase::Armed);

        let mut disabled_machine = enabled_machine(start);
        let (disabled_request, requested_at) = request_after_leave(&mut disabled_machine, start);
        disabled_machine.set_enabled(false, requested_at);
        assert!(!disabled_machine.confirm_hide(
            disabled_request,
            requested_at,
            visible_snapshot(false),
        ));
        assert_eq!(disabled_machine.phase, AutoHidePhase::Disabled);
    }

    #[test]
    fn final_revalidation_cancels_hide_after_pointer_return_or_interaction() {
        let start = Instant::now();
        let mut returned_machine = enabled_machine(start);
        let (returned_request, requested_at) = request_after_leave(&mut returned_machine, start);
        assert!(!returned_machine.confirm_hide(
            returned_request,
            requested_at,
            visible_snapshot(true),
        ));
        assert_eq!(returned_machine.phase, AutoHidePhase::Armed);

        let mut protected_machine = enabled_machine(start);
        let (protected_request, requested_at) = request_after_leave(&mut protected_machine, start);
        protected_machine.protect_for(requested_at, INTERACTION_GUARD);
        assert!(!protected_machine.confirm_hide(
            protected_request,
            requested_at,
            visible_snapshot(false),
        ));
        assert_eq!(protected_machine.phase, AutoHidePhase::Armed);

        let mut pressed_machine = enabled_machine(start);
        let (pressed_request, requested_at) = request_after_leave(&mut pressed_machine, start);
        let mut pressed_snapshot = visible_snapshot(false);
        pressed_snapshot.mouse_button_pressed = true;
        assert!(!pressed_machine.confirm_hide(pressed_request, requested_at, pressed_snapshot));
        assert_eq!(pressed_machine.phase, AutoHidePhase::Armed);
    }

    #[test]
    fn held_mouse_button_blocks_leave_timing_until_it_is_released() {
        let start = Instant::now();
        let mut machine = enabled_machine(start);
        let armed_at = start + SHOW_GUARD;
        assert!(machine.tick(armed_at, true, false, true, false).is_none());

        let held_at = armed_at + Duration::from_millis(1);
        assert!(machine.tick(held_at, true, false, false, true).is_none());
        assert_eq!(machine.phase, AutoHidePhase::Armed);
        assert!(machine
            .tick(held_at + Duration::from_secs(10), true, false, false, true)
            .is_none());
        assert_eq!(machine.phase, AutoHidePhase::Armed);

        let released_at = held_at + Duration::from_secs(10) + Duration::from_millis(1);
        assert!(machine
            .tick(released_at, true, false, false, false)
            .is_none());
        assert_eq!(machine.phase, AutoHidePhase::LeavePending);
        assert!(machine
            .tick(
                released_at + LEAVE_DELAY - Duration::from_millis(1),
                true,
                false,
                false,
                false,
            )
            .is_none());
        assert!(machine
            .tick(released_at + LEAVE_DELAY, true, false, false, false)
            .is_some());
    }

    #[test]
    fn showing_a_hidden_window_requires_a_fresh_entry() {
        let start = Instant::now();
        let mut machine = enabled_machine(start);
        machine.window_hidden();
        assert!(machine
            .tick(start + Duration::from_secs(2), true, false, false, false)
            .is_none());
        assert_eq!(machine.phase, AutoHidePhase::WaitingForEntry);
    }

    #[test]
    fn geometry_supports_physical_tolerance_and_negative_monitors() {
        assert!(point_inside_window(-1_928.0, 100.0, -1_920, 100, 420, 560));
        assert!(!point_inside_window(-1_929.0, 100.0, -1_920, 100, 420, 560));
        assert!(point_inside_window(2_108.0, 100.0, 1_700, 100, 400, 500));
        assert!(!point_inside_window(2_109.0, 100.0, 1_700, 100, 400, 500));
    }

    #[test]
    fn async_key_state_uses_only_the_current_down_bit() {
        assert!(!key_state_is_pressed(0));
        assert!(!key_state_is_pressed(1));
        assert!(key_state_is_pressed(i16::MIN));
        assert!(key_state_is_pressed(-1));
    }
}
