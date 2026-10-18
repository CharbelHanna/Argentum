//! The viewer surface across minimise and restore. Ours.
//!
//! The photo is drawn by wgpu onto the main window, under a transparent
//! webview. Wherever the surface has no presented frame the window is
//! see-through, and the viewer shows whatever is behind Argentum. After a
//! minimise and restore it did exactly that.
//!
//! WHAT MINIMISING DOES
//!
//! Windows reports a size of 0x0 and moves the window to -32000,-32000. The
//! resize handler used to reconfigure the surface to 1x1, which threw the photo
//! away. The move also starts `display_monitor::refresh_after_window_change`,
//! which can draw while the window is minimised, when a Vulkan surface has no
//! size at all. Skipping the 1x1 alone fixed the first restore, not the second.
//!
//! So nothing is drawn while minimised, and a restore reconfigures the surface
//! and draws again, then a few more times over the next half second in case the
//! first frame came too early to be shown.
//!
//! The `[viewer]` log lines say which of those steps happened. They go once the
//! cause is confirmed.

use std::sync::TryLockError;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tauri::Manager;
use tauri::PhysicalSize;

use crate::AppState;
use crate::gpu_processing::WgpuDisplay;
use crate::image_processing::GpuContext;

/// When the viewer is drawn again after a restore, counted from the restore.
const REDRAW_AFTER_RESTORE_MS: [u64; 3] = [50, 200, 500];

static MINIMISED: AtomicBool = AtomicBool::new(false);

/// The window's latest non-zero size, width in the high half, for the redraws
/// that run after it may have changed again.
static LATEST_SIZE: AtomicU64 = AtomicU64::new(0);

/// Whether the main window is minimised. Nothing may be drawn to it then.
pub fn is_minimised() -> bool {
    MINIMISED.load(Ordering::Relaxed)
}

/// The main window's `Resized` event: fit the surface to the window and redraw.
pub fn apply_window_resize(window: &tauri::Window, size: PhysicalSize<u32>) {
    if size.width == 0 || size.height == 0 {
        if !MINIMISED.swap(true, Ordering::Relaxed) {
            log::info!("[viewer] minimised");
        }
        return;
    }
    LATEST_SIZE.store(
        (u64::from(size.width) << 32) | u64::from(size.height),
        Ordering::Relaxed,
    );
    let restored = MINIMISED.swap(false, Ordering::Relaxed);

    let state = window.state::<AppState>();
    let Some(ctx) = state
        .gpu_context
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    else {
        return;
    };

    // Never wait here: this is the event thread, and a render may hold the lock.
    let mut guard = match ctx.display.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            if restored {
                log::info!("[viewer] restored, display busy");
                redraw_after_restore(ctx.clone(), true);
            }
            return;
        }
    };
    let Some(display) = guard.as_mut() else {
        return;
    };
    let presented = fit_and_render(display, &ctx, size, restored);
    drop(guard);

    if restored {
        log::info!(
            "[viewer] restored {}x{}, frame presented: {presented}",
            size.width,
            size.height
        );
        redraw_after_restore(ctx, false);
    }
}

fn latest_size() -> PhysicalSize<u32> {
    let packed = LATEST_SIZE.load(Ordering::Relaxed);
    PhysicalSize::new((packed >> 32) as u32, packed as u32)
}

/// Fits the surface to `size` and draws; returns whether a frame was presented.
/// `reconfigure` forces it at the same size, for a swapchain that may have gone
/// stale while the window was minimised.
fn fit_and_render(
    display: &mut WgpuDisplay,
    ctx: &GpuContext,
    size: PhysicalSize<u32>,
    reconfigure: bool,
) -> bool {
    if reconfigure || display.config.width != size.width || display.config.height != size.height
    {
        display.config.width = size.width;
        display.config.height = size.height;
        display.surface.configure(&ctx.device, &display.config);
    }
    display.render(&ctx.device, &ctx.queue)
}

/// Draws the viewer again at each of `REDRAW_AFTER_RESTORE_MS`, off the event
/// thread. `reconfigure` when the restore itself could not reach the surface.
fn redraw_after_restore(ctx: GpuContext, mut reconfigure: bool) {
    std::thread::spawn(move || {
        let mut slept_ms = 0;
        for at_ms in REDRAW_AFTER_RESTORE_MS {
            std::thread::sleep(Duration::from_millis(at_ms - slept_ms));
            slept_ms = at_ms;
            if is_minimised() {
                return;
            }
            let mut guard = ctx.display.lock().unwrap_or_else(|e| e.into_inner());
            let Some(display) = guard.as_mut() else {
                return;
            };
            let presented = fit_and_render(display, &ctx, latest_size(), reconfigure);
            reconfigure = false;
            log::info!("[viewer] redraw {at_ms} ms after restore, frame presented: {presented}");
        }
    });
}
