//! Audio engine domain — device management, buffer/sample-rate config, engine state.

use super::{Action, App};

// ─── Types ────────────────────────────────────────────────────────────────────

pub struct AudioEngineConfig {
    pub device_name: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub input_latency_ms: f32,
    pub output_latency_ms: f32,
    pub input_channels: u32,
    pub output_channels: u32,
    pub exclusive_mode: bool,
    pub low_latency_mode: bool,
}

impl AudioEngineConfig {
    pub fn new() -> Self {
        Self {
            device_name: "Default".to_string(),
            sample_rate: 48000,
            buffer_size: 256,
            input_latency_ms: 5.0,
            output_latency_ms: 5.0,
            input_channels: 2,
            output_channels: 2,
            exclusive_mode: false,
            low_latency_mode: false,
        }
    }
}

impl Default for AudioEngineConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AudioInputDevice {
    pub id: usize,
    pub name: String,
    pub channel_count: u32,
    pub is_default: bool,
}

pub struct AudioOutputDevice {
    pub id: usize,
    pub name: String,
    pub channel_count: u32,
    pub is_default: bool,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

const VALID_BUFFER_SIZES: [u32; 5] = [64, 128, 256, 512, 1024];
const VALID_SAMPLE_RATES: [u32; 4] = [44100, 48000, 88200, 96000];

fn clamp_buffer_size(requested: u32) -> u32 {
    for &size in &VALID_BUFFER_SIZES {
        if size >= requested {
            return size;
        }
    }
    1024
}

// ─── Apply methods ────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_audio_engine(&mut self, action: Action) {
        match action {
            Action::SetAudioDevice { input, output } => {
                if let Some(name) = input {
                    if let Some(dev) = self.input_devices.iter().find(|d| d.name == name) {
                        self.active_input_device_id = Some(dev.id);
                    }
                }
                if let Some(name) = output {
                    if let Some(dev) = self.output_devices.iter().find(|d| d.name == name) {
                        self.active_output_device_id = Some(dev.id);
                        self.audio_config.device_name = dev.name.clone();
                    }
                }
            }
            Action::SetBufferSize(size) => {
                self.audio_config.buffer_size = clamp_buffer_size(size);
            }
            Action::SetAudioSampleRate(rate) => {
                if VALID_SAMPLE_RATES.contains(&rate) {
                    self.audio_config.sample_rate = rate;
                }
            }
            Action::SetLowLatencyMode(enabled) => {
                self.audio_config.low_latency_mode = enabled;
            }
            Action::SetExclusiveMode(enabled) => {
                self.audio_config.exclusive_mode = enabled;
            }
            Action::StartAudioEngine => {
                self.audio_engine_running = true;
            }
            Action::StopAudioEngine => {
                self.audio_engine_running = false;
            }
            Action::ReportAudioOverload => {
                self.audio_overload_count += 1;
            }
            Action::ResetOverloadCount => {
                self.audio_overload_count = 0;
            }
            Action::AddInputDevice { name, channels, is_default } => {
                let id = self.input_devices.len();
                let set_active = is_default && self.active_input_device_id.is_none();
                self.input_devices.push(AudioInputDevice {
                    id,
                    name,
                    channel_count: channels,
                    is_default,
                });
                if set_active {
                    self.active_input_device_id = Some(id);
                }
            }
            Action::AddOutputDevice { name, channels, is_default } => {
                let id = self.output_devices.len();
                let set_active = is_default && self.active_output_device_id.is_none();
                self.output_devices.push(AudioOutputDevice {
                    id,
                    name,
                    channel_count: channels,
                    is_default,
                });
                if set_active {
                    self.active_output_device_id = Some(id);
                }
            }
            Action::SelectInputDevice { device_id } => {
                self.active_input_device_id = device_id;
            }
            Action::SelectOutputDevice { device_id } => {
                self.active_output_device_id = device_id;
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn audio_engine_defaults() {
        let app = fresh();
        assert_eq!(app.audio_config.sample_rate, 48000);
        assert_eq!(app.audio_config.buffer_size, 256);
        assert_eq!(app.audio_config.device_name, "Default");
        assert!(!app.audio_engine_running);
        assert_eq!(app.audio_overload_count, 0);
        assert!(app.active_input_device_id.is_none());
        assert!(app.active_output_device_id.is_none());
    }

    #[test]
    fn set_buffer_size_exact() {
        let mut app = fresh();
        app.apply(Action::SetBufferSize(256));
        assert_eq!(app.audio_config.buffer_size, 256);
    }

    #[test]
    fn set_buffer_size_clamp_up() {
        let mut app = fresh();
        app.apply(Action::SetBufferSize(100));
        assert_eq!(app.audio_config.buffer_size, 128);
    }

    #[test]
    fn set_buffer_size_clamp_max() {
        let mut app = fresh();
        app.apply(Action::SetBufferSize(2048));
        assert_eq!(app.audio_config.buffer_size, 1024);
    }

    #[test]
    fn set_buffer_size_minimum() {
        let mut app = fresh();
        app.apply(Action::SetBufferSize(1));
        assert_eq!(app.audio_config.buffer_size, 64);
    }

    #[test]
    fn set_audio_sample_rate_valid() {
        let mut app = fresh();
        for &rate in &[44100u32, 48000, 88200, 96000] {
            app.apply(Action::SetAudioSampleRate(rate));
            assert_eq!(app.audio_config.sample_rate, rate);
        }
    }

    #[test]
    fn set_audio_sample_rate_invalid_noop() {
        let mut app = fresh();
        app.apply(Action::SetAudioSampleRate(22050));
        assert_eq!(app.audio_config.sample_rate, 48000);
    }

    #[test]
    fn start_stop_audio_engine() {
        let mut app = fresh();
        assert!(!app.audio_engine_running);
        app.apply(Action::StartAudioEngine);
        assert!(app.audio_engine_running);
        app.apply(Action::StopAudioEngine);
        assert!(!app.audio_engine_running);
    }

    #[test]
    fn report_audio_overload_increments() {
        let mut app = fresh();
        app.apply(Action::ReportAudioOverload);
        app.apply(Action::ReportAudioOverload);
        assert_eq!(app.audio_overload_count, 2);
    }

    #[test]
    fn reset_overload_count() {
        let mut app = fresh();
        app.apply(Action::ReportAudioOverload);
        app.apply(Action::ReportAudioOverload);
        app.apply(Action::ResetOverloadCount);
        assert_eq!(app.audio_overload_count, 0);
    }

    #[test]
    fn add_input_device_sets_active_if_default() {
        let mut app = fresh();
        app.apply(Action::AddInputDevice {
            name: "Scarlett 2i2".to_string(),
            channels: 2,
            is_default: true,
        });
        assert_eq!(app.input_devices.len(), 1);
        assert_eq!(app.input_devices[0].name, "Scarlett 2i2");
        assert_eq!(app.active_input_device_id, Some(0));
    }

    #[test]
    fn add_output_device_sets_active_if_default() {
        let mut app = fresh();
        app.apply(Action::AddOutputDevice {
            name: "Built-in Output".to_string(),
            channels: 2,
            is_default: true,
        });
        assert_eq!(app.output_devices.len(), 1);
        assert_eq!(app.active_output_device_id, Some(0));
    }

    #[test]
    fn select_input_device() {
        let mut app = fresh();
        app.apply(Action::AddInputDevice { name: "Dev A".to_string(), channels: 2, is_default: false });
        app.apply(Action::AddInputDevice { name: "Dev B".to_string(), channels: 2, is_default: false });
        app.apply(Action::SelectInputDevice { device_id: Some(1) });
        assert_eq!(app.active_input_device_id, Some(1));
        app.apply(Action::SelectInputDevice { device_id: None });
        assert_eq!(app.active_input_device_id, None);
    }

    #[test]
    fn select_output_device() {
        let mut app = fresh();
        app.apply(Action::AddOutputDevice { name: "Out A".to_string(), channels: 2, is_default: false });
        app.apply(Action::SelectOutputDevice { device_id: Some(0) });
        assert_eq!(app.active_output_device_id, Some(0));
    }

    #[test]
    fn set_audio_device_by_name() {
        let mut app = fresh();
        app.apply(Action::AddOutputDevice {
            name: "Focusrite".to_string(),
            channels: 2,
            is_default: false,
        });
        app.apply(Action::SetAudioDevice {
            input: None,
            output: Some("Focusrite".to_string()),
        });
        assert_eq!(app.active_output_device_id, Some(0));
        assert_eq!(app.audio_config.device_name, "Focusrite");
    }

    #[test]
    fn set_low_latency_mode() {
        let mut app = fresh();
        assert!(!app.audio_config.low_latency_mode);
        app.apply(Action::SetLowLatencyMode(true));
        assert!(app.audio_config.low_latency_mode);
        app.apply(Action::SetLowLatencyMode(false));
        assert!(!app.audio_config.low_latency_mode);
    }

    #[test]
    fn set_exclusive_mode() {
        let mut app = fresh();
        assert!(!app.audio_config.exclusive_mode);
        app.apply(Action::SetExclusiveMode(true));
        assert!(app.audio_config.exclusive_mode);
    }
}
