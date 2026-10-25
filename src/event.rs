use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Scan { device: String, code: String },
    WeightStable { device: String, grams: i32 },
    WeightCleared { device: String },
    DeviceConnected { device: String, port: String },
    DeviceDisconnected { device: String, reason: String }
}

#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub v: u8,
    pub id: String,
    pub at: String,
    pub device: String,
    #[serde(flatten)]
    pub body: Body 
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Body {
    Scan { code: String },
    WeightStable { grams: i32 },
    WeightCleared,
    DeviceConnected { port: String },
    DeviceDisconnected { reason: String }
}

impl Envelope {
    pub fn from_event(event: Event) -> Self {
        let (device, body) = match event {
            Event::Scan { device, code } => (device, Body::Scan { code }),
            Event::WeightStable { device, grams } => (device, Body::WeightStable { grams }),
            Event::WeightCleared { device } => (device, Body::WeightCleared),
            Event::DeviceConnected { device, port } => (device, Body::DeviceConnected { port }),
            Event::DeviceDisconnected { device, reason } => (device, Body::DeviceDisconnected { reason })
        };
        Self {
            v: 1,
            id: uuid::Uuid::new_v4().to_string(),
            at: chrono::Utc::now().to_rfc3339(),
            device,
            body
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Body, Envelope, Event};

    #[test]
    fn scan_envelope_flattens_kind() {
        let env = Envelope::from_event(Event::Scan {
            device: "scanner-1".into(),
            code: "779".into(),
        });
        assert_eq!(env.v, 1);
        assert_eq!(env.device, "scanner-1");
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["kind"], "scan");
        assert_eq!(json["code"], "779");
    }

    #[test]
    fn weight_cleared_has_no_extra_fields() {
        let env = Envelope::from_event(Event::WeightCleared {
            device: "scale-1".into(),
        });
        assert!(matches!(env.body, Body::WeightCleared));
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["kind"], "weight_cleared");
        assert!(json.get("grams").is_none());
    }
}
