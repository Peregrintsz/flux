<div align="center">
  <img src="assets/flux-mark.svg" width="112" alt="Flux logo">
  <h1>Flux</h1>
  <p><strong>A compact, fast desktop autoclicker written in Rust.</strong></p>
  <p>
    <img src="https://img.shields.io/badge/version-0.1.0-white?style=flat-square&labelColor=111111" alt="Version 0.1.0">
    <img src="https://img.shields.io/badge/built_with-Rust-white?style=flat-square&labelColor=111111" alt="Built with Rust">
    <img src="https://img.shields.io/badge/license-MIT-white?style=flat-square&labelColor=111111" alt="MIT license">
  </p>
</div>

<hr>

## Features

| Control | Options |
|:--|:--|
| Mouse button | Left, right, or middle |
| Click speed | CPS or milliseconds, with no preset upper limit |
| Toggle hotkey | Record any key or key combination in the app |
| Run state | Start/stop button or global hotkey |

The click loop runs on a separate thread, so the interface stays responsive.

## Run

Install [Rust](https://www.rust-lang.org/tools/install), then run this in the project directory:

```powershell
cargo run --release
```

On Windows, the executable is created at `target/release/flux.exe`.

## Notes

- Enter a positive integer for CPS or milliseconds. The app keeps a minimum interval of 1 ms.
- Some games and apps may block synthetic input.
- Click the shortcut field, then press a key or key combination to assign it. The default shortcut is F6.

## License

MIT
