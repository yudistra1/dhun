# Design decisions

Running log of non-obvious decisions, per CLAUDE.md. Newest first.

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
