//! The pill's window on macOS: a borderless, non-activating panel above every
//! window and space, that lets every click through to the app underneath.
//!
//! AppKit only works on the main thread, and iced owns that one. So the pill
//! has a thread of its own that steps `Look` and draws each frame, exactly as
//! on Windows, and hands the finished frame to the main thread through Grand
//! Central Dispatch, where it becomes the panel's image. Idle, that thread
//! sleeps on its channel and nothing is drawn.
//!
//! Unlike Windows, it takes no clicks yet, so it shows no buttons: hands-free
//! stops with the shortcut or Esc, Retry is paste last, and the fixes are in
//! likh's window.

use super::{CANVAS, Command, Look};
use dispatch2::DispatchQueue;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSColor, NSImage, NSImageScaling, NSImageView, NSPanel, NSScreen, NSStatusWindowLevel, NSWindowCollectionBehavior,
    NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSData, NSPoint, NSRect, NSSize};
use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

const FRAME: Duration = Duration::from_millis(16);
/// Frames are drawn at this many pixels per point: sharp on Retina, scaled down elsewhere.
const SCALE: f32 = 2.0;

thread_local! {
    // Lives on the main thread: the panel and the view showing the frames.
    static PANEL: RefCell<Option<(objc2::rc::Retained<NSPanel>, objc2::rc::Retained<NSImageView>)>> =
        const { RefCell::new(None) };
}

/// The system's "Reduce motion" (Accessibility, Display).
pub fn animations_on() -> bool {
    !NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// Starts the pill's thread. Commands arrive on `commands`; it sleeps between them.
pub fn spawn(commands: Receiver<Command>) -> Result<(), String> {
    std::thread::Builder::new()
        .name("likh-pill".into())
        .spawn(move || run(&commands))
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn run(commands: &Receiver<Command>) {
    let mut look = Look::default();
    let mut moving = false;
    let mut last = Instant::now();
    loop {
        // Still: sleep until the next command. Moving: a frame every 16 ms.
        let first = if moving {
            match commands.recv_timeout(FRAME) {
                Ok(command) => Some(command),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => return,
            }
        };
        if first.is_some() && !moving {
            last = Instant::now();
        }
        // Everything waiting goes in before one frame is drawn: the mic's levels
        // arrive about 33 times a second, and a frame each fell behind (CI's Mac, 2026-10-22).
        for command in first.into_iter().chain(commands.try_iter()) {
            match command {
                Command::Show(show) => {
                    if look.is_hidden() && show != super::Show::Hidden {
                        look.hold_still(!animations_on());
                        DispatchQueue::main().exec_async(place);
                    }
                    look.set(without_buttons(show));
                }
                Command::Level(level) => look.level(level),
            }
        }
        // Real time, in steps the springs handle, however long the last frame
        // took: clamping it once made a 6 s error stay 24 s in a debug build.
        let now = Instant::now();
        let mut behind = (now - last).as_secs_f32().min(1.0);
        moving = false;
        while behind > 0.0 {
            moving = look.step(behind.min(0.05));
            behind -= 0.05;
        }
        last = now;
        if let Some(pixmap) = look.draw(SCALE)
            && let Ok(png) = pixmap.encode_png()
        {
            deliver(Frame::Image(png));
        }
        if !moving && look.is_hidden() {
            deliver(Frame::Hide);
        }
    }
}

/// The macOS pill takes no clicks, so a problem shows as its error, without a
/// button that would do nothing.
fn without_buttons(show: super::Show) -> super::Show {
    match show {
        super::Show::Problem { text, .. } => super::Show::Error(text),
        other => other,
    }
}

/// What the main thread shows next.
enum Frame {
    Image(Vec<u8>),
    Hide,
}

/// Only the newest frame waits for the main thread; one it hasn't shown yet is
/// replaced. Queueing every frame put the pill seconds behind.
static LATEST: Mutex<Option<Frame>> = Mutex::new(None);
/// A block that shows `LATEST` is already queued on the main thread.
static QUEUED: AtomicBool = AtomicBool::new(false);

fn deliver(frame: Frame) {
    *LATEST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(frame);
    if !QUEUED.swap(true, Ordering::SeqCst) {
        DispatchQueue::main().exec_async(|| {
            QUEUED.store(false, Ordering::SeqCst);
            match LATEST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take() {
                Some(Frame::Image(png)) => present(&png),
                Some(Frame::Hide) => hide(),
                None => {}
            }
        });
    }
}

/// The panel, made the first time the pill shows.
fn with_panel(mtm: MainThreadMarker, then: impl FnOnce(&NSPanel, &NSImageView)) {
    PANEL.with(|slot| {
        let mut slot = slot.borrow_mut();
        let (panel, view) = slot.get_or_insert_with(|| {
            let panel = NSPanel::new(mtm);
            panel.setStyleMask(NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel);
            panel.setOpaque(false);
            panel.setBackgroundColor(Some(&NSColor::clearColor()));
            panel.setHasShadow(false);
            // A panel hides whenever its app isn't the active one, and likh never is:
            // the pill must not take focus. Without this it never showed (CI's Mac, 2026-10-22).
            panel.setHidesOnDeactivate(false);
            panel.setFloatingPanel(true);
            panel.setLevel(NSStatusWindowLevel);
            panel.setIgnoresMouseEvents(true);
            panel.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            let view = NSImageView::new(mtm);
            view.setImageScaling(NSImageScaling::ScaleAxesIndependently);
            panel.setContentView(Some(&view));
            (panel, view)
        });
        then(panel, view);
    });
}

/// Bottom centre of the screen's visible area (above the Dock), and on screen.
fn place() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(screen) = NSScreen::mainScreen(mtm) else { return };
    let area = screen.visibleFrame();
    let (w, h) = (f64::from(CANVAS.0), f64::from(CANVAS.1));
    // The canvas leaves the pill's own gap under it, so it sits on the Dock's edge like on the taskbar's.
    let origin = NSPoint::new(area.origin.x + (area.size.width - w) / 2.0, area.origin.y);
    with_panel(mtm, |panel, _| {
        panel.setFrame_display(NSRect::new(origin, NSSize::new(w, h)), false);
        panel.orderFrontRegardless();
        if !panel.isVisible() {
            crate::diagnostics::warn("pill", "the pill's panel didn't come on screen");
        }
    });
}

/// Shows one frame, a PNG of the canvas at `SCALE`.
fn present(png: &[u8]) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(png)) else { return };
    image.setSize(NSSize::new(f64::from(CANVAS.0), f64::from(CANVAS.1)));
    with_panel(mtm, |_, view| view.setImage(Some(&image)));
}

fn hide() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    with_panel(mtm, |panel, _| panel.orderOut(None));
}
