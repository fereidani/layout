//! Regression tests locking in the observable effects of the SoA optimizations
//! (capacity bounds, value correctness) rather than timings.

use layout::SOA;

#[derive(Clone, SOA)]
#[layout(Clone)]
struct Particle {
    x: f64,
    y: f64,
    mass: f64,
}

fn particle(i: usize) -> Particle {
    Particle {
        x: i as f64,
        y: -(i as f64),
        mass: (i % 7) as f64,
    }
}

// A heavily-filtered `collect` must size capacity from the result, not the
// source length, so it does not allocate source-sized columns.
#[test]
fn from_iter_capacity_tracks_result_not_source() {
    let src: ParticleVec = (0..10_000).map(particle).collect();
    let kept: ParticleVec = src
        .iter()
        .filter(|p| *p.mass == 0.0)
        .map(|p| p.to_owned())
        .collect();
    assert!(kept.len() <= 1500);
    assert!(
        kept.capacity() < 4096,
        "over-allocated: len={} capacity={}",
        kept.len(),
        kept.capacity()
    );
    // Values are still correct.
    for p in kept.iter() {
        assert_eq!(*p.mass, 0.0);
    }
}

// The same lower-bound sizing for a compact column's `collect`.
#[test]
fn compact_from_iter_capacity_tracks_result() {
    use layout::{Compact, CompactVec};
    let full: CompactVec<bool> = (0..100_000)
        .map(|i| Compact::new(i % 10_000 == 0))
        .collect();
    let kept: CompactVec<bool> = full.iter().filter(|c| c.get()).collect();
    assert!(kept.len() <= 20);
    assert!(
        kept.capacity() < 10_000,
        "over-allocated: len={} capacity={}",
        kept.len(),
        kept.capacity()
    );
    assert!(kept.iter().all(|c| c.get()));
}
