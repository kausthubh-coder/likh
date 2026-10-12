//! The shortcut on Windows, as a system hotkey.
//!
//! One thread owns the hotkeys (`RegisterHotKey` ties them to the thread that
//! made them) and sleeps in `GetMessageW`. Windows posts `WM_HOTKEY` when the
//! shortcut goes down and swallows the keystroke, so the focused app never sees
//! it. Windows doesn't say when it comes back up, so while it's held a 15ms
//! timer looks at the keys; when it's let go the timer stops. Idle, nothing runs.
//!
//! A mouse button needs a low-level mouse hook on this same thread. Windows
//! calls it on every mouse event and silently drops hooks that answer slowly,
//! so it does nothing but compare one number and post a message, the thread
//! runs at high priority while it's installed, and it's only installed while
//! a button is picked.
//!
//! This and `pill/windows.rs` are the only places allowed `unsafe` (AGENTS.md, Rules).
#![allow(unsafe_code, reason = "Win32 hotkeys and key state; see the module docs")]

use super::{Binding, MouseTrigger};
use crate::dictation::Input;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_HIGHEST, THREAD_PRIORITY_NORMAL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, KillTimer, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostThreadMessageW,
    SetTimer, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_APP, WM_HOTKEY, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_TIMER, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
};

const SHORTCUT: i32 = 1;
const ESCAPE: i32 = 2;
const POLL_MS: u32 = 15;
const WATCH_ESCAPE: u32 = WM_APP + 1;
const REBIND: u32 = WM_APP + 2;
const PAUSE: u32 = WM_APP + 3;
const SET_MOUSE: u32 = WM_APP + 4;
const MOUSE_DOWN: u32 = WM_APP + 5;
const MOUSE_UP: u32 = WM_APP + 6;

/// The picked mouse button, as `mouse_code`; read by the hook on every mouse event.
static MOUSE: AtomicU8 = AtomicU8::new(0);

/// The hotkey thread's id, for posting it commands.
static THREAD: OnceLock<u32> = OnceLock::new();
/// A rebind's new shortcut going in, and where its result goes back.
struct RebindRequest {
    binding: Binding,
    reply: Sender<Result<(), String>>,
}
static REBIND_REQUEST: Mutex<Option<RebindRequest>> = Mutex::new(None);

pub fn spawn(dictation: Sender<Input>, binding: Binding, mouse: MouseTrigger) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("likh-hotkey".into())
        .spawn(move || {
            // SAFETY: no arguments; this thread's id.
            let _ = THREAD.set(unsafe { GetCurrentThreadId() });
            MOUSE.store(mouse_code(mouse), Ordering::SeqCst);
            let _ = ready_tx.send(register(binding));
            // Keep running even if another app owns the shortcut: a rebind can still fix it.
            run(dictation, binding);
        })
        .map_err(|error| error.to_string())?;
    ready_rx.recv().map_err(|_| "the shortcut thread stopped before it started".to_string())?
}

pub fn rebind(binding: Binding) -> Result<(), String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    *REBIND_REQUEST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
        Some(RebindRequest { binding, reply: reply_tx });
    post(REBIND, 0)?;
    reply_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| "the shortcut thread didn't answer".to_string())?
}

pub fn set_mouse(mouse: MouseTrigger) -> Result<(), String> {
    post(SET_MOUSE, usize::from(mouse_code(mouse)))
}

fn mouse_code(mouse: MouseTrigger) -> u8 {
    match mouse {
        MouseTrigger::Off => 0,
        MouseTrigger::Middle => 1,
        MouseTrigger::Back => 2,
        MouseTrigger::Forward => 3,
    }
}

/// Runs on this thread for every mouse event while installed. It must answer fast.
unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let picked = MOUSE.load(Ordering::Relaxed);
    if code == HC_ACTION as i32 && picked != 0 {
        // SAFETY: for WH_MOUSE_LL with HC_ACTION, lparam points at an MSLLHOOKSTRUCT Windows owns for this call.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let message = wparam.0 as u32;
        let button = match message {
            WM_MBUTTONDOWN | WM_MBUTTONUP => 1,
            WM_XBUTTONDOWN | WM_XBUTTONUP => match (info.mouseData >> 16) as u16 {
                XBUTTON1 => 2,
                XBUTTON2 => 3,
                _ => 0,
            },
            _ => 0,
        };
        // Injected clicks (remote tools, macros) pass through untouched.
        if button == picked && info.flags & LLMHF_INJECTED == 0 {
            let down = matches!(message, WM_MBUTTONDOWN | WM_XBUTTONDOWN);
            let _ = post(if down { MOUSE_DOWN } else { MOUSE_UP }, 0);
            // Swallowed, so the click doesn't also go back a page or open a link.
            return LRESULT(1);
        }
    }
    // SAFETY: passes the event on unchanged, as every hook must.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Installs or removes the mouse hook to match `MOUSE`.
fn hook_mouse(hook: &mut Option<HHOOK>) {
    let wanted = MOUSE.load(Ordering::SeqCst) != 0;
    match (wanted, *hook) {
        (true, None) => {
            // SAFETY: our own module handle; the hook proc lives as long as the program.
            let module = unsafe { GetModuleHandleW(None) }.ok().map(Into::into);
            // SAFETY: a global low-level hook owned by this thread, which pumps messages in `run`.
            match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), module, 0) } {
                Ok(installed) => {
                    *hook = Some(installed);
                    // SAFETY: changes this thread's own priority, so a busy machine doesn't time the hook out.
                    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) };
                }
                Err(error) => eprintln!("likh: the mouse button won't work: {error}"),
            }
        }
        (false, Some(installed)) => {
            // SAFETY: the hook this thread installed above.
            let _ = unsafe { UnhookWindowsHookEx(installed) };
            *hook = None;
            // SAFETY: back to normal once nothing time-critical runs here.
            let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL) };
        }
        _ => {}
    }
}

pub fn pause(paused: bool) {
    let _ = post(PAUSE, usize::from(paused));
}

pub fn watch_escape(on: bool) {
    let _ = post(WATCH_ESCAPE, usize::from(on));
}

fn post(message: u32, wparam: usize) -> Result<(), String> {
    let thread = THREAD.get().ok_or("the shortcut isn't running")?;
    // SAFETY: posts plain values to a thread's queue; fails harmlessly if it is gone.
    unsafe { PostThreadMessageW(*thread, message, WPARAM(wparam), LPARAM(0)) }.map_err(|error| error.to_string())
}

fn modifiers(binding: Binding) -> HOT_KEY_MODIFIERS {
    let mut flags = MOD_NOREPEAT;
    for (on, flag) in
        [(binding.ctrl, MOD_CONTROL), (binding.alt, MOD_ALT), (binding.shift, MOD_SHIFT), (binding.win, MOD_WIN)]
    {
        if on {
            flags |= flag;
        }
    }
    flags
}

fn register(binding: Binding) -> Result<(), String> {
    // SAFETY: registers a hotkey for this thread; plain values only.
    unsafe { RegisterHotKey(None, SHORTCUT, modifiers(binding), u32::from(binding.key.virtual_key())) }
        .map_err(|_| format!("{binding} is already used by another app. Pick another shortcut."))
}

fn unregister(id: i32) {
    // SAFETY: unregisters this thread's own hotkey; harmless if it wasn't registered.
    let _ = unsafe { UnregisterHotKey(None, id) };
}

fn is_down(key: VIRTUAL_KEY) -> bool {
    // SAFETY: reads the key's current state; no pointers.
    (unsafe { GetAsyncKeyState(i32::from(key.0)) } as u16 & 0x8000) != 0
}

/// Every key of the shortcut is still down.
fn still_held(binding: Binding) -> bool {
    is_down(VIRTUAL_KEY(binding.key.virtual_key()))
        && (!binding.ctrl || is_down(VK_CONTROL))
        && (!binding.alt || is_down(VK_MENU))
        && (!binding.shift || is_down(VK_SHIFT))
        && (!binding.win || is_down(VK_LWIN) || is_down(VK_RWIN))
}

fn run(dictation: Sender<Input>, mut binding: Binding) {
    let mut held = false;
    let mut timer = 0usize;
    let mut mouse_hook: Option<HHOOK> = None;
    hook_mouse(&mut mouse_hook);
    let mut msg = MSG::default();
    // SAFETY: `msg` is ours; GetMessageW fills it.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        match msg.message {
            WM_HOTKEY if msg.wParam.0 == SHORTCUT as usize && !held => {
                held = true;
                let _ = dictation.send(Input::Hold);
                // SAFETY: a thread timer (no window); WM_TIMER comes back to this loop.
                timer = unsafe { SetTimer(None, 0, POLL_MS, None) };
            }
            WM_HOTKEY if msg.wParam.0 == ESCAPE as usize => {
                let _ = dictation.send(Input::Cancel);
            }
            WM_TIMER if held && !still_held(binding) => {
                held = false;
                // SAFETY: the thread timer we started above.
                let _ = unsafe { KillTimer(None, timer) };
                let _ = dictation.send(Input::Release(Instant::now()));
            }
            MOUSE_DOWN if !held => {
                let _ = dictation.send(Input::Hold);
            }
            MOUSE_UP if !held => {
                let _ = dictation.send(Input::Release(Instant::now()));
            }
            SET_MOUSE => {
                MOUSE.store(msg.wParam.0 as u8, Ordering::SeqCst);
                hook_mouse(&mut mouse_hook);
            }
            WATCH_ESCAPE if msg.wParam.0 == 1 => {
                // SAFETY: registers Esc for this thread while a recording can be cancelled.
                let _ = unsafe { RegisterHotKey(None, ESCAPE, MOD_NOREPEAT, u32::from(VK_ESCAPE.0)) };
            }
            WATCH_ESCAPE => unregister(ESCAPE),
            PAUSE if msg.wParam.0 == 1 => unregister(SHORTCUT),
            PAUSE => {
                // A rebind during the pause may already have registered it.
                unregister(SHORTCUT);
                let _ = register(binding);
            }
            REBIND => {
                let request = REBIND_REQUEST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
                if let Some(RebindRequest { binding: next, reply }) = request {
                    unregister(SHORTCUT);
                    let result = register(next);
                    if result.is_ok() {
                        binding = next;
                    } else {
                        let _ = register(binding);
                    }
                    let _ = reply.send(result);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex_auth::CodexStatus;
    use crate::hotkey::KeyName;
    use crate::onboarding::{Message, Onboarding};
    use std::time::{Duration, Instant};

    #[test]
    fn closing_onboarding_during_capture_restores_the_global_shortcut() {
        let _ui = crate::shared::test_ui::lock();
        let binding = Binding::new(true, true, true, false, KeyName("F10")).expect("test shortcut");
        let (dictation, _inputs) = mpsc::channel();
        spawn(dictation, binding, MouseTrigger::Off).expect("register test shortcut");
        let mut onboarding = Onboarding::new(CodexStatus::SignedOut);
        let _ = onboarding.update(Message::ChangeShortcut);
        assert!(wait_for_availability(binding, true), "capture should release the shortcut to the window");
        onboarding.closing();
        assert!(wait_for_availability(binding, false), "closing should give the shortcut back to likh");
    }

    fn wait_for_availability(binding: Binding, expected: bool) -> bool {
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            // Register on this test thread to observe whether likh owns the keys.
            let available = register(binding).is_ok();
            if available {
                unregister(SHORTCUT);
            }
            if available == expected {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }
}
