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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeRole {
    Scanner,
    Scale,
}

pub fn classify_line(line: &str) -> Option<ProbeRole> {
    let t = line.trim();
    if t.is_empty() {
        return None;
    }
    if looks_like_weight(t) {
        return Some(ProbeRole::Scale);
    }
    if is_scan_code(t) {
        return Some(ProbeRole::Scanner);
    }
    None
}

fn looks_like_weight(t: &str) -> bool {
    let lower = t.to_ascii_lowercase();
    let stripped = lower
        .trim()
        .trim_end_matches("kg")
        .trim_end_matches('g')
        .trim();
    let Some(grams) = parse_grams(stripped) else {
        return false;
    };
    if grams.abs() > 200_000 {
        return false;
    }
    stripped.contains('.') || t.len() <= 6
}

fn is_scan_code(t: &str) -> bool {
    t.len() >= 4
        && t.len() <= 64
        && t.chars().all(|c| c.is_ascii_graphic())
        && !looks_like_weight(t)
}

#[cfg(test)]
mod tests {
    use super::{classify_line, ProbeRole};

    #[test]
    fn decimal_weight_is_scale() {
        assert_eq!(classify_line("0.452"), Some(ProbeRole::Scale));
        assert_eq!(classify_line("1,230"), Some(ProbeRole::Scale));
        assert_eq!(classify_line("0.45 kg"), Some(ProbeRole::Scale));
    }

    #[test]
    fn ean_is_scanner() {
        assert_eq!(classify_line("7791234567890"), Some(ProbeRole::Scanner));
        assert_eq!(classify_line("ABC-42"), Some(ProbeRole::Scanner));
    }

    #[test]
    fn short_integer_weight() {
        assert_eq!(classify_line("125"), Some(ProbeRole::Scale));
    }

    #[test]
    fn empty_line_is_ignored() {
        assert_eq!(classify_line("  "), None);
    }

    #[test]
    fn framer_splits_crlf() {
        let mut framer = super::LineFramer::new(64);
        assert!(framer.feed(b"12").is_empty());
        assert_eq!(framer.feed(b"3\r\nABC\n"), vec!["123", "ABC"]);
    }

    #[test]
    fn debouncer_drops_duplicate_inside_window() {
        let mut debouncer = super::ScanDebouncer::new(Duration::from_millis(80));
        let t0 = Instant::now();
        assert!(debouncer.accept("X", t0));
        assert!(!debouncer.accept("X", t0 + Duration::from_millis(10)));
        assert!(debouncer.accept("Y", t0 + Duration::from_millis(10)));
        assert!(debouncer.accept("X", t0 + Duration::from_millis(100)));
    }

    #[test]
    fn stabilizer_emits_stable_then_cleared() {
        let mut stab = super::Stabilizer::new(2, Duration::from_millis(400), 20);
        let t0 = Instant::now();
        assert!(stab.push(100, t0, "s").is_none());
        assert!(stab.push(101, t0 + Duration::from_millis(100), "s").is_none());
        let event = stab.push(100, t0 + Duration::from_millis(400), "s").unwrap();
        assert_eq!(
            event,
            Event::WeightStable {
                device: "s".into(),
                grams: 100
            }
        );
        let cleared = stab.push(0, t0 + Duration::from_millis(500), "s").unwrap();
        assert_eq!(
            cleared,
            Event::WeightCleared {
                device: "s".into()
            }
        );
    }
}