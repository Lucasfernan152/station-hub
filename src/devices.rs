use std::io::{self, Write};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serialport::{SerialPort, SerialPortType};

use crate::core::{parse_grams, LineFramer, Stabilizer};
use crate::event::Event;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortMatch {
    Usb { vid: u16, pid: u16 },
    Name(String)
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

pub fn spawn_printer(spec: DeviceSpec, tx: Sender<Event>, jobs: Receiver<Vec<u8>>) {
    thread::spawn(move || run_printer(spec, tx, jobs));
}

fn run_printer(spec: DeviceSpec, tx: Sender<Event>, jobs: Receiver<Vec<u8>>) {
    loop {
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
            port: port_name,
        });

        let reason = write_jobs(&mut *port, &jobs);
        let channel_closed = reason == "channel closed";
        let _ = tx.send(Event::DeviceDisconnected {
            device: spec.id.clone(),
            reason,
        });
        if channel_closed {
            return;
        }
    }
}

fn write_jobs(port: &mut dyn SerialPort, jobs: &Receiver<Vec<u8>>) -> String {
    loop {
        match jobs.recv_timeout(Duration::from_millis(400)) {
            Ok(bytes) => {
                if port.write_all(&bytes).is_err() {
                    return "write failed".into();
                }
                if port.flush().is_err() {
                    return "flush failed".into();
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return "channel closed".into(),
        }
    }
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
    let listed: Vec<(String, Option<(u16, u16)>)> = ports
        .into_iter()
        .map(|p| {
            let usb = match p.port_type {
                SerialPortType::UsbPort(info) => Some((info.vid, info.pid)),
                _ => None,
            };
            (p.port_name, usb)
        })
        .collect();
    pick_port(spec, &listed)
}

pub(crate) fn pick_port(spec: &PortMatch, ports: &[(String, Option<(u16, u16)>)]) -> Option<String> {
    match spec {
        PortMatch::Usb { vid, pid } => ports.iter().find_map(|(name, usb)| match usb {
            Some((v, p)) if v == vid && p == pid => Some(name.clone()),
            _ => None,
        }),
        PortMatch::Name(name) => ports
            .iter()
            .find(|(n, _)| n == name)
            .map(|(n, _)| n.clone())
            .or_else(|| Some(name.clone())),
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

#[cfg(test)]
mod tests {
    use super::{pick_port, PortMatch};

    #[test]
    fn usb_match_uses_vid_pid() {
        let ports = vec![
            ("COM3".into(), Some((0x0403, 0x6001))),
            ("COM10".into(), Some((0x0C2E, 0x0200))),
        ];
        assert_eq!(
            pick_port(&PortMatch::Usb { vid: 0x0C2E, pid: 0x0200 }, &ports),
            Some("COM10".into())
        );
    }

    #[test]
    fn name_falls_back_when_missing() {
        let ports = vec![("COM3".into(), None)];
        assert_eq!(
            pick_port(&PortMatch::Name("COM12".into()), &ports),
            Some("COM12".into())
        );
    }

    #[test]
    fn name_uses_listed_port() {
        let ports = vec![("COM12".into(), None)];
        assert_eq!(
            pick_port(&PortMatch::Name("COM12".into()), &ports),
            Some("COM12".into())
        );
    }
}

