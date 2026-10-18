//! Golden render test (SPEC §8.1): the `basic_beat` fixture must render to the
//! committed golden WAV.
//!
//! Comparison is tolerance-based, which SPEC §8 explicitly permits "within a
//! documented tolerance where floating-point allows". The tolerance below
//! absorbs at-most-a-few-LSB differences from platform `sin`/`tan`/`exp`
//! implementations (e.g. macOS vs. CI Linux) while still catching any real DSP
//! or scheduling regression, which would shift output by far more.

use std::path::PathBuf;

use dhun_engine::render_offline;
use dhun_project::Project;

/// Maximum allowed absolute difference per 16-bit sample.
const TOLERANCE_LSB: i32 = 8;

fn repo_root() -> PathBuf {
    // crates/dhun-engine -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn basic_beat_matches_golden() {
    let root = repo_root();
    let project =
        Project::load(root.join("tests/fixtures/basic_beat")).expect("load basic_beat fixture");
    let rendered = render_offline(&project, None).expect("render basic_beat");
    let actual = rendered.to_i16();

    let golden_path = root.join("tests/golden/basic_beat.wav");
    let reader = hound::WavReader::open(&golden_path)
        .unwrap_or_else(|e| panic!("open golden {}: {e}", golden_path.display()));

    let spec = reader.spec();
    assert_eq!(spec.channels, 2, "golden should be stereo");
    assert_eq!(
        spec.sample_rate, rendered.sample_rate,
        "sample-rate mismatch"
    );

    let golden: Vec<i16> = reader
        .into_samples::<i16>()
        .collect::<Result<_, _>>()
        .expect("read golden samples");

    assert_eq!(
        actual.len(),
        golden.len(),
        "sample count changed: rendered {}, golden {}",
        actual.len(),
        golden.len()
    );

    let mut max_diff = 0i32;
    let mut worst_at = 0usize;
    for (i, (&a, &g)) in actual.iter().zip(&golden).enumerate() {
        let diff = (i32::from(a) - i32::from(g)).abs();
        if diff > max_diff {
            max_diff = diff;
            worst_at = i;
        }
    }
    assert!(
        max_diff <= TOLERANCE_LSB,
        "render drifted from golden: max diff {max_diff} LSB at sample {worst_at} (tolerance {TOLERANCE_LSB})"
    );
}
