use serde::Deserialize;

use crate::devices::{DeviceSpec, PortMatch};

#[derive(Debug, Deserialize)]
pub struct FileConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    pub device: Vec<DeviceConfig>
}

fn default_bind() -> String {
    "127.0.0.1:17321".into()
}

#[derive(Debug, Deserialize)]
pub struct DeviceConfig {
    pub id: String,
    pub kind: DeviceKind,
    pub name: Option<String>,
    pub vid: Option<u16>,
    pub pid: Option<u16>
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Scanner,
    Scale
}

impl FileConfig {
    pub fn load() -> Result<Self, String> {
        let raw = std::fs::read_to_string("station.toml")
            .map_err(|e| format!("station.toml: {e}"))?;
        toml::from_str(&raw).map_err(|e| format!("station.toml: {e}"))
    }
}

impl DeviceConfig {
    pub fn spec(self) -> Result<(DeviceKind, DeviceSpec), String> {
        let port = match (self.name, self.vid, self.pid) {
            (Some(name), None, None)  => PortMatch::Name(name),
            (None, Some(vid), Some(pid)) => PortMatch::Usb { vid, pid },
            _ => {
                return Err(format!(
                    "{}: set name= or vid+pid, not both or none",
                    self.id
                ));
            }
        };
        Ok((self.kind, DeviceSpec { id: self.id, port }))
    }
}