# et54-rs

Rust driver and native GUI for East Tester ET54 electronic loads, based on
[Philipp Pagel's ET54.py](https://github.com/philpagel/ET54.py).
Supports Linux/NixOS and Windows; the primary target is the **ET5406A+**.

**Disconnecting or exiting does not switch the load off. Use INPUT OFF.**

## Features

- **GUI:** CC/CV/CP/CR control, single-stage CC battery discharge with voltage
  cutoff, live measurements and voltage/current plots, ranges and protection
  settings. English/German and light/dark mode, with saved preferences.
- **Library:** typed serial/SCPI API, multi-channel control, battery, transient,
  LIST, SCAN and qualification functions. Available modes depend on the model;
  not all device functions are implemented.
- **Models:** ET5406A+, ET5407A+, ET5410/ET5410A+, ET5411/ET5411A+ and
  ET5420/ET5420A+. Software limits are defined in
  [`models.rs`](et54/src/models.rs); always follow your device's ratings.

## Run

Requires **Rust 1.95+**. On Linux, install a C/C++ linker, `pkg-config`, OpenGL,
`libxkbcommon` and the X11/Wayland libraries for your desktop session.
On NixOS, run `nix develop` first to use the included development environment.

```sh
cargo run -p et54-gui --locked
```

On Linux, your user needs serial-port access, usually through the `dialout` or
`uucp` group. Do not run the GUI as root. Prefer `/dev/serial/by-id/…` over
`/dev/ttyUSB0` when available; on Windows, use a port such as `COM3`.

## Basic use

1. Set the device to **9600 baud**, select its port and click **Connect**.
   Connecting reads the device settings without activating the load.
2. Select a channel if available, choose a mode and edit the setpoint, ranges,
   protection limits and V-on/V-off thresholds.
3. Click **Apply (switch OFF first)** to switch the selected input off, write
   the settings and read them back. Activation is a separate action.
4. Use **INPUT OFF** to stop the load and verify that OFF is confirmed.

Battery voltage cutoff runs on the device. The independent V-off threshold
may stop discharge earlier. Ah/Wh readings are device counters; their reset
behavior and attribution to a single discharge run are unverified. Software
checks do not replace safe wiring, suitable protection or battery supervision.

## Library

The [`et54`](et54/) crate works without the GUI or graphics libraries.
Add it as a path dependency, adjusting the path for your project:

```toml
[dependencies]
et54 = { path = "et54" }
```

Build the API documentation for examples and available operations:

```sh
cargo doc -p et54 --no-deps --open
```

Manufacturer SCPI references are in [`documents/`](documents/).

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Tests use mocks, headless UI rendering and Linux pseudoterminals. They do not
connect to physical devices or verify electrical behavior.

## License

[MIT](LICENSE). Derived from Philipp Pagel's ET54.py (MIT, copyright 2024
Philipp Pagel). Manufacturer manuals are external references, not covered by
this project's MIT license.
