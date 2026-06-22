use super::*;

#[derive(Clone, Debug)]
pub struct AudioBus {
    pub id: usize,
    pub name: String,
    pub volume: f32,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
    pub sends: Vec<(usize, f32)>,
    pub eq_enabled: bool,
    pub eq_low: f32,
    pub eq_mid: f32,
    pub eq_high: f32,
    pub compressor_threshold: f32,
    pub compressor_ratio: f32,
}

impl AudioBus {
    pub fn new(id: usize, name: String) -> Self {
        Self {
            id,
            name,
            volume: 1.0,
            pan: 0.0,
            muted: false,
            solo: false,
            sends: Vec::new(),
            eq_enabled: false,
            eq_low: 0.0,
            eq_mid: 0.0,
            eq_high: 0.0,
            compressor_threshold: -18.0,
            compressor_ratio: 4.0,
        }
    }
}
