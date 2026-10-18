# Dhun

**An original, open desktop DAW written in Rust** — make a beat, write a melody, and play it back in real time, with a neon-Tokyo UI.

[![CI](https://github.com/yudistra1/dhun/actions/workflows/ci.yml/badge.svg)](https://github.com/yudistra1/dhun/actions/workflows/ci.yml)
&nbsp;License: MIT OR Apache-2.0

> Clean-room project: every built-in sound is synthesized by Dhun's own DSP — no third-party
> samples, and no code, presets, or branding from any other DAW. See [`SPEC.md`](SPEC.md) for the
> full product spec and [`docs/decisions.md`](docs/decisions.md) for the design log.

---

## What you get today

- 🥁 **Step sequencer** — kick / snare / clap / hats / tom, all synthesized, with one-click groove
  **presets** (Four-on-the-Floor, Boom Bap, Trap).
- 🎹 **Piano roll** — a built-in 2-oscillator subtractive synth (detune, low-pass filter, ADSR,
  8-voice polyphony).
- 🎚️ **Mixer** — per-channel volume / pan / mute / solo plus master, with built-in effects
  (gain, 3-band EQ, compressor, reverb).
- ▶️ **Real-time playback** — low-latency audio via [cpal]; **edits are heard live** as the loop
  plays. The audio callback is allocation-free (verified by a test).
- 🎙️ **Audio recording** — capture a take from any input device to a WAV, place it on an audio
  track, and hear it mixed in (trim + latency offset supported).
- 💾 **Deterministic offline render** — bounce a project to a WAV from the GUI or the CLI; the
  render is bit-identical across runs.
- 🖥️ **Headless CLI** — render / inspect / validate / play / record projects without the GUI.

Milestones **M1–M6** are complete; audio recording (**M7**) and save/load/export polish (**M8**)
are in progress. See [Status](#status).

## Quick start

You need the **Rust toolchain** ([install via rustup](https://rustup.rs)). Then:

```bash
git clone https://github.com/yudistra1/dhun.git
cd dhun
cargo run -p dhun-app --release
```

That launches the desktop app. Turn **Loop** on, hit **▶ Play**, click steps into the grid and
notes into the piano roll — the loop updates as you edit.

### Platform audio dependencies

- **macOS** — nothing extra (uses CoreAudio).
- **Linux** — install ALSA + windowing headers first:
  ```bash
  sudo apt-get install -y libasound2-dev libxcb-render0-dev libxcb-shape0-dev \
    libxcb-xfixes0-dev libxkbcommon-dev libgl1-mesa-dev
  ```
- **Windows** — nothing extra (uses WASAPI).

## Command-line usage

The `dhun` binary (crate `dhun-cli`) drives the engine without a GUI:

```bash
# Play a project out the default audio device (Ctrl-C to stop)
cargo run -p dhun-cli -- play tests/fixtures/basic_beat --loop

# Render a project to a 16-bit WAV (offline, deterministic)
cargo run -p dhun-cli -- render tests/fixtures/basic_beat -o out.wav

# Print a summary, or validate a project file
cargo run -p dhun-cli -- info     tests/fixtures/basic_beat
cargo run -p dhun-cli -- validate tests/fixtures/basic_beat

# Record a take from the default input device (Enter to stop, or --seconds N)
cargo run -p dhun-cli -- record -o my-project/audio/take1.wav --seconds 8
```

A project is a folder containing `project.json` (see [`tests/fixtures/basic_beat`](tests/fixtures/basic_beat)),
plus an `audio/` subfolder for recorded takes. Reference a take from an audio track:

```json
"audio_tracks": [
  { "id": 10, "name": "Guitar",
    "mixer": { "gain_db": 0.0, "pan": 0.0, "mute": false, "solo": false },
    "clips": [ { "source": "audio/take1.wav", "start_tick": 0, "offset_frames": 0 } ] }
]
```

`offset_frames` trims the start of the take — set it to your interface's input latency in frames
to line the recording up with the beat.

## Standalone binaries

Build self-contained executables with the helper scripts:

```bash
scripts/build-mac.sh       # universal arm64 + x86_64 → target/apple-universal/release/{dhun,dhun-app}
scripts/build-windows.sh   # x86_64 .exe → target/x86_64-pc-windows-gnu/release/{dhun.exe,dhun-app.exe}
```

Prereqs are documented at the top of each script (the Windows cross-build needs `mingw-w64`).
On macOS, a downloaded (unsigned) binary is quarantined by Gatekeeper — clear it with
`xattr -d com.apple.quarantine ./dhun` or right-click → Open once.

## Architecture

Layered Rust workspace (lower layers never depend on higher ones):

| crate | role |
|---|---|
| `dhun-core` | shared time/tempo types, ids, errors |
| `dhun-dsp` | oscillators, envelopes, filters, drum synth, effects |
| `dhun-project` | project model, serialization, migrations |
| `dhun-engine` | scheduling, voices, offline render, real-time engine |
| `dhun-io` | real-time audio device I/O (cpal) |
| `dhun-cli` | headless commands |
| `dhun-ui` | egui views (depends on the project; talks to the engine only via messages) |
| `dhun-app` | desktop binary wiring UI + engine + I/O |

The UI never calls the engine directly — it emits commands over lock-free queues, keeping the
audio thread free of allocations and locks.

## Development

```bash
cargo test --workspace     # all tests, including the golden render and the no-alloc audit
cargo fmt --all            # format
cargo clippy --workspace --all-targets -- -D warnings
```

## Status

| Milestone | | 
|---|---|
| M1 Workspace + core types + CI | ✅ |
| M2 DSP: synth + drum kit + effects | ✅ |
| M3 Engine + offline render + CLI | ✅ |
| M4 Real-time playback (cpal) | ✅ |
| M5 UI: transport, sequencer, mixer | ✅ |
| M6 Piano roll | ✅ |
| M7 Audio recording | 🚧 record + clip playback via CLI; in-app audio-track UI next |
| M8 Save/load/export polish → v0.1 | ⬜ |

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
The bundled Orbitron display font is licensed separately under the SIL Open Font License 1.1
(see [`crates/dhun-ui/assets/fonts/OFL.txt`](crates/dhun-ui/assets/fonts/OFL.txt)).

[cpal]: https://github.com/RustAudio/cpal
