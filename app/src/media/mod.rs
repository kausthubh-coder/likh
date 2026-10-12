//! Pauses music and videos while you talk, then plays them again: only the
//! ones likh paused, so something you'd paused yourself stays paused.
//!
//! Windows asks each app through its media controls (the same ones the
//! keyboard's play key and the volume flyout use). Apps without media
//! controls (most games, some players) keep playing. Nothing runs between
//! dictations: no polling, no listeners.
//!
//! macOS has no public way to do this, so it does nothing there.

#[cfg(target_os = "windows")]
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

/// What `pause_playing` paused. `resume` plays exactly these again.
#[derive(Default)]
pub struct Paused {
    #[cfg(target_os = "windows")]
    sessions: Vec<GlobalSystemMediaTransportControlsSession>,
}

impl Paused {
    pub fn resume(self) {
        #[cfg(target_os = "windows")]
        for session in self.sessions {
            let _ = session.TryPlayAsync().and_then(|played| played.join());
        }
    }
}

/// Pauses everything playing right now. Blocks for a few tens of milliseconds,
/// so call it off the dictation's thread.
pub fn pause_playing() -> Paused {
    #[cfg(target_os = "windows")]
    {
        let Ok(manager) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().and_then(|op| op.join())
        else {
            return Paused::default();
        };
        let Ok(sessions) = manager.GetSessions() else { return Paused::default() };
        let playing = (0..sessions.Size().unwrap_or(0)).filter_map(|i| sessions.GetAt(i).ok()).filter(|session| {
            session.GetPlaybackInfo().and_then(|info| info.PlaybackStatus()).ok()
                == Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
        });
        let sessions = playing
            .filter(|session| session.TryPauseAsync().and_then(|paused| paused.join()).unwrap_or(false))
            .collect();
        Paused { sessions }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Paused::default()
    }
}

/// Whether this computer can pause other apps' media.
pub const SUPPORTED: bool = cfg!(target_os = "windows");

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    /// Lists this computer's media sessions without pausing anything: proves an
    /// unpackaged likh can reach Windows' media controls at all.
    /// Run by hand: `cargo test -p likh media -- --ignored --nocapture`
    #[test]
    #[ignore = "reads this computer's media sessions"]
    fn windows_lists_media_sessions_to_an_unpackaged_app() {
        let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .and_then(|op| op.join())
            .expect("the media session manager");
        let sessions = manager.GetSessions().expect("sessions");
        for i in 0..sessions.Size().expect("count") {
            let session = sessions.GetAt(i).expect("session");
            let app = session.SourceAppUserModelId().map(|id| id.to_string_lossy()).unwrap_or_default();
            let status = session.GetPlaybackInfo().and_then(|info| info.PlaybackStatus()).map(|s| s.0);
            eprintln!("{app}: playback status {status:?} (4 is playing)");
        }
    }
}
