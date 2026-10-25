use std::time::{Duration, Instant};

use crate::event::Event;

pub struct LineFramer {
    buf: Vec<u8>,
    max_len: usize
}

impl LineFramer {
    pub fn new(max_len: usize) -> Self {
        Self {
            buf: Vec::new(),
            max_len
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for &b in bytes {
            match b {
                b'\r' | b'\n' => {
                    if !self.buf.is_empty() {
                        let line = String::from_utf8_lossy(&self.buf).trim().to_string();
                        if !line.is_empty() {
                            lines.push(line);
                        }
                        self.buf.clear();
                    }
                }
                _ if self.buf.len() < self.max_len => self.buf.push(b),
                _ => self.buf.clear()
            }
        }
        lines
    }
}

pub struct ScanDebouncer {
    window: Duration,
    last: Option<(String, Instant)>
}

impl ScanDebouncer {
    pub fn new(window: Duration) -> Self {
        Self {window, last: None}
    }

    pub fn accept(&mut self, code: &str, now: Instant) -> bool {
        let dup = matches!(&self.last, Some((c, t)) if c == code && now.duration_since(*t) < self.window);
        self.last = Some((code.to_string(), now));
        !dup
    }
}

#[derive(Clone, Copy)]
enum StabState {
    Empty,
    Settling { anchor: i32, last: i32, since: Instant },
    Stable { grams: i32 }
}

pub struct Stabilizer {
    band_grams: i32,
    hold: Duration,
    min_grams: i32,
    state: StabState
}

impl Stabilizer {
    pub fn new(band_grams: i32, hold: Duration, min_grams: i32) -> Self {
        Self {
            band_grams,
            hold,
            min_grams,
            state: StabState::Empty
        }
    }

    pub fn tick(&mut self, now: Instant, device: &str) -> Option<Event> {
        let StabState::Settling { last, .. } = self.state else {
            return None;
        };
        self.push(last, now, device)
    }

    pub fn push(&mut self, grams: i32, now: Instant, device: &str) -> Option<Event> {
        if grams < self.min_grams {
            let was_loaded = !matches!(self.state, StabState::Empty);
            self.state = StabState::Empty;
            return was_loaded.then(|| Event::WeightCleared {
                device: device.to_string()
            });
        }

        match self.state {
            StabState::Empty => {
                self.state = StabState::Settling { anchor: grams, last: grams, since: now };
                None
            }
            StabState::Settling { anchor, since, .. } => {
                if (grams - anchor).abs() > self.band_grams {
                    self.state = StabState::Settling { anchor: grams, last: grams, since: now };
                    None
                } else if now.duration_since(since) >= self.hold {
                    self.state = StabState::Stable { grams };
                    Some(Event::WeightStable {
                        device: device.to_string(),
                        grams
                    })
                } else {
                    None
                }
            }
            StabState::Stable { grams: current } => {
                if (grams - current).abs() > self.band_grams {
                    self.state = StabState::Settling { anchor: grams, last: grams, since: now };
                }
                None
            }
        }
    }
}

pub fn parse_grams(line: &str) -> Option<i32> {
    let s = line.trim().replace(',', ".");
    if s.contains(".") {
        let kg: f64 = s.parse().ok()?;
        Some((kg * 1000.0).round() as i32)
    } else {
        s.parse().ok()
    }
}