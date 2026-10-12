//! Windows' own speech recognizer (`Windows.Media.SpeechRecognition`), the
//! engine behind Windows dictation. Free, no account, no key. It needs
//! "Online speech recognition" on in Windows' privacy settings.
//!
//! Unlike the other transcribers it can't be handed a recording: it opens
//! the default microphone itself. So it listens alongside likh's own
//! recording (which still drives the pill's bars) from key-down to key-up.
//! Getting a recognizer ready took about 10ms here, so it's made per dictation.

use super::Failure;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::SpeechRecognition::{
    SpeechContinuousRecognitionCompletedEventArgs, SpeechContinuousRecognitionResultGeneratedEventArgs,
    SpeechContinuousRecognitionSession, SpeechRecognitionResultStatus, SpeechRecognitionScenario,
    SpeechRecognitionTopicConstraint, SpeechRecognizer,
};

/// Windows reports "the speech privacy policy was not accepted" with this code
/// when Online speech recognition is off.
const PRIVACY_POLICY_NOT_ACCEPTED: i32 = 0x8004_5509_u32 as i32;
/// Where Windows remembers that Online speech recognition was turned on.
const PRIVACY_KEY: &str = r"Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy";
/// The session ends on its own after this much quiet; a dictation ends sooner.
const LONGEST_QUIET: Duration = Duration::from_secs(600);
/// How long `finish` waits for Windows to hand over the last words.
const FINISH_WAIT: Duration = Duration::from_secs(10);

/// A recognizer that is listening right now.
pub struct Listening {
    recognizer: SpeechRecognizer,
    heard: Arc<Mutex<Vec<String>>>,
    ended: mpsc::Receiver<SpeechRecognitionResultStatus>,
}

/// Ready to listen, or the reason it can't: shown by onboarding and settings.
/// Never opens the microphone: compiling the grammar succeeds even with the
/// privacy switch off (Windows only refuses when listening starts), so the
/// switch is read from the registry.
pub fn check() -> Result<(), Failure> {
    let accepted = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(PRIVACY_KEY)
        .and_then(|key| key.get_value::<u32, _>("HasAccepted"))
        .is_ok_and(|value| value == 1);
    if !accepted {
        return Err(Failure::WindowsPrivacy);
    }
    prepare().map(|_| ())
}

fn prepare() -> Result<SpeechRecognizer, Failure> {
    let recognizer = SpeechRecognizer::new().map_err(|error| unavailable(&error))?;
    let dictation = SpeechRecognitionTopicConstraint::Create(SpeechRecognitionScenario::Dictation, &"dictation".into())
        .map_err(|error| unavailable(&error))?;
    recognizer.Constraints().and_then(|constraints| constraints.Append(&dictation)).map_err(|e| unavailable(&e))?;
    let compiled =
        recognizer.CompileConstraintsAsync().and_then(|op| op.join()).map_err(|error| unavailable(&error))?;
    match compiled.Status() {
        Ok(SpeechRecognitionResultStatus::Success) => Ok(recognizer),
        Ok(status) => Err(from_status(status)),
        Err(error) => Err(unavailable(&error)),
    }
}

/// Starts listening to the default microphone now.
pub fn start() -> Result<Listening, Failure> {
    let recognizer = prepare()?;
    let session = recognizer.ContinuousRecognitionSession().map_err(|error| unavailable(&error))?;
    let quiet = TimeSpan { Duration: (LONGEST_QUIET.as_millis() as i64) * 10_000 };
    let _ = session.SetAutoStopSilenceTimeout(quiet);
    let heard = Arc::new(Mutex::new(Vec::new()));
    let (ended_tx, ended) = mpsc::channel();

    let words = heard.clone();
    session
        .ResultGenerated(&TypedEventHandler::<
            SpeechContinuousRecognitionSession,
            SpeechContinuousRecognitionResultGeneratedEventArgs,
        >::new(move |_, args| {
            if let Some(args) = args.as_ref() {
                let result = args.Result()?;
                if result.Status()? == SpeechRecognitionResultStatus::Success {
                    let text = result.Text()?.to_string_lossy();
                    if !text.trim().is_empty() {
                        words.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(text.trim().to_owned());
                    }
                }
            }
            Ok(())
        }))
        .map_err(|error| unavailable(&error))?;
    session
        .Completed(&TypedEventHandler::<
            SpeechContinuousRecognitionSession,
            SpeechContinuousRecognitionCompletedEventArgs,
        >::new(move |_, args| {
            let status = args.as_ref().and_then(|args| args.Status().ok());
            let _ = ended_tx.send(status.unwrap_or(SpeechRecognitionResultStatus::Unknown));
            Ok(())
        }))
        .map_err(|error| unavailable(&error))?;
    session.StartAsync().and_then(|action| action.join()).map_err(|error| unavailable(&error))?;
    Ok(Listening { recognizer, heard, ended })
}

impl Listening {
    /// Stops listening and returns everything Windows heard, in order.
    pub fn finish(self) -> Result<String, Failure> {
        let stopped = self.recognizer.ContinuousRecognitionSession().and_then(|session| session.StopAsync());
        // Stopping fails if Windows already ended the session (say, the network went); `ended` says why.
        if let Ok(stop) = stopped {
            let _ = stop.join();
        }
        let status = self.ended.recv_timeout(FINISH_WAIT).unwrap_or(SpeechRecognitionResultStatus::Success);
        let heard = self.heard.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).join(" ");
        match status {
            _ if !heard.is_empty() => Ok(heard),
            SpeechRecognitionResultStatus::Success | SpeechRecognitionResultStatus::UserCanceled => Ok(heard),
            status => Err(from_status(status)),
        }
    }
}

fn from_status(status: SpeechRecognitionResultStatus) -> Failure {
    match status {
        SpeechRecognitionResultStatus::NetworkFailure => Failure::WindowsOffline,
        SpeechRecognitionResultStatus::TopicLanguageNotSupported
        | SpeechRecognitionResultStatus::GrammarLanguageMismatch => Failure::WindowsLanguage,
        SpeechRecognitionResultStatus::MicrophoneUnavailable => Failure::WindowsMicrophone,
        other => Failure::WindowsSpeech(format!("status {}", other.0)),
    }
}

fn unavailable(error: &windows::core::Error) -> Failure {
    if error.code().0 == PRIVACY_POLICY_NOT_ACCEPTED {
        Failure::WindowsPrivacy
    } else {
        Failure::WindowsSpeech(format!("{:#010x}", error.code().0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs Online speech recognition on and a microphone. Run by hand:
    /// `cargo test -p likh windows_speech -- --ignored --nocapture`
    #[test]
    #[ignore = "uses this computer's microphone and Windows speech service"]
    fn listens_and_stops_without_an_error() {
        if let Err(failure) = check() {
            panic!("not ready: {failure:?} ({})", failure.message());
        }
        let started = std::time::Instant::now();
        let listening = start().expect("start listening");
        eprintln!("start took {:?}", started.elapsed());
        std::thread::sleep(Duration::from_secs(3));
        let heard = listening.finish();
        eprintln!("heard: {heard:?}");
        assert!(heard.is_ok(), "{heard:?}");
    }
}
