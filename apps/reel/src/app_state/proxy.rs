use super::{App, Action};

/// Proxy format for low-res substitutes.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ProxyFormat {
    #[default] ProRes422Proxy, H264Proxy, DnxHd36, Mjpeg,
}

/// Proxy creation settings.
#[derive(Debug, Clone)]
pub struct ProxySettings {
    pub format: ProxyFormat,
    pub scale: f32,
    pub destination: std::path::PathBuf,
    pub create_in_background: bool,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            format: ProxyFormat::default(),
            scale: 0.25,
            destination: std::path::PathBuf::from("Proxies"),
            create_in_background: false,
        }
    }
}

/// Per-clip proxy attachment record.
#[derive(Debug, Clone, Default)]
pub struct ClipProxy {
    pub clip_idx: usize,
    pub proxy_path: std::path::PathBuf,
    pub attached: bool,
}

pub trait AppProxyExt {
    fn apply_proxy(&mut self, action: Action);
}

impl AppProxyExt for App {
    fn apply_proxy(&mut self, action: Action) {
        match action {
            Action::ToggleProxyIngestPanel => { self.proxy_ingest_open = !self.proxy_ingest_open; }
            Action::SetProxyFormat(f) => { self.proxy_settings.format = f; }
            Action::SetProxyScale(v) => { self.proxy_settings.scale = v.clamp(0.1, 1.0); }
            Action::SetProxyDestination(p) => { self.proxy_settings.destination = p; }
            Action::SetProxyCreateInBackground(v) => { self.proxy_settings.create_in_background = v; }
            Action::CreateProxies { clip_indices } => {
                for idx in clip_indices {
                    let proxy_path = std::path::PathBuf::from(format!("Proxies/clip_{}.mp4", idx));
                    if !self.clip_proxies.iter().any(|cp| cp.clip_idx == idx) {
                        self.clip_proxies.push(ClipProxy { clip_idx: idx, proxy_path, attached: false });
                    }
                }
            }
            Action::AttachProxy { clip_idx, path } => {
                if let Some(cp) = self.clip_proxies.iter_mut().find(|cp| cp.clip_idx == clip_idx) {
                    cp.proxy_path = path;
                    cp.attached = true;
                } else {
                    self.clip_proxies.push(ClipProxy { clip_idx, proxy_path: path, attached: true });
                }
            }
            Action::DetachProxy { clip_idx } => {
                if let Some(cp) = self.clip_proxies.iter_mut().find(|cp| cp.clip_idx == clip_idx) {
                    cp.attached = false;
                }
            }
            Action::ToggleProxyPlayback => { self.toggle_proxy_enabled = !self.toggle_proxy_enabled; }
            Action::DeleteProxies { clip_indices } => {
                self.clip_proxies.retain(|cp| !clip_indices.contains(&cp.clip_idx));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};

    #[test]
    fn test_proxy_scale_clamp() {
        let mut app = App::new();
        // Below minimum → 0.1.
        app.apply(Action::SetProxyScale(0.0));
        assert_eq!(app.proxy_settings.scale, 0.1);
        // Above maximum → 1.0.
        app.apply(Action::SetProxyScale(2.0));
        assert_eq!(app.proxy_settings.scale, 1.0);
        // Within range.
        app.apply(Action::SetProxyScale(0.5));
        assert_eq!(app.proxy_settings.scale, 0.5);
    }

    #[test]
    fn test_create_proxies_pushes_entries() {
        let mut app = App::new();
        assert_eq!(app.clip_proxies.len(), 0);
        app.apply(Action::CreateProxies { clip_indices: vec![0, 2, 5] });
        assert_eq!(app.clip_proxies.len(), 3);
        assert_eq!(app.clip_proxies[0].clip_idx, 0);
        assert_eq!(app.clip_proxies[1].clip_idx, 2);
        assert_eq!(app.clip_proxies[2].clip_idx, 5);
        // All newly created proxies start unattached.
        assert!(!app.clip_proxies[0].attached);
        // Duplicate clip_idx should not add a second entry.
        app.apply(Action::CreateProxies { clip_indices: vec![0] });
        assert_eq!(app.clip_proxies.len(), 3);
    }

    #[test]
    fn test_attach_proxy_sets_flag() {
        let mut app = App::new();
        let p = std::path::PathBuf::from("Proxies/clip_0.mp4");
        app.apply(Action::AttachProxy { clip_idx: 0, path: p.clone() });
        assert_eq!(app.clip_proxies.len(), 1);
        assert!(app.clip_proxies[0].attached);
        assert_eq!(app.clip_proxies[0].proxy_path, p);
    }

    #[test]
    fn test_detach_proxy() {
        let mut app = App::new();
        app.apply(Action::AttachProxy {
            clip_idx: 1,
            path: std::path::PathBuf::from("Proxies/clip_1.mp4"),
        });
        assert!(app.clip_proxies[0].attached);
        app.apply(Action::DetachProxy { clip_idx: 1 });
        assert!(!app.clip_proxies[0].attached);
    }

    #[test]
    fn test_toggle_proxy_playback() {
        let mut app = App::new();
        assert!(!app.toggle_proxy_enabled);
        app.apply(Action::ToggleProxyPlayback);
        assert!(app.toggle_proxy_enabled);
        app.apply(Action::ToggleProxyPlayback);
        assert!(!app.toggle_proxy_enabled);
    }
}
