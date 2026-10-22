# Flux

A compact, fast desktop autoclicker written in Rust. First release: **0.1.0**.

## Features

- Choose left, right, or middle mouse button.
- Set the speed in CPS (clicks per second) or milliseconds.
- Choose a global start/stop hotkey from F6–F12.
- Start and stop with the UI button or hotkey.
- The clicking loop runs on a separate thread.

## Run

Install [Rust](https://www.rust-lang.org/tools/install), then run this in the project directory:

```powershell
cargo run --release
```

On Windows, the executable is created at `target/release/flux.exe`.

## Notes

- In CPS mode, the interval is calculated as `1000 / CPS` milliseconds. CPS ranges from 1 to 1,000.
- The millisecond interval ranges from 1 to 60,000 ms.
- Some games and apps may block synthetic input.
- Choose a global hotkey between F6 and F12. The default is F6.

## License

MIT
