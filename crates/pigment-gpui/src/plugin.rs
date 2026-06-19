//! Plugin system stub — defines the `Plugin` trait, `PluginRegistry`, and one
//! built-in example plugin (`InvertPlugin`).

/// The pixel buffer of a layer: RGBA linear-light f32, interleaved, row-major,
/// length = width × height × 4.
pub type LayerPixels = Vec<f32>;

/// A plugin that transforms a layer's pixels in place given JSON params.
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn apply(&self, layer: &mut LayerPixels, params: &serde_json::Value) -> Result<(), String>;
}

/// Registry of registered plugins.
pub struct PluginRegistry {
    pub plugins: Vec<Box<dyn Plugin>>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        let mut r = Self { plugins: Vec::new() };
        r.register(Box::new(InvertPlugin));
        r
    }
}

impl PluginRegistry {
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    pub fn run(&self, name: &str, layer: &mut LayerPixels, params: &serde_json::Value) -> Result<(), String> {
        for plugin in &self.plugins {
            if plugin.name() == name {
                return plugin.apply(layer, params);
            }
        }
        Err(format!("plugin '{name}' not found"))
    }

    pub fn names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.name()).collect()
    }
}

/// Built-in example plugin: inverts all pixel RGB values (alpha unchanged).
pub struct InvertPlugin;

impl Plugin for InvertPlugin {
    fn name(&self) -> &str {
        "Invert"
    }

    fn apply(&self, layer: &mut LayerPixels, _params: &serde_json::Value) -> Result<(), String> {
        let n = layer.len();
        let mut i = 0;
        while i + 3 < n {
            layer[i]     = 1.0 - layer[i];
            layer[i + 1] = 1.0 - layer[i + 1];
            layer[i + 2] = 1.0 - layer[i + 2];
            i += 4;
        }
        Ok(())
    }
}
