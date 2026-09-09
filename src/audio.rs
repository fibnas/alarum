use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Plays an alarm sound in a loop until `stop()` is called.
pub struct SoundPlayer {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    child_slot: Arc<std::sync::Mutex<Option<Child>>>,
}

impl SoundPlayer {
    pub fn play(path: Option<&Path>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let child_slot: Arc<std::sync::Mutex<Option<Child>>> = Arc::new(std::sync::Mutex::new(None));
        let stop_t = stop.clone();
        let slot_t = child_slot.clone();
        let path = path.map(PathBuf::from);

        let worker = thread::spawn(move || {
            let default_path = default_sound_path();
            let chosen = path
                .filter(|p| p.is_file())
                .unwrap_or(default_path);

            while !stop_t.load(Ordering::SeqCst) {
                match spawn_player(&chosen) {
                    Some(child) => {
                        {
                            let mut slot = slot_t.lock().unwrap();
                            *slot = Some(child);
                        }
                        // Poll so we can abort promptly.
                        loop {
                            if stop_t.load(Ordering::SeqCst) {
                                if let Ok(mut slot) = slot_t.lock() {
                                    if let Some(ref mut c) = *slot {
                                        let _ = c.kill();
                                        let _ = c.wait();
                                    }
                                    *slot = None;
                                }
                                return;
                            }
                            let finished = {
                                let mut slot = slot_t.lock().unwrap();
                                match slot.as_mut() {
                                    Some(c) => match c.try_wait() {
                                        Ok(Some(_)) => true,
                                        Ok(None) => false,
                                        Err(_) => true,
                                    },
                                    None => true,
                                }
                            };
                            if finished {
                                let mut slot = slot_t.lock().unwrap();
                                *slot = None;
                                break;
                            }
                            thread::sleep(Duration::from_millis(120));
                        }
                    }
                    None => {
                        // Nothing we can play; avoid a hot loop.
                        thread::sleep(Duration::from_millis(800));
                    }
                }
            }
        });

        Self {
            stop,
            worker: Some(worker),
            child_slot,
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.child_slot.lock() {
            if let Some(ref mut child) = *slot {
                let _ = child.kill();
                let _ = child.wait();
            }
            *slot = None;
        }
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for SoundPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn spawn_player(path: &Path) -> Option<Child> {
    let path_str = path.to_string_lossy().to_string();

    // Prefer players that exist on Fedora/XFCE/PipeWire systems.
    let attempts: &[(&str, Vec<String>)] = &[
        (
            "mpv",
            vec![
                "--no-video".into(),
                "--really-quiet".into(),
                "--audio-display=no".into(),
                path_str.clone(),
            ],
        ),
        ("pw-play", vec![path_str.clone()]),
        ("paplay", vec![path_str.clone()]),
        (
            "ffplay",
            vec![
                "-nodisp".into(),
                "-autoexit".into(),
                "-loglevel".into(),
                "quiet".into(),
                path_str.clone(),
            ],
        ),
        ("ffplay", vec!["-nodisp".into(), "-autoexit".into(), path_str.clone()]),
        ("aplay", vec!["-q".into(), path_str.clone()]),
    ];

    for (bin, args) in attempts {
        if which(bin) {
            match Command::new(bin)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => return Some(child),
                Err(_) => continue,
            }
        }
    }
    None
}

fn which(bin: &str) -> bool {
    Command::new("which")
        .arg(bin)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Writes a short built-in two-tone WAV and returns its path.
pub fn default_sound_path() -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("alarum");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("default-alarm.wav");
    if !path.exists() {
        if let Ok(bytes) = build_default_wav() {
            if let Ok(mut f) = std::fs::File::create(&path) {
                let _ = f.write_all(&bytes);
            }
        }
    }
    path
}

/// 2.4s of alternating 880 Hz / 660 Hz beeps, 16-bit PCM mono 44.1 kHz.
fn build_default_wav() -> std::io::Result<Vec<u8>> {
    const SAMPLE_RATE: u32 = 44_100;
    const DURATION_SECS: f32 = 2.4;
    let total = (SAMPLE_RATE as f32 * DURATION_SECS) as usize;
    let mut pcm = Vec::with_capacity(total * 2);

    for n in 0..total {
        let t = n as f32 / SAMPLE_RATE as f32;
        // Four 0.3s on / 0.3s off pairs.
        let cycle = t % 0.6;
        let on = cycle < 0.28;
        let freq = if ((t / 0.6) as i32) % 2 == 0 { 880.0 } else { 660.0 };
        let sample = if on {
            let env = ((cycle / 0.28) * std::f32::consts::PI).sin().clamp(0.0, 1.0);
            (env * 0.55 * (2.0 * std::f32::consts::PI * freq * t).sin() * i16::MAX as f32) as i16
        } else {
            0
        };
        pcm.extend_from_slice(&sample.to_le_bytes());
    }

    let data_len = pcm.len() as u32;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // audio format = PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // channels
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&pcm);
    Ok(out)
}

pub fn notify(title: &str, body: &str) {
    if which("notify-send") {
        let _ = Command::new("notify-send")
            .args(["--urgency=critical", "--app-name=Alarum", title, body])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

pub fn pick_audio_file() -> Option<PathBuf> {
    // Prefer portal/GTK pickers that are common on Fedora XFCE.
    let commands: [Vec<&str>; 3] = [
        vec![
            "zenity",
            "--file-selection",
            "--title=Choose alarm sound",
            "--file-filter=Audio files | *.wav *.mp3 *.ogg *.oga *.flac *.m4a *.aac",
        ],
        vec![
            "yad",
            "--file",
            "--title=Choose alarm sound",
            "--file-filter=Audio | *.wav *.mp3 *.ogg *.oga *.flac",
        ],
        vec!["kdialog", "--getopenfilename", ".", "*.wav *.mp3 *.ogg *.oga *.flac"],
    ];

    for args in commands {
        if !which(args[0]) {
            continue;
        }
        let output = Command::new(args[0]).args(&args[1..]).output().ok()?;
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !text.is_empty() {
                return Some(PathBuf::from(text));
            }
        }
    }
    None
}

pub fn preview_sound(path: Option<&Path>) {
    let default = default_sound_path();
    let chosen = path.filter(|p| p.is_file()).unwrap_or(default.as_path());
    let _ = spawn_player(chosen);
}
