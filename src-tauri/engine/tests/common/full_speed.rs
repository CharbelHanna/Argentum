//! Keeps a timing test on every core, whichever window has focus.
//!
//! On a CPU with P-cores and E-cores, Windows moves a test like this onto the
//! E-cores while another window has focus. Work shared across the cores then
//! takes 2.2 times as long, which reads as a slowdown in the code. High QoS,
//! which a process asks for itself, keeps it on every core. Shared by the
//! engine's tests and, through `#[path]`, the app's `speed_budgets.rs`, so
//! the app itself never asks.

#[cfg(windows)]
pub use win32::{held, hold};

/// Nothing to ask for off Windows.
#[cfg(not(windows))]
pub fn hold() {}

#[cfg(windows)]
mod win32 {
    use std::ffi::c_void;

    /// `ProcessPowerThrottling` of `PROCESS_INFORMATION_CLASS`.
    const PROCESS_POWER_THROTTLING: i32 = 4;
    /// `PROCESS_POWER_THROTTLING_EXECUTION_SPEED`.
    const EXECUTION_SPEED: u32 = 1;

    /// `PROCESS_POWER_THROTTLING_STATE`.
    #[repr(C)]
    struct Throttling {
        version: u32,
        control_mask: u32,
        state_mask: u32,
    }

    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessInformation(
            process: *mut c_void,
            class: i32,
            information: *mut c_void,
            size: u32,
        ) -> i32;
        fn SetProcessInformation(
            process: *mut c_void,
            class: i32,
            information: *const c_void,
            size: u32,
        ) -> i32;
    }

    /// Asks Windows to leave this process on every core: the control bit set,
    /// which says we decide, and the state bit clear, which says at full speed.
    /// A refusal is said, not failed: it only means the run may be slowed.
    pub fn hold() {
        let state = Throttling {
            version: 1,
            control_mask: EXECUTION_SPEED,
            state_mask: 0,
        };
        // SAFETY: the current process's pseudo-handle, and a struct of the
        // size it says.
        let asked = unsafe {
            SetProcessInformation(
                GetCurrentProcess(),
                PROCESS_POWER_THROTTLING,
                (&raw const state).cast(),
                size_of::<Throttling>() as u32,
            )
        };
        if asked == 0 || !held() {
            eprintln!("full speed: Windows refused, so a window in front can slow this run");
        }
    }

    /// Whether this process has asked for full speed and been given it.
    pub fn held() -> bool {
        let mut state = Throttling {
            version: 1,
            control_mask: 0,
            state_mask: 0,
        };
        // SAFETY: the current process's pseudo-handle, and a struct of the
        // size it says.
        let read = unsafe {
            GetProcessInformation(
                GetCurrentProcess(),
                PROCESS_POWER_THROTTLING,
                (&raw mut state).cast(),
                size_of::<Throttling>() as u32,
            )
        };
        read != 0
            && state.control_mask & EXECUTION_SPEED != 0
            && state.state_mask & EXECUTION_SPEED == 0
    }
}
