//! Model download manager for Tone.
//!
//! On first use of any AI feature, the required ONNX model is downloaded
//! from its public URL to the local cache dir (~/.local/share/prism-suite/models/).
//! Subsequent runs skip the download if the file already exists.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;

/// Canonical cache directory for all Prism model files.
pub fn models_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("prism-suite")
        .join("models")
}

/// Every AI model Tone can use.
#[derive(Clone, Debug, PartialEq)]
pub enum ToneModelId {
    MusicGenSmall,
    DemucsHybrid,
    AiMasterNet,
    MelodyRnn,
}

impl ToneModelId {
    /// File name stored under `models_dir()`.
    pub fn filename(&self) -> &'static str {
        match self {
            Self::MusicGenSmall => "musicgen-small-decoder.onnx",
            Self::DemucsHybrid  => "demucs-hybrid-transformer.th",
            Self::AiMasterNet   => "ai-master-net.onnx",
            Self::MelodyRnn     => "melody-rnn.onnx",
        }
    }

    /// Public download URL (no auth required).
    pub fn url(&self) -> &'static str {
        match self {
            Self::MusicGenSmall =>
                "https://huggingface.co/Xenova/musicgen-small/resolve/main/onnx/decoder_model_merged.onnx",
            Self::DemucsHybrid =>
                "https://dl.fbaipublicfiles.com/demucs/hybrid_transformer/955717e8-8726e21a.th",
            Self::AiMasterNet =>
                "https://huggingface.co/prism-suite/models/resolve/main/ai-master-net.onnx",
            Self::MelodyRnn =>
                "https://huggingface.co/Xenova/melody-rnn/resolve/main/onnx/model.onnx",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::MusicGenSmall => "MusicGen Small",
            Self::DemucsHybrid  => "Demucs Hybrid",
            Self::AiMasterNet   => "AI Master Net",
            Self::MelodyRnn     => "Melody RNN",
        }
    }

    /// Approximate download size in MB (shown in UI before download starts).
    pub fn size_mb(&self) -> u64 {
        match self {
            Self::MusicGenSmall => 490,
            Self::DemucsHybrid  => 80,
            Self::AiMasterNet   => 45,
            Self::MelodyRnn     => 60,
        }
    }

    pub fn local_path(&self) -> PathBuf {
        models_dir().join(self.filename())
    }

    pub fn is_downloaded(&self) -> bool {
        self.local_path().exists()
    }
}

/// Events streamed from the download thread back to the UI thread.
pub enum DownloadEvent {
    /// How many bytes done out of total (total = 0 if server didn't send Content-Length).
    Progress { bytes_done: u64, bytes_total: u64 },
    Done,
    Error(String),
}

/// A live download: the model being fetched + the channel to drain for events.
pub struct ModelDownloadHandle {
    pub model_id: ToneModelId,
    pub rx: mpsc::Receiver<DownloadEvent>,
}

/// Kick off a background download for `model`. Returns immediately; the caller
/// should store the handle and drain it in the render loop via `try_recv`.
pub fn start_download(model: ToneModelId) -> ModelDownloadHandle {
    let (tx, rx) = mpsc::channel();
    let m = model.clone();

    std::thread::spawn(move || {
        let path = m.local_path();
        let url  = m.url();

        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let _ = tx.send(DownloadEvent::Error(format!("mkdir: {e}")));
                return;
            }
        }

        let resp = match ureq::get(url).call() {
            Ok(r)  => r,
            Err(e) => {
                let _ = tx.send(DownloadEvent::Error(format!("HTTP: {e}")));
                return;
            }
        };

        let total: u64 = resp
            .header("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let mut reader = resp.into_reader();
        let mut file = match std::fs::File::create(&path) {
            Ok(f)  => f,
            Err(e) => {
                let _ = tx.send(DownloadEvent::Error(format!("create file: {e}")));
                return;
            }
        };

        let mut done = 0u64;
        let mut buf  = vec![0u8; 65_536];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Err(e) = file.write_all(&buf[..n]) {
                        let _ = tx.send(DownloadEvent::Error(format!("write: {e}")));
                        // Remove partial file
                        let _ = std::fs::remove_file(&path);
                        return;
                    }
                    done += n as u64;
                    let _ = tx.send(DownloadEvent::Progress { bytes_done: done, bytes_total: total });
                }
                Err(e) => {
                    let _ = tx.send(DownloadEvent::Error(format!("read: {e}")));
                    let _ = std::fs::remove_file(&path);
                    return;
                }
            }
        }
        let _ = tx.send(DownloadEvent::Done);
    });

    ModelDownloadHandle { model_id: model, rx }
}
