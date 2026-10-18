# CLAUDE.md — How to work on Dhun

You are building **Dhun**, an original desktop DAW in Rust. Read `SPEC.md` before any task — it is the source of truth for scope. If a request conflicts with SPEC.md, stop and ask.

## Clean-room rules (non-negotiable)
- Never copy code, UI layouts, icons, presets, samples, or names from FL Studio or any other commercial DAW.
- Do not decompile or inspect proprietary binaries.
- Build from public specs (WAV/RIFF, MIDI, etc.), general DSP knowledge, and permissively licensed crates only.
- All built-in sounds are synthesized by our own DSP. No third-party sample packs.

## Workspace layout
See SPEC.md §6. Respect the layer rules:
- `dhun-core` ← `dhun-dsp`, `dhun-project` ← `dhun-engine` ← `dhun-io`, `dhun-cli` ← `dhun-app`
- `dhun-ui` depends on `dhun-project` and talks to the engine **only** through message queues.
- Never add a dependency that points "upward". If you think you need one, stop and propose a design change instead.

## Real-time audio rules
In anything reachable from the audio callback:
- No heap allocation, no `Mutex`/`RwLock`, no file or network I/O, no `println!`/logging, no `unwrap()` on fallible paths.
- Pre-allocate buffers and voices at setup. Use `rtrb` ring buffers for UI ↔ engine messages.
- If you touch the audio path, run the no-allocation guard test.

## Definition of done (every task)
1. `cargo fmt --all` — clean
2. `cargo clippy --workspace --all-targets -- -D warnings` — clean
3. `cargo test --workspace` — all green, including golden renders
4. New behavior has tests. DSP changes include a unit test with a numeric check.
5. If a change alters audio output on purpose, regenerate goldens with `cargo run -p dhun-cli -- render ...` **and** explain why in the commit message. Never update a golden just to make a test pass.

## Testing approach
- Prefer testing through the engine's offline render and the CLI, not the GUI.
- Fixtures live in `tests/fixtures/<name>/project.json`; goldens in `tests/golden/<name>.wav`.
- Keep tests fast; mark slow benchmarks `#[ignore]`.

## Working style
- Work in small, reviewable steps; one milestone task at a time (SPEC.md §9).
- Before coding a non-trivial task, write a short plan: files to touch, tests to add.
- When running parallel agents, give each one a separate crate or a clearly separate module and its own git worktree/branch.
- Commit messages: `area: what changed` (e.g. `dsp: add state-variable filter`).
- If something in the spec is ambiguous, record the decision in `docs/decisions.md` (date, decision, reason).

## Commands
- Build app: `cargo run -p dhun-app --release`
- Render: `cargo run -p dhun-cli -- render tests/fixtures/basic_beat -o /tmp/out.wav`
- Play: `cargo run -p dhun-cli -- play tests/fixtures/basic_beat`
- Tests: `cargo test --workspace`
- Benchmarks: `cargo test --workspace -- --ignored`
- macOS universal binary (arm64 + x86_64): `scripts/build-mac.sh`
- Windows .exe (cross-compile): `scripts/build-windows.sh` (see docs/decisions.md for prereqs)
