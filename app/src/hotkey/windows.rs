//! The shortcuts on Windows, as system hotkeys.
//!
//! One thread owns the hotkeys (`RegisterHotKey` ties them to the thread that
//! made them) and sleeps in `GetMessageW`. Windows posts `WM_HOTKEY` when a
//! shortcut goes down and swallows the keystroke, so the focused app never sees
//! it. Windows doesn't say when it comes back up, so while one is held a 15ms
//! timer looks at the keys; when it's let go the timer stops. Idle, nothing runs.
//!
//! Three hotkeys can be held or pressed: the dictation shortcut, the action
//! key (both start a recording) and paste last. A mouse button can do either
//! recording's job.
//!
//! A mouse button needs a low-level mouse hook on this same thread. Windows
//! calls it on every mouse event and silently drops hooks that answer slowly,
//! so it does nothing but compare two numbers and post a message, the thread
//! runs at high priority while it's installed, and it's only installed while
//! a button is picked.
//!
//! It also reads which window is in front (`foreground`), for actions.
//!
//! This and `pill/windows.rs` are the only places allowed `unsafe` (AGENTS.md, Rules).
#![allow(unsafe_code, reason = "Win32 hotkeys, key state and the window in front; see the module docs")]

use super::{Binding, MouseTrigger, Triggers, Window};
use crate::dictation::Input;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use windows::Win32::Foundation::{CloseHandle, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW, SetThreadPriority, THREAD_PRIORITY_HIGHEST, THREAD_PRIORITY_NORMAL,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, HC_ACTION, HHOOK,
    KillTimer, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostThreadMessageW, SetTimer, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_MOUSE_LL, WM_APP, WM_HOTKEY, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_TIMER, WM_XBUTTONDOWN,
    WM_XBUTTONUP, XBUTTON1, XBUTTON2,
};
use windows::core::PWSTR;

const SHORTCUT: i32 = 1;
const ESCAPE: i32 = 2;
const PASTE_LAST: i32 = 3;
const ACTION: i32 = 4;
const POLL_MS: u32 = 15;
const WATCH_ESCAPE: u32 = WM_APP + 1;
const REBIND: u32 = WM_APP + 2;
const PAUSE: u32 = WM_APP + 3;
/// wparam: which job (`DICTATE` or `ACT`); lparam: the button's `mouse_code`.
const SET_MOUSE: u32 = WM_APP + 4;
/// wparam: which job the button does.
const MOUSE_DOWN: u32 = WM_APP + 5;
const MOUSE_UP: u32 = WM_APP + 6;
const DICTATE: usize = 0;
const ACT: usize = 1;

/// The picked mouse buttons, as `mouse_code`; read by the hook on every mouse event.
static MOUSE: AtomicU8 = AtomicU8::new(0);
static ACTION_MOUSE: AtomicU8 = AtomicU8::new(0);

/// The hotkey thread's id, for posting it commands.
static THREAD: OnceLock<u32> = OnceLock::new();
/// A rebind going in (which hotkey, and its new keys or None for off), and where its result goes back.
struct RebindRequest {
    id: i32,
    binding: Option<Binding>,
    reply: Sender<Result<(), String>>,
}
static REBIND_REQUEST: Mutex<Option<RebindRequest>> = Mutex::new(None);

pub fn spawn(dictation: Sender<Input>, triggers: Triggers) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("likh-hotkey".into())
        .spawn(move || {
            // SAFETY: no arguments; this thread's id.
            let _ = THREAD.set(unsafe { GetCurrentThreadId() });
            MOUSE.store(mouse_code(triggers.mouse), Ordering::SeqCst);
            ACTION_MOUSE.store(mouse_code(triggers.action_mouse), Ordering::SeqCst);
            let paste_last = triggers.paste_last.map(|keys| register(PASTE_LAST, keys));
            if let Some(Err(reason)) = &paste_last {
                crate::diagnostics::warn("hotkey", format!("pasting the last dictation won't work: {reason}"));
            }
            super::note_paste_last(paste_last.is_some_and(|registered| registered.is_err()));
            let action = triggers.action_key.map(|keys| register(ACTION, keys));
            if let Some(Err(reason)) = &action {
                crate::diagnostics::warn("hotkey", format!("the action key won't work: {reason}"));
            }
            super::note_action_key(action.is_some_and(|registered| registered.is_err()));
            let _ = ready_tx.send(register(SHORTCUT, triggers.shortcut));
            // Keep running even if another app owns the shortcut: a rebind can still fix it.
            run(dictation, triggers);
        })
        .map_err(|error| error.to_string())?;
    ready_rx.recv().map_err(|_| "the shortcut thread stopped before it started".to_string())?
}

pub fn rebind(binding: Binding) -> Result<(), String> {
    request(SHORTCUT, Some(binding))
}

pub fn set_paste_last(binding: Option<Binding>) -> Result<(), String> {
    request(PASTE_LAST, binding)
}

pub fn set_action_key(binding: Option<Binding>) -> Result<(), String> {
    request(ACTION, binding)
}

fn request(id: i32, binding: Option<Binding>) -> Result<(), String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    *REBIND_REQUEST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
        Some(RebindRequest { id, binding, reply: reply_tx });
    post(REBIND, 0, 0)?;
    reply_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| "the shortcut thread didn't answer".to_string())?
}

pub fn set_mouse(mouse: MouseTrigger) -> Result<(), String> {
    post(SET_MOUSE, DICTATE, isize::from(mouse_code(mouse)))
}

pub fn set_action_mouse(mouse: MouseTrigger) -> Result<(), String> {
    post(SET_MOUSE, ACT, isize::from(mouse_code(mouse)))
}

fn mouse_code(mouse: MouseTrigger) -> u8 {
    match mouse {
        MouseTrigger::Off => 0,
        MouseTrigger::Middle => 1,
        MouseTrigger::Back => 2,
        MouseTrigger::Forward => 3,
    }
}

/// Which job a pressed button does, if any: dictation's button first, then the action key's.
fn job_for(button: u8, dictate: u8, act: u8) -> Option<usize> {
    if button == 0 {
        None
    } else if button == dictate {
        Some(DICTATE)
    } else if button == act {
        Some(ACT)
    } else {
        None
    }
}

/// Runs on this thread for every mouse event while installed. It must answer fast.
unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let (dictate, act) = (MOUSE.load(Ordering::Relaxed), ACTION_MOUSE.load(Ordering::Relaxed));
    if code == HC_ACTION as i32 && (dictate != 0 || act != 0) {
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
        if let Some(job) = job_for(button, dictate, act)
            && info.flags & LLMHF_INJECTED == 0
        {
            let down = matches!(message, WM_MBUTTONDOWN | WM_XBUTTONDOWN);
            let _ = post(if down { MOUSE_DOWN } else { MOUSE_UP }, job, 0);
            // Swallowed, so the click doesn't also go back a page or open a link.
            return LRESULT(1);
        }
    }
    // SAFETY: passes the event on unchanged, as every hook must.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Installs or removes the mouse hook to match `MOUSE` and `ACTION_MOUSE`.
fn hook_mouse(hook: &mut Option<HHOOK>) {
    let wanted = MOUSE.load(Ordering::SeqCst) != 0 || ACTION_MOUSE.load(Ordering::SeqCst) != 0;
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
                Err(error) => crate::diagnostics::warn("hotkey", format!("the mouse button won't work: {error}")),
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
    let _ = post(PAUSE, usize::from(paused), 0);
}

pub fn watch_escape(on: bool) {
    let _ = post(WATCH_ESCAPE, usize::from(on), 0);
}

fn post(message: u32, wparam: usize, lparam: isize) -> Result<(), String> {
    let thread = THREAD.get().ok_or("the shortcut isn't running")?;
    // SAFETY: posts plain values to a thread's queue; fails harmlessly if it is gone.
    unsafe { PostThreadMessageW(*thread, message, WPARAM(wparam), LPARAM(lparam)) }.map_err(|error| error.to_string())
}

/// The window in front: its handle, its program's file name and its title.
pub fn foreground() -> Option<Window> {
    // SAFETY: no arguments; a handle we only read from and compare.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let mut title = [0u16; 512];
    // SAFETY: writes at most the buffer's length into our own buffer.
    let length = unsafe { GetWindowTextW(hwnd, &mut title) };
    let title = String::from_utf16_lossy(&title[..usize::try_from(length).unwrap_or(0).min(title.len())]);
    let mut pid = 0u32;
    // SAFETY: fills our own u32 with the window's process id.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    Some(Window { id: hwnd.0 as isize, app: process_name(pid).unwrap_or_default(), title })
}

/// A process's program file name (`chrome.exe`), read with the least access Windows offers.
fn process_name(pid: u32) -> Option<String> {
    // SAFETY: opens a handle we close below; limited query rights only.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut path = [0u16; 1024];
    let mut size = path.len() as u32;
    // SAFETY: Windows writes at most `size` units into our buffer and updates `size`.
    let read = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut size) };
    // SAFETY: the handle opened above.
    let _ = unsafe { CloseHandle(process) };
    read.ok()?;
    let full = String::from_utf16_lossy(&path[..(size as usize).min(path.len())]);
    full.rsplit(['\\', '/']).next().map(str::to_owned)
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

fn register(id: i32, binding: Binding) -> Result<(), String> {
    // SAFETY: registers a hotkey for this thread; plain values only.
    unsafe { RegisterHotKey(None, id, modifiers(binding), u32::from(binding.key.virtual_key())) }
        .map_err(|_| format!("{binding} is already used by another app. Pick other keys."))
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

/// No key of the shortcut is down any more.
fn all_up(binding: Binding) -> bool {
    !is_down(VIRTUAL_KEY(binding.key.virtual_key()))
        && !(binding.ctrl && is_down(VK_CONTROL))
        && !(binding.alt && is_down(VK_MENU))
        && !(binding.shift && is_down(VK_SHIFT))
        && !(binding.win && (is_down(VK_LWIN) || is_down(VK_RWIN)))
}

/// The keys each hotkey id is on right now (None: off).
struct Bound {
    shortcut: Binding,
    paste_last: Option<Binding>,
    action: Option<Binding>,
}

impl Bound {
    fn get(&self, id: i32) -> Option<Binding> {
        match id {
            SHORTCUT => Some(self.shortcut),
            PASTE_LAST => self.paste_last,
            ACTION => self.action,
            _ => None,
        }
    }

    fn set(&mut self, id: i32, binding: Option<Binding>) {
        match id {
            SHORTCUT => self.shortcut = binding.unwrap_or(self.shortcut),
            PASTE_LAST => self.paste_last = binding,
            ACTION => self.action = binding,
            _ => {}
        }
    }
}

fn run(dictation: Sender<Input>, triggers: Triggers) {
    let mut bound = Bound { shortcut: triggers.shortcut, paste_last: triggers.paste_last, action: triggers.action_key };
    // The hotkey held right now (SHORTCUT or ACTION): its keys coming up ends the recording.
    let mut held: Option<i32> = None;
    let mut timer = 0usize;
    // Paste last waits for its keys to come up: a Ctrl+V sent while Alt and
    // Shift are still down would reach the app as Ctrl+Alt+Shift+V.
    let mut paste_timer = 0usize;
    let mut mouse_hook: Option<HHOOK> = None;
    hook_mouse(&mut mouse_hook);
    let mut msg = MSG::default();
    // SAFETY: `msg` is ours; GetMessageW fills it.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        let hotkey = msg.wParam.0 as i32;
        match msg.message {
            WM_HOTKEY if matches!(hotkey, SHORTCUT | ACTION) && held.is_none() => {
                held = Some(hotkey);
                let _ = dictation.send(if hotkey == ACTION { Input::ActionHold } else { Input::Hold });
                // SAFETY: a thread timer (no window); WM_TIMER comes back to this loop.
                timer = unsafe { SetTimer(None, 0, POLL_MS, None) };
            }
            WM_HOTKEY if hotkey == ESCAPE => {
                let _ = dictation.send(Input::Cancel);
            }
            WM_HOTKEY if hotkey == PASTE_LAST && paste_timer == 0 => {
                // SAFETY: a thread timer (no window); WM_TIMER comes back to this loop.
                paste_timer = unsafe { SetTimer(None, 0, POLL_MS, None) };
            }
            WM_TIMER if paste_timer != 0 && msg.wParam.0 == paste_timer => {
                if bound.paste_last.is_none_or(all_up) {
                    // SAFETY: the thread timer we started above.
                    let _ = unsafe { KillTimer(None, paste_timer) };
                    paste_timer = 0;
                    let _ = dictation.send(Input::PasteLast);
                }
            }
            WM_TIMER if held.is_some_and(|id| bound.get(id).is_none_or(|keys| !still_held(keys))) => {
                held = None;
                // SAFETY: the thread timer we started above.
                let _ = unsafe { KillTimer(None, timer) };
                let _ = dictation.send(Input::Release(Instant::now()));
            }
            MOUSE_DOWN if held.is_none() => {
                let _ = dictation.send(if msg.wParam.0 == ACT { Input::ActionHold } else { Input::Hold });
            }
            MOUSE_UP if held.is_none() => {
                let _ = dictation.send(Input::Release(Instant::now()));
            }
            SET_MOUSE => {
                let code = msg.lParam.0 as u8;
                let (mine, other) = if msg.wParam.0 == ACT { (&ACTION_MOUSE, &MOUSE) } else { (&MOUSE, &ACTION_MOUSE) };
                mine.store(code, Ordering::SeqCst);
                // One button, one job: picking it for one takes it from the other.
                if code != 0 && other.load(Ordering::SeqCst) == code {
                    other.store(0, Ordering::SeqCst);
                }
                hook_mouse(&mut mouse_hook);
            }
            WATCH_ESCAPE if msg.wParam.0 == 1 => {
                // SAFETY: registers Esc for this thread while a recording or an action can be stopped.
                let _ = unsafe { RegisterHotKey(None, ESCAPE, MOD_NOREPEAT, u32::from(VK_ESCAPE.0)) };
            }
            WATCH_ESCAPE => unregister(ESCAPE),
            // Picking new keys pauses every shortcut, so pressing any of them reaches the window.
            PAUSE if msg.wParam.0 == 1 => {
                for id in [SHORTCUT, PASTE_LAST, ACTION] {
                    unregister(id);
                }
            }
            PAUSE => {
                // A rebind during the pause may already have registered them.
                for id in [SHORTCUT, PASTE_LAST, ACTION] {
                    unregister(id);
                    if let Some(keys) = bound.get(id) {
                        let _ = register(id, keys);
                    }
                }
            }
            REBIND => {
                let request = REBIND_REQUEST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
                if let Some(RebindRequest { id, binding: next, reply }) = request {
                    let current = bound.get(id);
                    unregister(id);
                    let result = next.map_or(Ok(()), |keys| register(id, keys));
                    match &result {
                        Ok(()) => {
                            bound.set(id, next);
                            match id {
                                SHORTCUT => super::note_shortcut(false),
                                PASTE_LAST => super::note_paste_last(false),
                                ACTION => super::note_action_key(false),
                                _ => {}
                            }
                        }
                        Err(_) => {
                            if let Some(keys) = current {
                                let _ = register(id, keys);
                            }
                        }
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
        let triggers = Triggers {
            shortcut: binding,
            paste_last: None,
            mouse: MouseTrigger::Off,
            action_key: None,
            action_mouse: MouseTrigger::Off,
        };
        spawn(dictation, triggers).expect("register test shortcut");
        let mut onboarding = Onboarding::new(CodexStatus::SignedOut);
        let _ = onboarding.update(Message::ChangeShortcut);
        assert!(wait_for_availability(binding, true), "capture should release the shortcut to the window");
        onboarding.closing();
        assert!(wait_for_availability(binding, false), "closing should give the shortcut back to likh");
    }

    #[test]
    fn a_mouse_button_does_the_job_it_was_picked_for_and_only_that() {
        // Back (2) dictates, forward (3) runs actions, middle (1) is nobody's.
        assert_eq!(job_for(2, 2, 3), Some(DICTATE));
        assert_eq!(job_for(3, 2, 3), Some(ACT));
        assert_eq!(job_for(1, 2, 3), None);
        // Nothing picked: no button is ever taken from the app.
        assert_eq!(job_for(0, 0, 0), None);
        assert_eq!(job_for(2, 0, 0), None);
    }

    #[test]
    fn the_window_in_front_has_a_program_name() {
        // The test runner's own console, or whatever is in front: either way a real program.
        if let Some(window) = foreground() {
            assert!(window.app.to_lowercase().ends_with(".exe"), "got {:?}", window.app);
        }
    }

    fn wait_for_availability(binding: Binding, expected: bool) -> bool {
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            // Register on this test thread to observe whether likh owns the keys.
            let available = register(SHORTCUT, binding).is_ok();
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
