use std::io::{self};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use serialport::{SerialPort, SerialPortType};

use crate::core::{parse_grams, LineFramer, Stabilizer};
use crate::event::Event;

pub enum PortMatch {
    Usb { vid: u16, pid: u16 },
    Name(String)
}

pub struct DeviceSpec {
    pub id: String,
    pub port: PortMatch
}

pub fn spawn_scanner(spec: DeviceSpec, tx: Sender<Event>) {
    spawn_device(spec, tx, ReadMode::Scanner);
}

pub fn spawn_scale(spec: DeviceSpec, tx: Sender<Event>) {
    spawn_device(spec, tx, ReadMode::Scale);
}

enum ReadMode {
    Scanner,
    Scale
}

fn spawn_device(spec: DeviceSpec, tx: Sender<Event>, mode: ReadMode) {
    thread::spawn(move || loop {
        let Some(port_name) = find_port(&spec.port) else {
            eprintln!("{}: waiting for port", spec.id);
            thread::sleep(Duration::from_secs(2));
            continue;
        };

        let opened = serialport::new(&port_name, 9600)
            .timeout(Duration::from_millis(200))
            .open();

        let Ok(mut port) = opened else {
            eprintln!("{}: open failed on port {port_name}", spec.id);
            thread::sleep(Duration::from_secs(2));
            continue;
        };
        let _ = port.write_data_terminal_ready(true);
        let _ = tx.send(Event::DeviceConnected {
            device: spec.id.clone(),
            port: port_name
        });

        let reason = match mode {
            ReadMode::Scanner => read_scanner(&mut *port, &spec.id, &tx),
            ReadMode::Scale => read_scale(&mut *port, &spec.id, &tx)
        };
        let _ = tx.send(Event::DeviceDisconnected {
            device: spec.id.clone(),
            reason
        });
    });
}

fn find_port(spec: &PortMatch) -> Option<String> {
    let ports = serialport::available_ports().ok()?;
    match spec {
        PortMatch::Usb { vid, pid } => ports.into_iter().find_map(|p| match p.port_type {
            SerialPortType::UsbPort(info) if info.vid == *vid && info.pid == *pid => {
                Some(p.port_name)
            }
            _ => None
        }),
        PortMatch::Name(name) => ports
            .into_iter()
            .map(|p| p.port_name)
            .find(|n| n == name)
            .or_else(|| Some(name.clone()))
    }
}

fn read_scanner(port: &mut dyn SerialPort, device: &str, tx: &Sender<Event>) -> String {
    let mut framer = LineFramer::new(64);
    let mut buf = [0u8; 256];
    loop {
        match port.read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                for code in framer.feed(&buf[..n]) {
                    if tx
                        .send(Event::Scan { device: device.to_string(), code })
                        .is_err()
                    {
                        return "channel closed".into();
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
            Err(e) => return e.to_string()
        }
    }
}

fn read_scale(port: &mut dyn SerialPort, device: &str, tx: &Sender<Event>) -> String {
    let mut framer = LineFramer::new(64);
    let mut stab = Stabilizer::new(2, Duration::from_millis(400), 20);
    let mut buf = [0u8; 256];
    loop {
        match port.read(&mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                for line in framer.feed(&buf[..n]) {
                    let Some(grams) = parse_grams(&line) else {
                        continue;
                    };
                    if let Some(event) = stab.push(grams, Instant::now(), device) {
                        if tx.send(event).is_err() {
                            return "channel closed".into();
                        }
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                if let Some(event) = stab.tick(Instant::now(), device) {
                    if tx.send(event).is_err() {
                        return "channel closed".into();
                    }
                }
                continue;
            },
            Err(e) => return e.to_string()
        }
    }
}

