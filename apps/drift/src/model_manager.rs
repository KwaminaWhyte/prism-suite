//! Model download manager for Drift.

// Several model variants and helpers are stubs pending Phase 3 AI wiring.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;

pub fn models_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("prism-suite")
        .join("models")
}

#[derive(Clone, Debug, PartialEq)]
pub enum DriftModelId {
    AnimateDiff,
    FilmRife,
    Wav2Vec2,
    StyleTransfer,
}

impl DriftModelId {
    pub fn filename(&self) -> &'static str {
        match self {
            Self::AnimateDiff   => "animatediff-lightning-4step.onnx",
            Self::FilmRife      => "rife-v4.onnx",
            Self::Wav2Vec2      => "wav2vec2-base.onnx",
            Self::StyleTransfer => "style-transfer.onnx",
        }
    }

    pub fn url(&self) -> &'static str {
        match self {
            Self::AnimateDiff =>
                "https://huggingface.co/ByteDance/AnimateDiff-Lightning/resolve/main/animatediff_lightning_4step_diffusers.safetensors",
            Self::FilmRife =>
                "https://huggingface.co/NaJiChao/RIFE-ONNX/resolve/main/rife_v4.onnx",
            Self::Wav2Vec2 =>
                "https://huggingface.co/Xenova/wav2vec2-base-960h/resolve/main/onnx/model.onnx",
            Self::StyleTransfer =>
                "https://huggingface.co/prism-suite/models/resolve/main/style-transfer.onnx",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::AnimateDiff   => "AnimateDiff Lightning",
            Self::FilmRife      => "RIFE Frame Interpolation",
            Self::Wav2Vec2      => "wav2vec2 Lip Sync",
            Self::StyleTransfer => "Style Transfer",
        }
    }

    pub fn size_mb(&self) -> u64 {
        match self {
            Self::AnimateDiff   => 1600,
            Self::FilmRife      => 30,
            Self::Wav2Vec2      => 360,
            Self::StyleTransfer => 55,
        }
    }

    pub fn local_path(&self) -> PathBuf {
        models_dir().join(self.filename())
    }

    pub fn is_downloaded(&self) -> bool {
        self.local_path().exists()
    }
}

pub enum DownloadEvent {
    Progress { bytes_done: u64, bytes_total: u64 },
    Done,
    Error(String),
}

pub struct ModelDownloadHandle {
    pub model_id: DriftModelId,
    pub rx: mpsc::Receiver<DownloadEvent>,
}

pub fn start_download(model: DriftModelId) -> ModelDownloadHandle {
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
