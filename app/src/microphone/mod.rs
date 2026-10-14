//! The microphone: how loud it is (a stream of 0–1 levels for bars) and what
//! it heard (a recording, as 16 kHz mono samples for the transcriber).
//!
//! The mic is open only while something listens: the onboarding meter, or a
//! dictation between key-down and key-up. Never in the background.
//!
//! `LIKH_AUDIO=path.wav` makes recordings replay that file in real time
//! instead of opening the mic, so agents can dictate without anyone speaking.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use iced::futures::Stream;
use iced::futures::channel::mpsc::Sender;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

/// One level per 30ms of audio: smooth enough for bars, about 33 redraws a second.
const WINDOW: Duration = Duration::from_millis(30);

/// Below this a sample is digital silence (-80 dBFS): what a muted mic, or one
/// Windows blocks, sends. A quiet room on a real mic sits around -60 dBFS, and
/// even a whisper is far above it.
const SOUND_FLOOR: f32 = 0.000_1;

/// The loudness range the bars cover: -55 dBFS (a quiet room) is empty and
/// -10 dBFS (talking close to the mic) is full.
const FLOOR_DB: f32 = -55.0;
const CEILING_DB: f32 = -10.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Opened { device: String },
    Level(f32),
    Failed(String),
}

/// Opens the default input device and reports its level until the stream is dropped.
pub fn listen() -> impl Stream<Item = Event> {
    iced::stream::channel(64, async |output: Sender<Event>| {
        // cpal streams can't move between threads on every platform, so one
        // thread owns it for its whole life.
        let spawned = std::thread::Builder::new().name("likh-meter".into()).spawn(move || meter(output));
        if let Err(error) = spawned {
            eprintln!("likh: could not start the microphone thread: {error}");
        }
    })
}

fn meter(mut output: Sender<Event>) {
    let mut levels = output.clone();
    let mut meter: Option<Meter> = None;
    let opened = open_input(move |rate, mono| {
        let meter = meter.get_or_insert_with(|| Meter::new(window_for(rate)));
        meter.push(mono.iter().copied(), |level| {
            // A full channel means the UI is behind; dropping a level is fine.
            let _ = levels.try_send(Event::Level(level));
        });
    });
    let stream = match opened {
        Ok(input) => {
            let _ = output.try_send(Event::Opened { device: input.device });
            input.stream
        }
        Err(reason) => {
            let _ = output.try_send(Event::Failed(reason));
            return;
        }
    };
    // The subscriber dropping its receiver is the signal to let go of the mic.
    while !output.is_closed() {
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(stream);
}

/// A dictation in progress. Dropping it without `finish` throws the audio away.
pub struct Recording {
    stop: Arc<AtomicBool>,
    heard: Arc<Mutex<Heard>>,
    /// The last moment a voice was heard (`silence::Listener`); None until someone speaks.
    last_voice: Arc<Mutex<Option<Instant>>>,
    /// Any sample above digital silence so far.
    sounded: Arc<AtomicBool>,
    /// The mic's name as Windows shows it, or the replayed file's name.
    device: String,
    thread: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct Heard {
    rate: u32,
    samples: Vec<f32>,
}

impl Recording {
    /// Starts recording now. `on_level` gets a 0–1 level every 30ms, for the pill's bars.
    /// Returns once the mic is really open, or with the reason it couldn't be.
    pub fn start(on_level: impl FnMut(f32) + Send + 'static) -> Result<Self, String> {
        Self::start_from(std::env::var_os("LIKH_AUDIO").map(PathBuf::from), on_level)
    }

    /// `source` is a WAV file to replay, or `None` for the default microphone.
    pub fn start_from(source: Option<PathBuf>, on_level: impl FnMut(f32) + Send + 'static) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let heard = Arc::new(Mutex::new(Heard::default()));
        let last_voice = Arc::new(Mutex::new(None));
        let sounded = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<String, String>>();
        let (thread_stop, thread_heard) = (stop.clone(), heard.clone());
        let ears = Ears { last_voice: last_voice.clone(), sounded: sounded.clone() };
        let thread = std::thread::Builder::new()
            .name("likh-recorder".into())
            .spawn(move || record(source, on_level, ears, thread_stop, thread_heard, ready_tx))
            .map_err(|error| error.to_string())?;
        match ready_rx.recv() {
            Ok(Ok(device)) => Ok(Self { stop, heard, last_voice, sounded, device, thread: Some(thread) }),
            Ok(Err(reason)) => Err(reason),
            Err(_) => Err("The microphone thread stopped before it started.".into()),
        }
    }

    /// How long it has been quiet since the last speech; None if nobody has spoken yet.
    pub fn quiet_for(&self) -> Option<Duration> {
        self.last_voice.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).map(|at| at.elapsed())
    }

    /// Someone has spoken: at least `silence::ARM_AFTER` of voice so far.
    pub fn spoke(&self) -> bool {
        self.last_voice.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_some()
    }

    /// The mic has sent anything but digital silence. False means it's muted,
    /// blocked or broken, not that the room is quiet.
    pub fn sounded(&self) -> bool {
        self.sounded.load(Ordering::Relaxed)
    }

    /// The mic's name as Windows shows it ("Microphone Array"), for messages.
    pub fn device(&self) -> &str {
        &self.device
    }

    /// Stops and returns what was heard, as 16 kHz mono.
    pub fn finish(mut self) -> Vec<f32> {
        self.stop_thread();
        let heard = std::mem::take(&mut *self.heard.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        resample(&heard.samples, heard.rate, crate::transcriber::SAMPLE_RATE)
    }

    fn stop_thread(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        self.stop_thread();
    }
}

/// What the recorder thread reports back while it listens.
struct Ears {
    last_voice: Arc<Mutex<Option<Instant>>>,
    sounded: Arc<AtomicBool>,
}

fn record(
    source: Option<PathBuf>,
    mut on_level: impl FnMut(f32) + Send + 'static,
    ears: Ears,
    stop: Arc<AtomicBool>,
    heard: Arc<Mutex<Heard>>,
    ready: mpsc::Sender<Result<String, String>>,
) {
    let mut meter: Option<Meter> = None;
    let mut listener: Option<crate::silence::Listener> = None;
    let mut keep = move |rate: u32, mono: &[f32]| {
        meter.get_or_insert_with(|| Meter::new(window_for(rate))).push(mono.iter().copied(), &mut on_level);
        if !ears.sounded.load(Ordering::Relaxed) && mono.iter().any(|sample| sample.abs() > SOUND_FLOOR) {
            ears.sounded.store(true, Ordering::Relaxed);
        }
        let listener = listener.get_or_insert_with(|| crate::silence::Listener::new(rate));
        if let Some(ago) = listener.push(mono) {
            *ears.last_voice.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Instant::now().checked_sub(ago);
        }
        let mut heard = heard.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        heard.rate = rate;
        heard.samples.extend_from_slice(mono);
    };
    match source {
        Some(path) => {
            let (rate, samples) = match read_wav(&path) {
                Ok(file) => file,
                Err(reason) => {
                    let _ = ready.send(Err(reason));
                    return;
                }
            };
            let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into());
            let _ = ready.send(Ok(name));
            // Feed the file as if a mic were hearing it: one 30ms chunk every 30ms,
            // kept to the clock. Sleeping 30ms per chunk drifted badly on slow
            // machines (each sleep overshoots), so a long file came out short.
            let chunk = window_for(rate);
            let started = std::time::Instant::now();
            for (i, piece) in samples.chunks(chunk).enumerate() {
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                let due = started + WINDOW * i as u32;
                if let Some(wait) = due.checked_duration_since(std::time::Instant::now()) {
                    std::thread::sleep(wait);
                }
                keep(rate, piece);
            }
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        None => {
            let stream = match open_input(keep) {
                Ok(input) => {
                    let _ = ready.send(Ok(short_name(&input.device)));
                    input.stream
                }
                Err(reason) => {
                    let _ = ready.send(Err(reason));
                    return;
                }
            };
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
            drop(stream);
        }
    }
}

/// Windows names an input "Microphone Array (Realtek(R) Audio)": the name its
/// Sound settings show in large type, then the driver in brackets. Messages use the name.
pub fn short_name(device: &str) -> String {
    match device.split_once(" (") {
        Some((name, _)) if !name.trim().is_empty() => name.trim().to_string(),
        _ => device.trim().to_string(),
    }
}

/// What the pill says when `blocked_by_windows`.
pub const BLOCKED: &str = "Windows is blocking likh from the mic.";

/// Windows' privacy settings keep desktop apps off the mic: for everyone, for
/// this user, or for apps installed outside the Store (likh is one).
#[cfg(target_os = "windows")]
pub fn blocked_by_windows() -> bool {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
    let denied = |root: &RegKey, path: &str| {
        root.open_subkey(path).and_then(|key| key.get_value::<String, _>("Value")).is_ok_and(|value| value == "Deny")
    };
    let (machine, user) = (RegKey::predef(HKEY_LOCAL_MACHINE), RegKey::predef(HKEY_CURRENT_USER));
    denied(&machine, STORE) || denied(&user, STORE) || denied(&user, &format!(r"{STORE}\NonPackaged"))
}

#[cfg(not(target_os = "windows"))]
pub fn blocked_by_windows() -> bool {
    false
}

fn window_for(rate: u32) -> usize {
    (rate as f32 * WINDOW.as_secs_f32()) as usize
}

fn read_wav(path: &PathBuf) -> Result<(u32, Vec<f32>), String> {
    let mut reader = hound::WavReader::open(path).map_err(|error| format!("Can't read {}: {error}", path.display()))?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().filter_map(Result::ok).map(|s| s as f32 / scale).collect()
        }
    };
    let channels = usize::from(spec.channels.max(1));
    Ok((
        spec.sample_rate,
        interleaved.chunks(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect(),
    ))
}

struct Input {
    stream: cpal::Stream,
    device: String,
}

/// Opens the default input and calls `on_audio(sample_rate, mono)` for every buffer.
fn open_input(on_audio: impl FnMut(u32, &[f32]) + Send + 'static) -> Result<Input, String> {
    let device = cpal::default_host().default_input_device().ok_or("No microphone is connected.")?;
    let supported = device.default_input_config().map_err(|error| error.to_string())?;
    let config = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, on_audio),
        SampleFormat::I16 => build::<i16>(&device, config, on_audio),
        SampleFormat::U16 => build::<u16>(&device, config, on_audio),
        SampleFormat::I32 => build::<i32>(&device, config, on_audio),
        other => return Err(format!("The microphone uses a sample format likh can't read yet ({other:?}).")),
    }?;
    stream.play().map_err(|error| error.to_string())?;
    Ok(Input { stream, device: device.to_string() })
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut on_audio: impl FnMut(u32, &[f32]) + Send + 'static,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let rate = config.sample_rate;
    let channels = usize::from(config.channels.max(1));
    let mut mono = Vec::new();
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data: &[T], _| {
                mono.clear();
                mono.extend(
                    data.chunks(channels).map(|frame| {
                        frame.iter().map(|sample| sample.to_sample::<f32>()).sum::<f32>() / channels as f32
                    }),
                );
                on_audio(rate, &mono);
            },
            |error| eprintln!("likh: microphone error: {error}"),
            None,
        )
        .map_err(|error| error.to_string())
}

/// Turns a run of samples into one level per window.
pub struct Meter {
    window: usize,
    sum_squares: f64,
    count: usize,
}

impl Meter {
    pub fn new(window: usize) -> Self {
        Self { window: window.max(1), sum_squares: 0.0, count: 0 }
    }

    pub fn push(&mut self, samples: impl Iterator<Item = f32>, mut emit: impl FnMut(f32)) {
        for sample in samples {
            self.sum_squares += f64::from(sample) * f64::from(sample);
            self.count += 1;
            if self.count == self.window {
                let rms = (self.sum_squares / self.count as f64).sqrt() as f32;
                emit(level(rms));
                self.sum_squares = 0.0;
                self.count = 0;
            }
        }
    }
}

/// Maps an RMS amplitude (0–1) onto the bars' 0–1 range on a decibel scale,
/// which is how loudness is heard.
pub fn level(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * rms.log10();
    ((db - FLOOR_DB) / (CEILING_DB - FLOOR_DB)).clamp(0.0, 1.0)
}

/// Converts between sample rates. Going down, each output sample is the mean
/// of the input samples it covers (a box filter, enough to keep speech clean);
/// going up, it interpolates linearly.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() || from == 0 {
        return input.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let out_len = (input.len() as f64 / ratio).floor() as usize;
    (0..out_len)
        .map(|j| {
            if ratio > 1.0 {
                let start = (j as f64 * ratio) as usize;
                let end = (((j + 1) as f64 * ratio) as usize).clamp(start + 1, input.len());
                input[start..end].iter().sum::<f32>() / (end - start) as f32
            } else {
                let position = j as f64 * ratio;
                let i = position as usize;
                let next = input.get(i + 1).copied().unwrap_or(input[i]);
                input[i] + (next - input[i]) * (position - i as f64) as f32
            }
        })
        .collect()
}
