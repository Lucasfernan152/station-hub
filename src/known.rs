use crate::config::DeviceKind;

/// USB IDs that uniquely identify a scanner, scale, or ESC/POS printer.
/// Generic serial chips (FTDI, CH340, Prolific, VSPE) are left to traffic
/// probing unless the USB product string looks like a printer.
pub fn role_for_usb(vid: u16, pid: u16) -> Option<DeviceKind> {
    match (vid, pid) {
        (0x0C2E, _) => Some(DeviceKind::Scanner), // Honeywell
        (0x05E0, _) => Some(DeviceKind::Scanner), // Zebra / Symbol
        (0x05F9, _) => Some(DeviceKind::Scanner), // Datalogic
        (0x1EAB, _) => Some(DeviceKind::Scanner), // Newland
        (0x0536, _) => Some(DeviceKind::Scanner), // Hand Held Products
        (0x0EB8, _) => Some(DeviceKind::Scale),   // Mettler Toledo
        (0x04B8, _) => Some(DeviceKind::Printer), // Epson
        (0x0519, _) => Some(DeviceKind::Printer), // Star Micronics
        (0x1504, _) => Some(DeviceKind::Printer), // Bixolon
        (0x1D90, _) => Some(DeviceKind::Printer), // Citizen
        (0x0DD4, _) => Some(DeviceKind::Printer), // Custom / Sewoo
        (0x1CBE, _) => Some(DeviceKind::Printer), // SNBC
        _ => None,
    }
}

pub fn role_for_port(vid: u16, pid: u16, product: Option<&str>) -> Option<DeviceKind> {
    role_for_usb(vid, pid).or_else(|| {
        looks_like_printer(product).then_some(DeviceKind::Printer)
    })
}

fn looks_like_printer(product: Option<&str>) -> bool {
    let p = product.unwrap_or("").to_ascii_lowercase();
    p.contains("print")
        || p.contains("pos-80")
        || p.contains("pos80")
        || p.contains("tm-t")
        || p.contains("thermal")
}

#[cfg(test)]
mod tests {
    use super::{role_for_port, role_for_usb};
    use crate::config::DeviceKind;

    #[test]
    fn honeywell_is_scanner() {
        assert_eq!(role_for_usb(0x0C2E, 0xFFFF), Some(DeviceKind::Scanner));
    }

    #[test]
    fn mettler_is_scale() {
        assert_eq!(role_for_usb(0x0EB8, 0x0001), Some(DeviceKind::Scale));
    }

    #[test]
    fn epson_is_printer() {
        assert_eq!(role_for_usb(0x04B8, 0x0202), Some(DeviceKind::Printer));
    }

    #[test]
    fn ftdi_is_unknown() {
        assert_eq!(role_for_usb(0x0403, 0x6001), None);
    }

    #[test]
    fn product_string_can_mark_generic_chip_as_printer() {
        assert_eq!(
            role_for_port(0x1A86, 0x7523, Some("USB Printer")),
            Some(DeviceKind::Printer)
        );
        assert_eq!(role_for_port(0x1A86, 0x7523, Some("USB-SERIAL CH340")), None);
    }
}
