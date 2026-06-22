// Batch 5 domain types: Lumetri Scopes, Project Bins, Media Items,
// Export Presets B5, and Sequence Settings B5.

// ============================================================================
// A. Lumetri Scopes State
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScopeKind { Waveform, Parade, Vectorscope, Histogram, All }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScopeLayout { OneUp, TwoUp, FourUp }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaveformType { Luma, Rgb, Yuv, Hsl }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParadeType { Rgb, Yuv }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VectorscopeType { Hls, YuvDiamond }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HistogramChannel { Rgb, Red, Green, Blue, Luminance, All }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScopeColorspace { Rec709, Rec2020, P3 }

#[derive(Clone, Debug)]
pub struct LumetriScopesConfig {
    pub enabled: bool,
    pub scope_kind: ScopeKind,
    pub layout: ScopeLayout,
    pub waveform_type: WaveformType,
    pub parade_type: ParadeType,
    pub vectorscope_type: VectorscopeType,
    pub histogram_channel: HistogramChannel,
    /// Scope overlay intensity: 10–300, default 75.
    pub intensity: f32,
    pub colorspace: ScopeColorspace,
    pub show_clipping: bool,
}

impl Default for LumetriScopesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scope_kind: ScopeKind::Waveform,
            layout: ScopeLayout::OneUp,
            waveform_type: WaveformType::Luma,
            parade_type: ParadeType::Rgb,
            vectorscope_type: VectorscopeType::Hls,
            histogram_channel: HistogramChannel::Rgb,
            intensity: 75.0,
            colorspace: ScopeColorspace::Rec709,
            show_clipping: false,
        }
    }
}

// ============================================================================
// B. Project Manager & Bins
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinColor { None, Red, Orange, Yellow, Green, Blue, Purple, Gray }

#[derive(Clone, Debug)]
pub struct ProjectBin {
    pub id: usize,
    pub name: String,
    pub parent_id: Option<usize>,
    pub color_label: BinColor,
    pub item_ids: Vec<usize>,
    pub expanded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MediaKind { Video, Audio, StillImage, Sequence, Title, AdjustmentLayer, BarsTone }

#[derive(Clone, Debug)]
pub struct MediaItem {
    pub id: usize,
    pub name: String,
    pub path: String,
    pub duration_s: f64,
    pub frame_rate: f64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    pub has_video: bool,
    pub media_kind: MediaKind,
    pub proxy_path: Option<String>,
    pub label: BinColor,
    pub offline: bool,
    pub log_note: String,
}

// ============================================================================
// D. Export Presets (Batch 5 extended model)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExportCategory {
    H264, H265, ProRes, Av1, Dnxhd, Cineform, Mpeg2,
    Youtube, Vimeo, Twitter, WebDvd, Custom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExportContainer { Mp4, Mov, Mxf, Mkv, Avi, Gif, Webm, M4v }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VideoCodecB5 {
    H264, H265,
    ProRes422, ProRes422Hq, ProRes422Lt, ProRes422Proxy, ProRes4444,
    Dnxhd, Dnxhr,
    Av1, Vp9, Mpeg2,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioCodecB5 { Aac, Mp3, Pcm, Alac, Ac3, None }

#[derive(Clone, Debug)]
pub struct ExportPresetB5 {
    pub id: usize,
    pub name: String,
    pub category: ExportCategory,
    pub container: ExportContainer,
    pub video_codec: VideoCodecB5,
    pub audio_codec: AudioCodecB5,
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    pub audio_sample_rate: u32,
    pub two_pass: bool,
    pub hardware_encode: bool,
    pub match_source: bool,
}

impl ExportPresetB5 {
    pub fn youtube_1080p() -> Self {
        Self {
            id: 1, name: "YouTube 1080p Full HD".into(),
            category: ExportCategory::Youtube,
            container: ExportContainer::Mp4,
            video_codec: VideoCodecB5::H264,
            audio_codec: AudioCodecB5::Aac,
            width: 1920, height: 1080, frame_rate: 29.97,
            video_bitrate_kbps: 16000, audio_bitrate_kbps: 320, audio_sample_rate: 48000,
            two_pass: false, hardware_encode: false, match_source: false,
        }
    }
    pub fn youtube_4k() -> Self {
        Self {
            id: 2, name: "YouTube 4K UHD".into(),
            category: ExportCategory::Youtube,
            container: ExportContainer::Mp4,
            video_codec: VideoCodecB5::H264,
            audio_codec: AudioCodecB5::Aac,
            width: 3840, height: 2160, frame_rate: 29.97,
            video_bitrate_kbps: 40000, audio_bitrate_kbps: 320, audio_sample_rate: 48000,
            two_pass: false, hardware_encode: false, match_source: false,
        }
    }
    pub fn twitter() -> Self {
        Self {
            id: 3, name: "Twitter/X".into(),
            category: ExportCategory::Twitter,
            container: ExportContainer::Mp4,
            video_codec: VideoCodecB5::H264,
            audio_codec: AudioCodecB5::Aac,
            width: 1280, height: 720, frame_rate: 30.0,
            video_bitrate_kbps: 5000, audio_bitrate_kbps: 192, audio_sample_rate: 48000,
            two_pass: false, hardware_encode: false, match_source: false,
        }
    }
    pub fn vimeo_1080p() -> Self {
        Self {
            id: 4, name: "Vimeo 1080p".into(),
            category: ExportCategory::Vimeo,
            container: ExportContainer::Mp4,
            video_codec: VideoCodecB5::H264,
            audio_codec: AudioCodecB5::Aac,
            width: 1920, height: 1080, frame_rate: 23.976,
            video_bitrate_kbps: 10000, audio_bitrate_kbps: 320, audio_sample_rate: 48000,
            two_pass: false, hardware_encode: false, match_source: false,
        }
    }
    pub fn prores_422() -> Self {
        Self {
            id: 5, name: "ProRes 422".into(),
            category: ExportCategory::ProRes,
            container: ExportContainer::Mov,
            video_codec: VideoCodecB5::ProRes422,
            audio_codec: AudioCodecB5::Pcm,
            width: 0, height: 0, frame_rate: 0.0,
            video_bitrate_kbps: 0, audio_bitrate_kbps: 0, audio_sample_rate: 48000,
            two_pass: false, hardware_encode: false, match_source: true,
        }
    }
    pub fn gif() -> Self {
        Self {
            id: 6, name: "GIF".into(),
            category: ExportCategory::Custom,
            container: ExportContainer::Gif,
            video_codec: VideoCodecB5::None,
            audio_codec: AudioCodecB5::None,
            width: 480, height: 270, frame_rate: 15.0,
            video_bitrate_kbps: 0, audio_bitrate_kbps: 0, audio_sample_rate: 0,
            two_pass: false, hardware_encode: false, match_source: false,
        }
    }
    pub fn mp3_audio() -> Self {
        Self {
            id: 7, name: "MP3 Audio Only".into(),
            category: ExportCategory::Custom,
            container: ExportContainer::Mp4,
            video_codec: VideoCodecB5::None,
            audio_codec: AudioCodecB5::Mp3,
            width: 0, height: 0, frame_rate: 0.0,
            video_bitrate_kbps: 0, audio_bitrate_kbps: 192, audio_sample_rate: 48000,
            two_pass: false, hardware_encode: false, match_source: false,
        }
    }
}

// ============================================================================
// E. Multi-Sequence Settings (Batch 5)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldOrder { Progressive, UpperFirst, LowerFirst }

#[derive(Clone, Debug)]
pub struct SequenceSettingsB5 {
    pub id: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    pub pixel_aspect_ratio: f64,
    pub field_order: FieldOrder,
    pub sample_rate: u32,
    pub audio_channels: u32,
    pub preview_codec: String,
    pub preview_frame_size: (u32, u32),
    pub timebase: f64,
    pub working_color_space: String,
}

impl SequenceSettingsB5 {
    pub fn default_1080p(id: usize, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            width: 1920,
            height: 1080,
            frame_rate: 29.97,
            pixel_aspect_ratio: 1.0,
            field_order: FieldOrder::Progressive,
            sample_rate: 48000,
            audio_channels: 2,
            preview_codec: "I-Frame Only MPEG".into(),
            preview_frame_size: (1920, 1080),
            timebase: 29.97,
            working_color_space: "Rec.709".into(),
        }
    }
}
