const MAX_JOB: usize = 64 * 1024;

/// Client → hub: `{ "kind": "print", "data": "<base64 ESC/POS>" }`
pub fn decode_print_message(raw: &str) -> Option<Vec<u8>> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let kind = value.get("kind")?.as_str()?;
    if kind != "print" {
        return None;
    }
    let data = value.get("data")?.as_str()?;
    let bytes = decode_base64(data)?;
    if bytes.is_empty() || bytes.len() > MAX_JOB {
        return None;
    }
    Some(bytes)
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let filtered: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if filtered.is_empty() || filtered.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(filtered.len() / 4 * 3);
    for chunk in filtered.chunks(4) {
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        let c = chunk[2];
        let d = chunk[3];
        out.push((a << 2) | (b >> 4));
        if c != b'=' {
            let cv = val(c)?;
            out.push((b << 4) | (cv >> 2));
            if d != b'=' {
                out.push((cv << 6) | val(d)?);
            }
        } else if d != b'=' {
            return None;
        }
    }
    Some(out)
}

fn val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_base64, decode_print_message};

    #[test]
    fn decodes_print_job() {
        let payload = [0x1b, b'@', b'H', b'i'];
        let data = "G0BIaQ==";
        let raw = format!(r#"{{"kind":"print","data":"{data}"}}"#);
        assert_eq!(decode_print_message(&raw).as_deref(), Some(payload.as_slice()));
        assert_eq!(decode_base64(data).as_deref(), Some(payload.as_slice()));
    }

    #[test]
    fn ignores_other_messages() {
        assert!(decode_print_message(r#"{"kind":"ping"}"#).is_none());
        assert!(decode_print_message("not json").is_none());
        assert!(decode_print_message(r#"{"kind":"print","data":"!!!!"}"#).is_none());
    }
}
