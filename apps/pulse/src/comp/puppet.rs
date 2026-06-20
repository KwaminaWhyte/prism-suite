//! Puppet warp pins: layer-local anchor points used by the puppet deformation pass.

use serde::{Deserialize, Serialize};

pub type PinId = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuppetPin {
    pub id: PinId,
    pub position: [f32; 2], // layer-local coords
    pub is_stiff: bool,
    #[serde(default)]
    pub stiffness: f32,
}
