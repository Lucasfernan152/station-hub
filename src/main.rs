mod config;
mod core;
mod devices;
mod discover;
mod event;
mod known;
mod store;
mod ws;

use std::collections::HashMap;
use std::env;
use std::io::ErrorKind;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use crate::config::{DeviceKind, FileConfig, DEFAULT_BIND};
use crate::core::ScanDebouncer;
use crate::devices::{spawn_scale, spawn_scanner};
use crate::event::{Envelope, Event};
use crate::ws::{apply_snapshot, App};

struct Args {
    config: Option<String>,
    bind: Option<String>,
}

fn parse_args() -> Args {
    parse_args_from(env::args().skip(1))
}

fn parse_args_from<I>(args: I) -> Args
where
    I: IntoIterator<Item = String>,
{
    let mut config = None;
    let mut bind = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = args.next(),
            "--bind" => bind = args.next(),
            _ => {}
        }
    }
    Args { config, bind }
}

fn main() {
    let args = parse_args();
    let (tx, rx) = mpsc::channel();

    let bind = if let Some(path) = args.config.as_deref() {
        let cfg = FileConfig::load_path(path).expect("config");
        for device in cfg.device {
            let (kind, spec) = device.spec().expect("device");
            match kind {
                DeviceKind::Scanner => spawn_scanner(spec, tx.clone()),
                DeviceKind::Scale => spawn_scale(spec, tx.clone()),
            }
        }
        args.bind.unwrap_or(cfg.bind)
    } else {
        discover::spawn(tx.clone());
        args.bind.unwrap_or_else(|| DEFAULT_BIND.into())
    };

    let rt = tokio::runtime::Runtime::new().expect("tokio");
    rt.block_on(async move {
        let (events, _) = broadcast::channel::<Envelope>(64);
        let snapshot = Arc::new(Mutex::new(HashMap::new()));
        let app = App {
            events: events.clone(),
            snapshot: snapshot.clone(),
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

        match ws::serve(bind.clone(), app).await {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::AddrInUse => {
                eprintln!("hub already running on {bind}");
            }
            Err(err) => panic!("ws server: {err}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::parse_args_from;

    #[test]
    fn parses_config_and_bind() {
        let args = parse_args_from(
            ["--config", "station.toml", "--bind", "127.0.0.1:9"]
                .into_iter()
                .map(str::to_string),
        );
        assert_eq!(args.config.as_deref(), Some("station.toml"));
        assert_eq!(args.bind.as_deref(), Some("127.0.0.1:9"));
    }

    #[test]
    fn defaults_when_empty() {
        let args = parse_args_from(Vec::<String>::new());
        assert!(args.config.is_none());
        assert!(args.bind.is_none());
    }
}
