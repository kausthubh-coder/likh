//! The shortcut on Windows, as a system hotkey.
//!
//! One thread owns the hotkeys (`RegisterHotKey` ties them to the thread that
//! made them) and sleeps in `GetMessageW`. Windows posts `WM_HOTKEY` when the
//! shortcut goes down and swallows the keystroke, so the focused app never sees
//! it. Windows doesn't say when it comes back up, so while it's held a 15ms
//! timer looks at the keys; when it's let go the timer stops. Idle, nothing runs.
//!
//! This and `pill/windows.rs` are the only places allowed `unsafe` (AGENTS.md, Rules).
#![allow(unsafe_code, reason = "Win32 hotkeys and key state; see the module docs")]

use super::Binding;
use crate::dictation::Input;
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, KillTimer, MSG, PostThreadMessageW, SetTimer, WM_APP, WM_HOTKEY, WM_TIMER,
};

const SHORTCUT: i32 = 1;
const ESCAPE: i32 = 2;
const POLL_MS: u32 = 15;
const WATCH_ESCAPE: u32 = WM_APP + 1;
const REBIND: u32 = WM_APP + 2;
const PAUSE: u32 = WM_APP + 3;

/// The hotkey thread's id, for posting it commands.
static THREAD: OnceLock<u32> = OnceLock::new();
/// A rebind's new shortcut going in, and where its result goes back.
struct RebindRequest {
    binding: Binding,
    reply: Sender<Result<(), String>>,
}
static REBIND_REQUEST: Mutex<Option<RebindRequest>> = Mutex::new(None);

pub fn spawn(dictation: Sender<Input>, binding: Binding) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("likh-hotkey".into())
        .spawn(move || {
            // SAFETY: no arguments; this thread's id.
            let _ = THREAD.set(unsafe { GetCurrentThreadId() });
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
                let _ = dictation.send(Input::Release);
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
        spawn(dictation, binding).expect("register test shortcut");
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
