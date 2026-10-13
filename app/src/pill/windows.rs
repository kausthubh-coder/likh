//! The pill's window on Windows: a layered, always-on-top popup that never
//! takes focus and lets every click through to the app underneath.
//!
//! winit can't make a window like this (no per-pixel alpha with the software
//! renderer, no "never activate"), so this file talks to Win32 directly. It is
//! the only place in likh allowed to use `unsafe` (AGENTS.md, Rules).
//!
//! One thread owns the window and sleeps in `GetMessageW` until `Waker::wake`
//! posts to it. While the pill moves, a 16ms timer drives frames; once it
//! settles the timer stops, so a hidden or still pill costs nothing.
#![allow(unsafe_code, reason = "Win32 layered overlay window; see the module docs")]

use super::{CANVAS, Command, Look};
use crate::dictation::Input;
use std::cell::{Cell, RefCell};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW, HGDIOBJ,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, ReleaseDC, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GWL_EXSTYLE, GetCursorPos, GetMessageW, GetWindowLongPtrW,
    HTCLIENT, HTTRANSPARENT, IDC_HAND, KillTimer, LoadCursorW, MA_NOACTIVATE, MSG, PostThreadMessageW,
    RegisterClassExW, SPI_GETCLIENTAREAANIMATION, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetCursor, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    SystemParametersInfoW, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WM_APP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_NCHITTEST, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

/// Windows' own "Show animations in Windows" (Settings > Accessibility > Visual
/// effects). On when it can't be read, as it is out of the box.
pub fn animations_on() -> bool {
    let mut on = windows::core::BOOL::from(true);
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL through the pointer,
    // which points at `on`, a BOOL we own for the whole call.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(std::ptr::from_mut(&mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_err() || on.as_bool()
}

const FRAME_TIMER: usize = 1;
const FRAME_MS: u32 = 16;

thread_local! {
    // The window procedure runs on the pill thread, so what it needs lives there:
    // where the hands-free stop button is on screen (centre and radius, in
    // pixels), and where its click goes.
    static STOP_BUTTON: Cell<Option<(i32, i32, i32)>> = const { Cell::new(None) };
    static DICTATION: RefCell<Option<Sender<Input>>> = const { RefCell::new(None) };
}

/// Wakes the pill thread after a command is queued. Posting to a thread is
/// safe from any thread and needs no window handle.
#[derive(Clone)]
pub struct Waker(u32);

impl Waker {
    pub fn wake(&self) {
        // SAFETY: PostThreadMessageW only reads its plain-value arguments; a
        // thread that has exited just makes it return an error, which we ignore.
        unsafe {
            let _ = PostThreadMessageW(self.0, WM_APP, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn spawn(commands: Receiver<Command>, dictation: Sender<Input>) -> Result<Waker, String> {
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("likh-pill".into())
        .spawn(move || match Overlay::create() {
            Ok(overlay) => {
                DICTATION.with(|slot| *slot.borrow_mut() = Some(dictation));
                // SAFETY: no arguments; returns this thread's id.
                let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));
                overlay.run(commands);
            }
            Err(reason) => {
                let _ = ready_tx.send(Err(reason));
            }
        })
        .map_err(|error| error.to_string())?;
    match ready_rx.recv() {
        Ok(Ok(thread)) => Ok(Waker(thread)),
        Ok(Err(reason)) => Err(reason),
        Err(_) => Err("the pill thread stopped before it started".into()),
    }
}

struct Overlay {
    hwnd: HWND,
    /// Top-left of the window on screen and the monitor's scale, set each time the pill appears.
    origin: POINT,
    scale: f32,
}

impl Overlay {
    fn create() -> Result<Self, String> {
        // SAFETY: plain Win32 calls with valid, owned arguments. The class name
        // and window title are static wide strings from `w!`.
        unsafe {
            // Draw at the monitor's real resolution, whatever the rest of the process does.
            let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let instance = GetModuleHandleW(None).map_err(|error| error.to_string())?;
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("likh-pill"),
                ..Default::default()
            };
            if RegisterClassExW(&class) == 0 {
                return Err("could not register the pill window class".into());
            }
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("likh-pill"),
                w!("likh"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(|error| error.to_string())?;
            Ok(Self { hwnd, origin: POINT::default(), scale: 1.0 })
        }
    }

    fn run(mut self, commands: Receiver<Command>) {
        let mut look = Look::default();
        let mut last = Instant::now();
        let mut ticking = false;
        let mut msg = MSG::default();
        // SAFETY: `msg` is a valid MSG we own; GetMessageW fills it.
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            match msg.message {
                WM_APP => {
                    for command in commands.try_iter() {
                        match command {
                            Command::Show(show) => {
                                if look.is_hidden() && show != super::Show::Hidden {
                                    self.place();
                                    // Read once per appearance, so changing it in Windows needs no restart.
                                    look.hold_still(!animations_on());
                                }
                                look.set(show);
                            }
                            Command::Level(level) => look.level(level),
                        }
                    }
                    if !ticking {
                        last = Instant::now();
                        ticking = true;
                        // SAFETY: our own window; the timer posts WM_TIMER to this thread's queue.
                        unsafe {
                            SetTimer(Some(self.hwnd), FRAME_TIMER, FRAME_MS, None);
                        }
                    }
                }
                WM_TIMER => {
                    let now = Instant::now();
                    let moving = look.step((now - last).as_secs_f32().min(0.05));
                    last = now;
                    if let Some(pixmap) = look.draw(self.scale) {
                        self.present(&pixmap);
                    }
                    self.take_clicks_at(look.stop_button());
                    if !moving {
                        ticking = false;
                        // SAFETY: our own window and timer id.
                        unsafe {
                            let _ = KillTimer(Some(self.hwnd), FRAME_TIMER);
                            if look.is_hidden() {
                                let _ = ShowWindow(self.hwnd, SW_HIDE);
                            }
                        }
                    }
                }
                _ => {
                    // SAFETY: standard dispatch of a message GetMessageW just returned.
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            }
        }
    }

    /// The pill lets every click through, except on the hands-free stop button.
    fn take_clicks_at(&self, button: Option<(f32, f32, f32)>) {
        let on_screen = button.map(|(cx, cy, r)| {
            let px = |v: f32| (v * self.scale).round() as i32;
            (self.origin.x + px(cx), self.origin.y + px(cy), px(r))
        });
        if STOP_BUTTON.with(Cell::get).is_some() == on_screen.is_some() {
            STOP_BUTTON.with(|slot| slot.set(on_screen));
            return;
        }
        STOP_BUTTON.with(|slot| slot.set(on_screen));
        // SAFETY: reads and writes our own window's extended style.
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let transparent = WS_EX_TRANSPARENT.0 as isize;
            let next = if on_screen.is_some() { style & !transparent } else { style | transparent };
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, next);
        }
    }

    /// Bottom centre of the work area (above the taskbar) on the monitor the mouse is on.
    fn place(&mut self) {
        // SAFETY: every out-parameter is a local we own and size correctly.
        unsafe {
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(monitor, &mut info);
            let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
            self.scale = dpi_x as f32 / 96.0;
            let (w, h) = ((CANVAS.0 * self.scale).ceil() as i32, (CANVAS.1 * self.scale).ceil() as i32);
            let work = info.rcWork;
            self.origin = POINT { x: work.left + (work.right - work.left - w) / 2, y: work.bottom - h };
            let _ = SetWindowPos(self.hwnd, None, self.origin.x, self.origin.y, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    /// Hands a frame to the compositor. tiny-skia is premultiplied RGBA; GDI wants premultiplied BGRA.
    fn present(&self, pixmap: &tiny_skia::Pixmap) {
        let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
        let header = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down rows, like the pixmap
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let info = BITMAPINFO { bmiHeader: header, ..Default::default() };
        // SAFETY: the DIB section is created with exactly w * h 32-bit pixels and
        // `bits` points at it until DeleteObject; we write w * h * 4 bytes, no more.
        // Every GDI object made here is released before returning.
        unsafe {
            let screen = GetDC(None);
            let memory = CreateCompatibleDC(Some(screen));
            let mut bits = std::ptr::null_mut();
            let Ok(bitmap) = CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(memory);
                ReleaseDC(None, screen);
                return;
            };
            if !bits.is_null() {
                let target = std::slice::from_raw_parts_mut(bits.cast::<u8>(), (w * h * 4) as usize);
                for (out, pixel) in target.as_chunks_mut::<4>().0.iter_mut().zip(pixmap.pixels()) {
                    out.copy_from_slice(&[pixel.blue(), pixel.green(), pixel.red(), pixel.alpha()]);
                }
            }
            let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                Some(screen),
                Some(&self.origin),
                Some(&SIZE { cx: w, cy: h }),
                Some(memory),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            SelectObject(memory, previous);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
        }
    }
}

/// Never activate, never take a click: the user's app keeps focus and the cursor.
extern "system" fn window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => {
            // Screen coordinates, packed as two signed 16-bit halves.
            let (x, y) = (i32::from(lparam.0 as i16), i32::from((lparam.0 >> 16) as i16));
            let on_button =
                STOP_BUTTON.with(Cell::get).is_some_and(|(cx, cy, r)| (x - cx).pow(2) + (y - cy).pow(2) <= r * r);
            LRESULT(if on_button { HTCLIENT as isize } else { HTTRANSPARENT as isize })
        }
        WM_SETCURSOR => {
            // SAFETY: the system hand cursor; no ownership involved.
            unsafe {
                if let Ok(hand) = LoadCursorW(None, IDC_HAND) {
                    SetCursor(Some(hand));
                }
            }
            LRESULT(1)
        }
        // Take the click without taking focus; act on release, like a button.
        WM_LBUTTONDOWN => LRESULT(0),
        WM_LBUTTONUP => {
            DICTATION.with(|slot| {
                if let Some(dictation) = slot.borrow().as_ref() {
                    let _ = dictation.send(Input::Stop);
                }
            });
            LRESULT(0)
        }
        // SAFETY: default handling for a message Windows sent to our own window.
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}
