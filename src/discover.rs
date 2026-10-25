use std::collections::HashSet;
use std::io;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serialport::{SerialPortInfo, SerialPortType};

use crate::config::DeviceKind;
use crate::core::{classify_line, LineFramer, ProbeRole};
use crate::devices::{spawn_scale, spawn_scanner};
use crate::event::Event;
use crate::known::role_for_usb;
use crate::store::{DeviceStore, LearnedDevice};

pub fn spawn(tx: Sender<Event>) {
    thread::spawn(move || run(tx));
}

fn run(tx: Sender<Event>) {
    let store = Arc::new(Mutex::new(DeviceStore::load()));
    let claimed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let spawned_ids = Arc::new(Mutex::new(HashSet::<String>::new()));

    {
        let learned = store.lock().expect("store");
        for device in &learned.devices {
            start_device(&claimed, &spawned_ids, device, &tx);
        }
    }

    loop {
        let ports = serialport::available_ports().unwrap_or_default();
        for info in ports {
            if skip_port(&info) {
                continue;
            }
            let name = info.port_name.clone();
            if !claimed.lock().expect("claimed").insert(name.clone()) {
                continue;
            }

            let (vid, pid) = usb_ids(&info);
            if let Some(device) = store
                .lock()
                .expect("store")
                .match_port(&name, vid, pid)
                .cloned()
            {
                start_device(&claimed, &spawned_ids, &device, &tx);
                continue;
            }

            if let (Some(vid), Some(pid)) = (vid, pid) {
                if let Some(kind) = role_for_usb(vid, pid) {
                    remember_and_spawn(
                        &store,
                        &claimed,
                        &spawned_ids,
                        &tx,
                        kind,
                        name,
                        Some(vid),
                        Some(pid),
                    );
                    continue;
                }
            }

            let store = store.clone();
            let claimed = claimed.clone();
            let spawned_ids = spawned_ids.clone();
            let tx = tx.clone();
            thread::spawn(move || probe(name, vid, pid, store, claimed, spawned_ids, tx));
        }
        thread::sleep(Duration::from_secs(2));
    }
}

fn start_device(
    claimed: &Mutex<HashSet<String>>,
    spawned_ids: &Mutex<HashSet<String>>,
    device: &LearnedDevice,
    tx: &Sender<Event>,
) {
    claimed.lock().expect("claimed").insert(device.last_port.clone());
    if !spawned_ids.lock().expect("ids").insert(device.id.clone()) {
        return;
    }
    match device.kind {
        DeviceKind::Scanner => spawn_scanner(device.spec(), tx.clone()),
        DeviceKind::Scale => spawn_scale(device.spec(), tx.clone()),
    }
}

fn remember_and_spawn(
    store: &Mutex<DeviceStore>,
    claimed: &Mutex<HashSet<String>>,
    spawned_ids: &Mutex<HashSet<String>>,
    tx: &Sender<Event>,
    kind: DeviceKind,
    port: String,
    vid: Option<u16>,
    pid: Option<u16>,
) {
    let mut store = store.lock().expect("store");
    let id = store.next_id(kind);
    let device = LearnedDevice {
        id,
        kind,
        vid,
        pid,
        last_port: port.clone(),
    };
    eprintln!("{}: {:?} on {port}", device.id, kind);
    store.upsert(device.clone());
    drop(store);
    start_device(claimed, spawned_ids, &device, tx);
}

fn probe(
    name: String,
    vid: Option<u16>,
    pid: Option<u16>,
    store: Arc<Mutex<DeviceStore>>,
    claimed: Arc<Mutex<HashSet<String>>>,
    spawned_ids: Arc<Mutex<HashSet<String>>>,
    tx: Sender<Event>,
) {
    eprintln!("probe: waiting for data on {name}");
    let opened = serialport::new(&name, 9600)
        .timeout(Duration::from_millis(200))
        .open();
    let Ok(mut port) = opened else {
        claimed.lock().expect("claimed").remove(&name);
        return;
    };
    let _ = port.write_data_terminal_ready(true);

    let mut framer = LineFramer::new(64);
    let mut buf = [0u8; 256];
    let mut first: Option<(String, Instant)> = None;

    let role = loop {
        match port.read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                let now = Instant::now();
                let mut decided = None;
                for line in framer.feed(&buf[..n]) {
                    if let Some((_, at)) = &first {
                        if now.duration_since(*at) < Duration::from_millis(350) {
                            decided = Some(ProbeRole::Scale);
                            break;
                        }
                    }
                    if first.is_none() {
                        first = Some((line, now));
                    } else if let Some(role) = classify_line(&line) {
                        decided = Some(role);
                        break;
                    }
                }
                if let Some(role) = decided {
                    break Some(role);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                if let Some((line, at)) = &first {
                    if Instant::now().duration_since(*at) >= Duration::from_millis(400) {
                        break classify_line(line);
                    }
                }
            }
            Err(_) => {
                claimed.lock().expect("claimed").remove(&name);
                return;
            }
        }
    };

    drop(port);

    let Some(role) = role else {
        claimed.lock().expect("claimed").remove(&name);
        return;
    };
    let kind = match role {
        ProbeRole::Scanner => DeviceKind::Scanner,
        ProbeRole::Scale => DeviceKind::Scale,
    };
    remember_and_spawn(&store, &claimed, &spawned_ids, &tx, kind, name, vid, pid);
}

fn usb_ids(info: &SerialPortInfo) -> (Option<u16>, Option<u16>) {
    match &info.port_type {
        SerialPortType::UsbPort(usb) => (Some(usb.vid), Some(usb.pid)),
        _ => (None, None),
    }
}

fn skip_port(info: &SerialPortInfo) -> bool {
    skip_port_name(
        &info.port_name,
        matches!(info.port_type, SerialPortType::BluetoothPort),
    )
}

pub(crate) fn skip_port_name(name: &str, bluetooth: bool) -> bool {
    if bluetooth {
        return true;
    }
    let lower = name.to_ascii_lowercase();
    lower.contains("bluetooth")
        || lower.contains("debug-console")
        || lower.contains("incoming")
        || lower.contains("dial-in")
}

#[cfg(test)]
mod tests {
    use super::skip_port_name;

    #[test]
    fn skips_bluetooth_and_debug() {
        assert!(skip_port_name("COM5", true));
        assert!(skip_port_name("/dev/cu.Bluetooth-Incoming-Port", false));
        assert!(skip_port_name("/dev/cu.debug-console", false));
        assert!(skip_port_name("Dial-In", false));
    }

    #[test]
    fn keeps_vspe_and_usb_serial() {
        assert!(!skip_port_name("COM10", false));
        assert!(!skip_port_name("/dev/tty.usbserial-A", false));
    }
}
