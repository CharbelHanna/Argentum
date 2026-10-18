//! The viewer surface across minimise and restore. Ours.
//!
//! The photo is drawn by wgpu onto the main window, under a transparent
//! webview. Wherever the window holds no drawn frame it is see-through, and the
//! viewer shows whatever is behind Argentum. After a minimise and restore it did
//! exactly that, until the next edit drew the photo again.
//!
//! WHAT MINIMISING DOES
//!
//! The window's pixels are thrown away (seen with the OpenGL backend, which
//! draws into the window itself through `SwapBuffers`). The resize handler did
//! draw on restore, but a frame drawn while the window is still coming back is
//! thrown away too.
//!
//! Windows reports 0x0 on minimise, but Tauri passes on its own reading of the
//! window's size instead, which is not zero. So the window is asked whether it
//! is minimised, rather than the size being trusted.
//!
//! So nothing is drawn while minimised, and a restore draws at once and again
//! at each of `REDRAW_AFTER_RESTORE_MS`.
//!
//! The `[viewer]` log lines say which of those steps happened. They go once the
//! fix is confirmed.

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

/// The window's latest size, width in the high half, for the redraws that run
/// after it may have changed again.
static LATEST_SIZE: AtomicU64 = AtomicU64::new(0);

/// Whether the main window is minimised. Nothing may be drawn to it then.
pub fn is_minimised() -> bool {
    MINIMISED.load(Ordering::Relaxed)
}

/// The main window's `Resized` event: fit the surface to the window and redraw.
pub fn apply_window_resize(window: &tauri::Window, size: PhysicalSize<u32>) {
    if window.is_minimized().unwrap_or(false) || size.width == 0 || size.height == 0 {
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
    let presented = match ctx.display.try_lock() {
        Ok(mut guard) => guard.as_mut().map(|d| fit_and_render(d, &ctx, size)),
        Err(TryLockError::Poisoned(poisoned)) => {
            poisoned.into_inner().as_mut().map(|d| fit_and_render(d, &ctx, size))
        }
        Err(TryLockError::WouldBlock) => None,
    };

    if restored {
        log::info!(
            "[viewer] restored {}x{}, frame presented: {presented:?}",
            size.width,
            size.height
        );
        redraw_after_restore(ctx);
    }
}

fn latest_size() -> PhysicalSize<u32> {
    let packed = LATEST_SIZE.load(Ordering::Relaxed);
    PhysicalSize::new((packed >> 32) as u32, packed as u32)
}

/// Fits the surface to `size` and draws; returns whether a frame was presented.
fn fit_and_render(display: &mut WgpuDisplay, ctx: &GpuContext, size: PhysicalSize<u32>) -> bool {
    if display.config.width != size.width || display.config.height != size.height {
        display.config.width = size.width;
        display.config.height = size.height;
        display.surface.configure(&ctx.device, &display.config);
    }
    display.render(&ctx.device, &ctx.queue)
}

/// Draws the viewer again at each of `REDRAW_AFTER_RESTORE_MS`, off the event
/// thread, so one of the frames lands once the window is back on screen.
fn redraw_after_restore(ctx: GpuContext) {
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
            let presented = fit_and_render(display, &ctx, latest_size());
            log::info!("[viewer] redraw {at_ms} ms after restore, frame presented: {presented}");
        }
    });
}
