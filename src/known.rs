use crate::config::DeviceKind;

/// USB IDs that uniquely identify a scanner or scale. Generic serial
/// chips (FTDI, CH340, Prolific, VSPE) are left to traffic probing.
pub fn role_for_usb(vid: u16, pid: u16) -> Option<DeviceKind> {
    match (vid, pid) {
        (0x0C2E, _) => Some(DeviceKind::Scanner), // Honeywell
        (0x05E0, _) => Some(DeviceKind::Scanner), // Zebra / Symbol
        (0x05F9, _) => Some(DeviceKind::Scanner), // Datalogic
        (0x1EAB, _) => Some(DeviceKind::Scanner), // Newland
        (0x0536, _) => Some(DeviceKind::Scanner), // Hand Held Products
        (0x0EB8, _) => Some(DeviceKind::Scale),   // Mettler Toledo
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::role_for_usb;
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
    fn ftdi_is_unknown() {
        assert_eq!(role_for_usb(0x0403, 0x6001), None);
    }
}
