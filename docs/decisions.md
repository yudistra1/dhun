# Design decisions

Running log of non-obvious decisions, per CLAUDE.md. Newest first.

## 2026-10-18 — Mixer meters via lock-free atomics
**Decision:** the audio thread publishes per-channel (post-fader) and master
peak levels into `EngineMeters` (an `AtomicU32`-per-slot store, floats as bit
patterns, `Relaxed`) each `process` block; `Playback` hands a shared `Arc` to the
app, which feeds them into a UI-side `Meters` that applies a per-frame falloff
and latches a clip flag. The mixer draws a green→amber→red bar + clip dot per
channel and on master.
**Reason:** meters are engine→UI data, and the UI may only reach the engine via
lock-free messaging (SPEC §6) — atomics are the simplest wait-free channel and
keep the audio thread allocation-free (the peak scratch is pre-allocated; the
no-alloc guard covers it). Per-channel level is `peak(voice+inserts) ×
sqrt(lg²+rg²)` (the channel's overall gain), i.e. post-fader; master is the final
post-fader output peak. The UI keeps repainting while meters are still falling so
the falloff animates after the transport stops.

## 2026-10-18 — Insert effects wired into the mix path
**Decision:** `MixerStrip` and `Master` gain a `#[serde(default)] effects:
Vec<EffectSpec>`; the engine builds each spec into a `dhun-dsp` processor
(`inserts::Insert` enum) and runs them in `process`. Channel inserts process the
**mono** channel signal before pan/gain; master inserts run **per stereo side**
(two independent chains) before the master fader.
**Reason:** the DSP effects (gain/EQ/compressor/reverb) existed since M2 but were
never in the signal path — a song couldn't actually be EQ'd or reverbed. Mono
channel inserts fit our mono voices; duplicating the master chain per side gives
a stereo master bus without a stereo-effect rewrite.
**Consequences / deferred:**
- Effect chains live in `Program`, which is rebuilt on each live edit, so moving
  an effect knob while playing **resets that effect's state** (brief glitch;
  acceptable for v0.1).
- Reverb allocates its delay lines at build (off-thread); `process` only indexes
  them, so the **no-alloc guard still holds** (it now runs a reverb under the
  guard to prove it).
- **Audio-track** inserts and an **in-app effects rack UI** are deferred — effects
  are set via `project.json` for now; the mixer UI still shows only
  gain/pan/mute/solo. Old projects load unchanged (`#[serde(default)]`).

## 2026-10-18 — Web build: egui→WASM with cpal's WebAudio backend
**Decision:** `dhun-app` builds to `wasm32-unknown-unknown` (via Trunk) and runs
in the browser. egui renders through WebGL (eframe glow backend); audio uses
cpal's WebAudio backend, enabled with a wasm-only `wasm-bindgen` cpal feature.
The `Recorder` (device input + writer thread) is `#[cfg(not(wasm32))]`; `Playback`
(output) compiles for both. A web entry point (`#[cfg(target_arch="wasm32")] main`)
mounts the app on a `<canvas>`; `crates/dhun-app/index.html` is the Trunk entry.
**Reason:** lets anyone try Dhun from a URL with real in-browser sound and no
install. Reusing cpal's WebAudio backend means the same `Playback`/engine code
runs on web — no separate audio path. Browser autoplay policy is satisfied
because the AudioContext is created from the Play click (a user gesture).
Deployed to GitHub Pages.

## 2026-10-18 — M7 audio: recording, in-memory clip assets, unified mix path
**Decisions:**
- **Recording** (`dhun-io::Recorder`): the cpal input callback pushes samples into
  an `rtrb` ring; a background thread drains it to a **32-bit float WAV**. The
  audio thread never allocates/locks/does I/O; the writer thread does. Takes are
  saved under the project's `audio/` folder.
- **Clip data** (`dhun-engine::AudioAssets`): clips are decoded to in-memory
  stereo frames (`Arc<ClipData>`) off the audio thread and shared with the
  engine, so playback is a pure index. Decoupling assets from file paths lets the
  engine stay testable with synthetic clips (no temp files).
- **Unified mix path**: `render_offline` now runs through `RealtimeEngine`
  instead of its own per-channel mixer, so offline == real-time sample-for-sample
  (the golden render still matches) and audio clips mix in one place.
- **Latency compensation** is modelled as the clip's `offset_frames` (trim from
  the source start). Precise alignment is a manual calibration for v0.1; the
  recorder does not yet auto-measure round-trip latency.
- **Format**: `Project` gains `#[serde(default)] audio_tracks`, so pre-audio
  `project.json` files still load (no format-version bump).
- **Deferred**: in-app audio-track UI (arm/record/waveform) and round-trip
  latency auto-calibration. Recording + clip playback are reachable via the CLI
  (`dhun record`, and clips play in `render`).

## 2026-10-18 — M6 piano roll is step-style for v0.1
**Decision:** the piano roll places fixed, one-step-long notes on a pitch × step
grid (click to add/remove), reusing the sequencer's grid timing
(`Project::ticks_per_step`) and the shared glowing `cell` widget. The demo
project ships a "Lead" synth channel with an empty note lane.
**Reason:** satisfies M6's "write and play a melody" with a UI consistent with
the drum grid, and it rides the engine's existing note scheduling + live program
swap (edits are heard on the next loop, same as drums). The richer piano-roll
editing in SPEC §3.3 — variable note length (resize), drag-to-move, per-note
velocity, and snap settings — is deferred to a follow-up; the on-disk `Note`
model already carries `start_tick`/`length_ticks`/`velocity` to support it
without a format change.

## 2026-10-18 — Live edits via off-thread program rebuild + lock-free swap
**Decision:** editing while playing rebuilds a `Program` (events + per-channel and
master mix) off the audio thread; it is sent over an `rtrb` queue and swapped in
by `RealtimeEngine::load_program`, which keeps the playhead and ringing voices
and hands the old program back over a recycle queue to be dropped on the main
thread. The UI emits a coalesced `UiCommand::ProjectEdited` on any step/mixer/
tempo/preset change; the app forwards it as `Playback::update_program`.
**Reason:** step/mixer/tempo edits must be heard as the loop plays, but the audio
callback may not allocate, lock, or free (CLAUDE.md). Rebuilding whole programs
off-thread and swapping a `Box` pointer keeps the callback to a pointer swap plus
a binary-search cursor recompute. Voices persist across swaps, so edits do not
click. A changed channel *count* is rejected by `load_program` (handed back) —
structural changes recreate the session instead. Verified by engine tests
(`live_mute_is_heard_immediately`, `live_step_edit_is_heard_on_next_loop`) and
the no-alloc guard still holds on `process`.

## 2026-10-18 — Bundle Orbitron (OFL-1.1) as the display font
**Decision:** vendor the Orbitron variable font at
`crates/dhun-ui/assets/fonts/Orbitron-Variable.ttf` (with its `OFL.txt`) and use
it for headings/titles via a named egui font family; body stays on egui's
built-in proportional face and numeric readouts use the built-in monospace.
**Reason:** the neon-Tokyo look wants a geometric display face. Orbitron is
licensed under the SIL Open Font License 1.1, which permits redistribution as
long as the license travels with it — satisfying the clean-room / permissive-
assets rule (CLAUDE.md). egui embeds the bytes via `include_bytes!`, so the font
ships inside the binary and needs no system install.

## 2026-10-18 — Sequencer ships built-in groove presets
**Decision:** `dhun-ui` exposes a `Preset` enum (Four-on-the-Floor, Boom Bap,
Trap, Clear) that fills the current pattern's drum lanes by each lane's drum
kind, surfaced as a button row above the grid.
**Reason:** gives a one-click starting point so the app is usable without
hand-placing every step; patterns are authored on a 16-step grid and clamped to
the pattern length.

## 2026-10-18 — UI talks to the engine only via `UiCommand`s; app re-prepares on Play
**Decision:** `dhun-ui` depends on `dhun-project` + `egui` only (SPEC §6) and
edits a `Project` in place, returning `UiCommand`s from `draw`. `dhun-app` owns
the `Playback` and, on `Play`, re-prepares a fresh `Playback` from the current
project; edits therefore take effect on the next Play.
**Reason:** honors the §6 rule that the UI reaches the engine only through
messages, and keeps M5 simple. Live hot-swap of the schedule/mixer while playing
(so edits are heard immediately) needs the engine to accept pattern/param
updates over the `rtrb` queue — deferred to a later milestone.

## 2026-10-18 — eframe with `default-features = false` + `glow`
**Decision:** depend on `eframe` with default features off, enabling only
`default_fonts` and `glow`.
**Reason:** the glow (OpenGL) backend is the most portable across macOS/Linux/
Windows; dropping defaults avoids pulling optional features (e.g. persistence,
accesskit) the app does not use yet. CI installs the egui/eframe windowing dev
packages (xcb, xkbcommon, GL) alongside ALSA so the Linux build links.

## 2026-10-18 — macOS ships a universal (arm64 + x86_64) binary
**Decision:** `scripts/build-mac.sh` builds release binaries for both
`aarch64-apple-darwin` and `x86_64-apple-darwin` and `lipo`-combines them into a
universal binary at `target/apple-universal/release/{dhun,dhun-app}`.
**Reason:** one artifact runs on both Apple Silicon and Intel Macs. The x86_64
slice cross-compiles cleanly on Apple Silicon against the system SDK (CoreAudio
frameworks are universal), so no extra toolchain beyond the rustup target is
needed. Verified at runtime on Apple Silicon (info/validate/render/play).

## 2026-10-18 — Windows builds use the `x86_64-pc-windows-gnu` target (mingw), cross-compiled
**Decision:** Windows `.exe` binaries are produced by cross-compiling to
`x86_64-pc-windows-gnu` with a mingw-w64 linker, driven by
`scripts/build-windows.sh`; the linker is pinned in `.cargo/config.toml`.
**Reason:** the dev machine is macOS (Apple Silicon). The MSVC target
(`...-windows-msvc`) needs the Windows SDK/linker, which isn't available on
macOS; the GNU target links with mingw-w64 (`brew install mingw-w64`). cpal's
WASAPI backend (via the `windows` crate) links cleanly under mingw, so no code
changes were needed. Produces `dhun.exe` (CLI) and `dhun-app.exe` (a stub until
M5). Requires `rustup` for the cross target, installed alongside the Homebrew
toolchain (not replacing it); native builds still use Homebrew `cargo`.

## 2026-10-18 — `dhun-cli` depends on `dhun-io` for a `play` command
**Decision:** added a `dhun play <project>` command, giving `dhun-cli` a
dependency on `dhun-io`.
**Reason:** SPEC §6 lists the CLI's deps as engine+project, but §3.7 says every
engine feature should be reachable from the CLI, and real-time playback is
otherwise unhearable until the GUI (M5). The edge is downward/sideways
(cli → io → engine), introduces no cycle, and does not violate the "no upward
dependency" rule. It lets M4's "plays a project" be verified by ear now.

## 2026-10-18 — `dhun-engine` re-exports `Project`; `dhun-io` depends on core+engine only
**Decision:** `dhun-engine` re-exports `dhun_project::Project`; `dhun-io` names it
as `dhun_engine::Project` rather than depending on `dhun-project` directly.
**Reason:** keeps `dhun-io`'s direct dependencies to exactly core+engine as listed
in SPEC §6, while still letting it call `RealtimeEngine::prepare`, whose input is
a `Project`.

## 2026-10-18 — Audio callback renders one stereo frame at a time
**Decision:** `dhun-io`'s cpal callback calls `RealtimeEngine::process` with a
2-sample stack buffer per device frame, then fans the stereo pair out to the
device's channel layout and sample format (f32/i16/u16).
**Reason:** handles any channel count and format without a per-callback heap
buffer, so the callback stays allocation-free (the no-alloc guard covers
`process` itself). The engine is prepared at the device's actual sample rate so
pitch/time are correct on the hardware.

## 2026-10-18 — CI installs `libasound2-dev`
**Decision:** the CI workflow runs `apt-get install -y libasound2-dev` before
building.
**Reason:** `cpal` links ALSA on Linux; without the dev headers the `dhun-io`
build (and thus the whole workspace) fails to compile on the CI runner.

## 2026-10-18 — Project format mirrors DSP param types instead of depending on `dhun-dsp`
**Decision:** `dhun-project` defines its own serializable `DrumKind`, `WaveformSpec`,
`SynthSpec`, and `AdsrSpec`; `dhun-engine` translates these into the matching
`dhun-dsp` types at render time.
**Reason:** SPEC §6 says `dhun-project` depends on `dhun-core` only. A
project→dsp edge is not in the layering, and CLAUDE.md forbids adding
dependencies not in the plan. The small amount of type duplication keeps the
layers clean and the on-disk format independent of DSP internals.

## 2026-10-18 — Offline render is stereo, 16-bit WAV, with a 2 s tail
**Decision:** `render_offline` produces interleaved stereo `f32`; the CLI writes
16-bit PCM WAV. The renderer appends 2 seconds after the song so note/drum tails
are not clipped. Float→i16 quantization lives in `RenderedAudio::to_i16` so the
CLI and the golden test quantize identically.
**Reason:** stereo is needed for per-channel pan (SPEC §3.5); 16-bit is the v0.1
default (24-bit is a later option in SPEC §3.6). Centralizing quantization avoids
golden/CLI drift.

## 2026-10-18 — Golden render comparison is tolerance-based (±8 LSB)
**Decision:** the `basic_beat` golden test compares re-rendered 16-bit samples to
the committed WAV with a max absolute difference of 8 LSB, and requires exact
sample-count and sample-rate match.
**Reason:** SPEC §8.1 permits "within a documented tolerance where floating-point
allows". The golden is generated on macOS (Apple Silicon) but CI runs on Linux;
platform `sin`/`tan`/`exp` differ by a few ULPs, which can shift a sample by a
couple of LSB. 8 LSB absorbs that while still catching any real DSP or scheduling
regression (which moves output by far more). Same-machine renders remain
bit-identical, satisfying the determinism NFR (SPEC §5).

## 2026-10-18 — DSP processors take `sample_rate: f32`, not `dhun_core::SampleRate`
**Decision:** `dhun-dsp` constructors accept a plain `f32` sample rate rather than
the validated `dhun_core::SampleRate`.
**Reason:** keeps the DSP primitives trivially unit-testable (tests pick any rate)
and decoupled from the project/engine layers. `dhun-core` stays a declared
dependency per SPEC §6 and the engine converts `SampleRate` → `f32` at the boundary.

## 2026-10-18 — Oscillators are naive (not band-limited) in v0.1
**Decision:** saw/square/triangle use direct (naive) waveform evaluation; no
BLEP/BLIT band-limiting yet.
**Reason:** correct phase/frequency is enough for M2's DSP tests, and band-limiting
is scope we can add later without API changes. Consequence: a small, documented DC
offset (~1/samples-per-cycle) on the sampled ramp — the oscillator DC-offset test
uses a 0.02 tolerance to allow for this while still catching gross offset bugs.

## 2026-10-18 — Reverb uses the public-domain Freeverb topology
**Decision:** the master/insert reverb is a mono Schroeder/Freeverb design
(8 damped combs → 4 all-passes) implemented from its public description.
**Reason:** Freeverb (Jezar) is public domain and documented independently of any
commercial DAW, satisfying the clean-room rules (CLAUDE.md). No third-party code
was copied.

## 2026-10-18 — `SampleRate` accepts only 44.1 kHz and 48 kHz
**Decision:** `dhun_core::SampleRate::new` rejects every rate other than 44 100 and
48 000 Hz.
**Reason:** SPEC §2 lists exactly these two for v0.1. Intentional constraint; widen
here (and update this note) if/when more rates are supported.
