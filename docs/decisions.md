# Design decisions

Running log of non-obvious decisions, per CLAUDE.md. Newest first.

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
