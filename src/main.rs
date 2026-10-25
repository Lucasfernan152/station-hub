mod core;
mod devices;
mod event;
mod config;
mod ws;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use crate::config::{DeviceKind, FileConfig};
use crate::core::ScanDebouncer;
use crate::devices::{spawn_scale, spawn_scanner};
use crate::event::{Envelope, Event};
use crate::ws::{App, apply_snapshot};

fn main() {
    let cfg = FileConfig::load().expect("config");
    let (tx, rx) = mpsc::channel();

    for device in cfg.device {
        let (kind, spec) = device.spec().expect("device");
        match kind {
            DeviceKind::Scanner => spawn_scanner(spec, tx.clone()),
            DeviceKind::Scale => spawn_scale(spec, tx.clone())
        }
    }

    let rt = tokio::runtime::Runtime::new().expect("tokio");
    rt.block_on(async move {
        let (events, _) = broadcast::channel::<Envelope>(64);
        let snapshot = Arc::new(Mutex::new(HashMap::new()));
        let app = App {
            events: events.clone(),
            snapshot: snapshot.clone()
        };

        std::thread::spawn(move || {
            let mut debouncer = ScanDebouncer::new(Duration::from_millis(80));
            for event in rx {
                match &event {
                    Event::Scan { code, .. } if !debouncer.accept(code, Instant::now()) => {
                        continue;
                    }
                    _ => {}
                }
                let env = Envelope::from_event(event);
                println!("{}", serde_json::to_string(&env).unwrap());
                apply_snapshot(&snapshot, &env);
                let _ = events.send(env);
            }
        });

        ws::serve(cfg.bind, app).await;
    });
}