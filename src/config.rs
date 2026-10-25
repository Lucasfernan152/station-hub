use serde::{Deserialize, Serialize};

use crate::devices::{DeviceSpec, PortMatch};

pub const DEFAULT_BIND: &str = "127.0.0.1:17321";

#[derive(Debug, Deserialize)]
pub struct FileConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    pub device: Vec<DeviceConfig>,
}

fn default_bind() -> String {
    DEFAULT_BIND.into()
}

#[derive(Debug, Deserialize)]
pub struct DeviceConfig {
    pub id: String,
    pub kind: DeviceKind,
    pub name: Option<String>,
    pub vid: Option<u16>,
    pub pid: Option<u16>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Scanner,
    Scale,
}

impl FileConfig {
    pub fn parse(raw: &str) -> Result<Self, String> {
        toml::from_str(raw).map_err(|e| format!("toml: {e}"))
    }

    pub fn load_path(path: &str) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        Self::parse(&raw).map_err(|e| format!("{path}: {e}"))
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

#[cfg(test)]
mod tests {
    use super::{DeviceKind, FileConfig};
    use crate::devices::PortMatch;

    #[test]
    fn parses_name_and_usb_devices() {
        let cfg = FileConfig::parse(
            r#"
bind = "127.0.0.1:9"
[[device]]
id = "scanner-1"
kind = "scanner"
name = "COM10"
[[device]]
id = "scale-1"
kind = "scale"
vid = 0x0EB8
pid = 1
"#,
        )
        .unwrap();
        assert_eq!(cfg.bind, "127.0.0.1:9");
        let mut devices = cfg.device;
        let (kind, spec) = devices.remove(0).spec().unwrap();
        assert_eq!(kind, DeviceKind::Scanner);
        assert_eq!(spec.port, PortMatch::Name("COM10".into()));
        let (kind, spec) = devices.remove(0).spec().unwrap();
        assert_eq!(kind, DeviceKind::Scale);
        assert_eq!(spec.port, PortMatch::Usb { vid: 0x0EB8, pid: 1 });
    }

    #[test]
    fn rejects_mixed_name_and_usb() {
        let cfg = FileConfig::parse(
            r#"
[[device]]
id = "bad"
kind = "scanner"
name = "COM1"
vid = 1
pid = 2
"#,
        )
        .unwrap();
        assert!(cfg.device.into_iter().next().unwrap().spec().is_err());
    }
}