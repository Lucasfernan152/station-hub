use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::DeviceKind;
use crate::devices::{DeviceSpec, PortMatch};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedDevice {
    pub id: String,
    pub kind: DeviceKind,
    pub vid: Option<u16>,
    pub pid: Option<u16>,
    pub last_port: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DeviceStore {
    pub devices: Vec<LearnedDevice>,
}

impl DeviceStore {
    pub fn path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Geco")
            .join("devices.json")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self) {
        self.save_to(&Self::path());
    }

    pub fn save_to(&self, path: &std::path::Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, raw);
        }
    }

    pub fn next_id(&self, kind: DeviceKind) -> String {
        let prefix = match kind {
            DeviceKind::Scanner => "scanner",
            DeviceKind::Scale => "scale",
        };
        let n = self
            .devices
            .iter()
            .filter(|d| d.kind == kind)
            .count()
            + 1;
        format!("{prefix}-{n}")
    }

    pub fn upsert(&mut self, device: LearnedDevice) {
        if let Some(existing) = self.devices.iter_mut().find(|d| d.id == device.id) {
            *existing = device;
        } else {
            self.devices.push(device);
        }
        self.save();
    }

    pub fn match_port(&self, name: &str, vid: Option<u16>, pid: Option<u16>) -> Option<&LearnedDevice> {
        self.devices.iter().find(|d| match (d.vid, d.pid, vid, pid) {
            (Some(sv), Some(sp), Some(pv), Some(pp)) if sv == pv && sp == pp => true,
            _ => d.last_port == name,
        })
    }
}

impl LearnedDevice {
    pub fn spec(&self) -> DeviceSpec {
        let port = match (self.vid, self.pid) {
            (Some(vid), Some(pid)) => PortMatch::Usb { vid, pid },
            _ => PortMatch::Name(self.last_port.clone()),
        };
        DeviceSpec {
            id: self.id.clone(),
            port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceStore, LearnedDevice};
    use crate::config::DeviceKind;
    use crate::devices::PortMatch;

    fn scanner(id: &str, port: &str) -> LearnedDevice {
        LearnedDevice {
            id: id.into(),
            kind: DeviceKind::Scanner,
            vid: Some(0x0C2E),
            pid: Some(1),
            last_port: port.into(),
        }
    }

    #[test]
    fn next_id_counts_per_kind() {
        let mut store = DeviceStore::default();
        assert_eq!(store.next_id(DeviceKind::Scanner), "scanner-1");
        store.devices.push(scanner("scanner-1", "COM10"));
        assert_eq!(store.next_id(DeviceKind::Scanner), "scanner-2");
        assert_eq!(store.next_id(DeviceKind::Scale), "scale-1");
    }

    #[test]
    fn match_prefers_usb_ids() {
        let mut store = DeviceStore::default();
        store.devices.push(scanner("scanner-1", "COM10"));
        let found = store.match_port("COM99", Some(0x0C2E), Some(1)).unwrap();
        assert_eq!(found.id, "scanner-1");
    }

    #[test]
    fn match_falls_back_to_port_name() {
        let mut store = DeviceStore::default();
        store.devices.push(LearnedDevice {
            id: "scale-1".into(),
            kind: DeviceKind::Scale,
            vid: None,
            pid: None,
            last_port: "COM12".into(),
        });
        assert!(store.match_port("COM12", None, None).is_some());
        assert!(store.match_port("COM1", None, None).is_none());
    }

    #[test]
    fn spec_uses_usb_when_present() {
        let spec = scanner("scanner-1", "COM10").spec();
        assert_eq!(spec.port, PortMatch::Usb { vid: 0x0C2E, pid: 1 });
    }

    #[test]
    fn roundtrip_json() {
        let dir = std::env::temp_dir().join(format!("geco-hub-test-{}", std::process::id()));
        let path = dir.join("devices.json");
        let mut store = DeviceStore::default();
        store.devices.push(scanner("scanner-1", "COM10"));
        store.save_to(&path);
        let loaded = DeviceStore::load_from(&path);
        assert_eq!(loaded.devices[0].id, "scanner-1");
        let _ = std::fs::remove_dir_all(dir);
    }
}
