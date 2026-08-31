use std::sync::{Arc, Mutex, MutexGuard};

const RESET_IN_PROGRESS_ERROR: &str = "正在清除本地数据，请稍后再试";
const OPERATIONS_IN_PROGRESS_ERROR: &str = "已有数据操作正在进行，请稍后重试清除";
const OPERATION_LIMIT_ERROR: &str = "当前数据操作过多，请稍后再试";

#[derive(Debug, Default)]
struct GateState {
    active_operations: usize,
    reset_active: bool,
}

#[derive(Debug, Default)]
struct GateInner {
    state: Mutex<GateState>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PersistenceResetGate {
    inner: Arc<GateInner>,
}

#[derive(Debug)]
#[must_use = "dropping the operation permit releases its reset exclusion"]
pub(crate) struct OperationPermit {
    inner: Arc<GateInner>,
}

#[derive(Debug)]
#[must_use = "dropping the reset permit allows persistence operations again"]
pub(crate) struct ResetPermit {
    inner: Arc<GateInner>,
}

impl PersistenceResetGate {
    pub(crate) fn try_operation(&self) -> Result<OperationPermit, &'static str> {
        let mut state = lock_state(&self.inner);
        if state.reset_active {
            return Err(RESET_IN_PROGRESS_ERROR);
        }
        state.active_operations = state
            .active_operations
            .checked_add(1)
            .ok_or(OPERATION_LIMIT_ERROR)?;
        Ok(OperationPermit {
            inner: Arc::clone(&self.inner),
        })
    }

    pub(crate) fn try_reset(&self) -> Result<ResetPermit, &'static str> {
        let mut state = lock_state(&self.inner);
        if state.reset_active || state.active_operations != 0 {
            return Err(OPERATIONS_IN_PROGRESS_ERROR);
        }
        state.reset_active = true;
        Ok(ResetPermit {
            inner: Arc::clone(&self.inner),
        })
    }
}

impl Drop for OperationPermit {
    fn drop(&mut self) {
        let mut state = lock_state(&self.inner);
        debug_assert!(state.active_operations > 0);
        state.active_operations = state.active_operations.saturating_sub(1);
    }
}

impl Drop for ResetPermit {
    fn drop(&mut self) {
        let mut state = lock_state(&self.inner);
        debug_assert!(state.reset_active);
        state.reset_active = false;
    }
}

fn lock_state(inner: &GateInner) -> MutexGuard<'_, GateState> {
    inner
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{
        OperationPermit, PersistenceResetGate, ResetPermit, OPERATIONS_IN_PROGRESS_ERROR,
        RESET_IN_PROGRESS_ERROR,
    };
    use std::future::Future;

    fn assert_send<T: Send>() {}

    fn assert_send_future<F: Future + Send>(_: F) {}

    #[test]
    fn reset_is_rejected_until_every_active_operation_is_released() {
        let gate = PersistenceResetGate::default();
        let first = gate.try_operation().expect("first operation");
        let second = gate.try_operation().expect("second operation");

        assert_eq!(
            gate.try_reset().expect_err("active operations block reset"),
            OPERATIONS_IN_PROGRESS_ERROR
        );
        drop(first);
        assert!(gate.try_reset().is_err());
        drop(second);

        let reset = gate.try_reset().expect("reset after all operations");
        drop(reset);
        assert!(gate.try_operation().is_ok());
    }

    #[test]
    fn active_reset_rejects_operations_until_its_permit_is_dropped() {
        let gate = PersistenceResetGate::default();
        let reset = gate.try_reset().expect("reset permit");

        assert_eq!(
            gate.try_operation()
                .expect_err("reset blocks persistence operations"),
            RESET_IN_PROGRESS_ERROR
        );
        assert!(gate.try_reset().is_err());

        drop(reset);
        assert!(gate.try_operation().is_ok());
    }

    #[test]
    fn permits_are_send_and_release_the_gate_from_another_thread() {
        assert_send::<OperationPermit>();
        assert_send::<ResetPermit>();

        let gate = PersistenceResetGate::default();
        let permit = gate.try_operation().expect("operation permit");
        std::thread::spawn(move || drop(permit))
            .join()
            .expect("permit drop thread");

        assert!(gate.try_reset().is_ok());
    }

    #[test]
    fn operation_permit_can_live_across_an_await_in_a_send_future() {
        let gate = PersistenceResetGate::default();
        assert_send_future(async move {
            let permit = gate.try_operation().expect("operation permit");
            std::future::ready(()).await;
            drop(permit);
        });
    }
}
