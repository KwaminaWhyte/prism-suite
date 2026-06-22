use super::*;

#[derive(Clone, Debug, PartialEq)]
pub enum MotionEasing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Hold,
}

#[derive(Clone, Debug)]
pub struct MotionPathPoint {
    pub time_s: f32,
    pub x: f32,
    pub y: f32,
    pub in_handle: (f32, f32),
    pub out_handle: (f32, f32),
    pub easing: MotionEasing,
}

#[derive(Clone, Debug)]
pub struct MotionPath {
    pub id: usize,
    pub layer_id: usize,
    pub points: Vec<MotionPathPoint>,
    pub closed: bool,
    pub auto_orient: bool,
    pub orient_smoothness: f32,
}
