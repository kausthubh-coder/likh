//! The shortcut on Windows: a low-level keyboard hook on its own thread.
//!
//! The thread sits in `GetMessageW`, which is where Windows calls the hook, so
//! it wakes only when a key moves: nothing runs while idle. Only keyboard
//! events are hooked (no mouse hook), and murmur's own injected keys (the
//! Ctrl+V paste, the Start-menu mask) are ignored.
//!
//! This and `pill/windows.rs` are the only places allowed `unsafe` (AGENTS.md, Rules).
#![allow(unsafe_code, reason = "Win32 low-level keyboard hook; see the module docs")]

use super::{Key, Keys, WATCH_ESCAPE};
use crate::dictation::Input;
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL,
    VK_ESCAPE, VK_LCONTROL, VK_LWIN, VK_RCONTROL, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, SetWindowsHookExW, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_SYSKEYDOWN,
};

/// An unassigned key code (AutoHotkey's default "menu mask"). Pressing it while
/// Win is down tells the shell Win was part of a combination.
const MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

thread_local! {
    // The hook runs on the thread that installed it, so its state lives there.
    static STATE: RefCell<Option<(Keys, Sender<Input>)>> = const { RefCell::new(None) };
}

pub fn spawn(dictation: Sender<Input>) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("murmur-hotkey".into())
        .spawn(move || {
            STATE.with(|state| *state.borrow_mut() = Some((Keys::default(), dictation)));
            // SAFETY: installs a hook procedure that lives for the whole program;
            // the module handle is our own executable's.
            let hook = unsafe {
                GetModuleHandleW(None)
                    .and_then(|module| SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), Some(module.into()), 0))
            };
            if let Err(error) = hook {
                let _ = ready_tx.send(Err(error.to_string()));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            let mut msg = MSG::default();
            // SAFETY: `msg` is ours; the loop only exists so Windows can call the hook.
            while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {}
        })
        .map_err(|error| error.to_string())?;
    ready_rx.recv().map_err(|_| "the shortcut thread stopped before it started".to_string())?
}

extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for WH_KEYBOARD_LL with code >= 0, lparam points at a KBDLLHOOKSTRUCT.
        let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let injected = event.flags.0 & LLKHF_INJECTED.0 != 0;
        if !injected && react(event.vkCode, matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN)) {
            return LRESULT(1);
        }
    }
    // SAFETY: passes the event on unchanged, as every hook must.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Feeds one key event to `Keys`. Returns true when the event should be swallowed.
fn react(code: u32, down: bool) -> bool {
    let key = match VIRTUAL_KEY(code as u16) {
        VK_LCONTROL | VK_RCONTROL | VK_CONTROL => Key::Ctrl,
        VK_LWIN | VK_RWIN => Key::Win,
        VK_ESCAPE => Key::Escape,
        _ => Key::Other,
    };
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some((keys, dictation)) = state.as_mut() else { return false };
        let reaction = keys.on_key(key, down, WATCH_ESCAPE.load(Ordering::SeqCst));
        if let Some(input) = reaction.input {
            let _ = dictation.send(input);
        }
        if reaction.mask_start_menu {
            mask_start_menu();
        }
        reaction.swallow
    })
}

fn mask_start_menu() {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: MASK_KEY, dwFlags: flags, ..Default::default() } },
    };
    // SAFETY: two fully initialised keyboard INPUTs; SendInput copies them.
    unsafe {
        SendInput(&[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)], size_of::<INPUT>() as i32);
    }
}
