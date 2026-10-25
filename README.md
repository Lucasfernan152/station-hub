# station-hub

Local serial hub for a barcode scanner and scale. Listens on `127.0.0.1:17321` and broadcasts JSON over WebSocket (`/ws`).

## Default (no config)

Plug the devices in and start the binary:

```bash
cargo run --release
```

The hub lists serial ports, skips Bluetooth, and **learns roles from traffic**:

- USB VID/PID in a small known table → scanner or scale immediately
- otherwise it waits for the first lines: streaming weight → scale; a single barcode-like line → scanner

The mapping is saved to `%APPDATA%\Geco\devices.json` (or `~/Library/Application Support/Geco/` on macOS) so the next launch reconnects without waiting. Delete that file to re-learn.

If something is already bound to `17321`, the process exits 0 and leaves the existing hub running.

## Lab override

```bash
cargo run --release -- --config station.toml
```

`station.toml` pins COM names or VID/PID. Not needed in the shop.

## Protocol

Same envelopes as before: `scan`, `weight_stable`, `weight_cleared`, `device_connected`, `device_disconnected`, plus a `snapshot` on connect.
