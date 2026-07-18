//! Benchmarks for `CompactVec`/generated-SoA operations, each measured against
//! a natural baseline (a `std` equivalent or the strategy it replaces).
#![allow(clippy::needless_return, dead_code)]

use bencher::{benchmark_group, benchmark_main, black_box, Bencher};
use layout::{Compact, CompactVec, SOA};

#[derive(Clone, SOA)]
#[layout(Clone)]
pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub mass: f64,
}

fn particle(i: usize) -> Particle {
    let f = i as f64;
    Particle {
        x: f * 0.5,
        y: f * 0.25,
        z: f * 0.125,
        mass: (i % 977) as f64,
    }
}

fn build_particles(n: usize) -> ParticleVec {
    let mut v = ParticleVec::with_capacity(n);
    for i in 0..n {
        v.push(particle(i));
    }
    v
}

fn build_compact_bools(
    n: usize,
    f: impl Fn(usize) -> bool,
) -> CompactVec<bool> {
    let mut v = CompactVec::with_capacity(n);
    for i in 0..n {
        v.push(Compact::new(f(i)));
    }
    v
}

const N: usize = 100_000;

// --- FromIterator: exact-size vs heavily-filtered `collect` ---

fn from_iter_exact(b: &mut Bencher) {
    let src = build_particles(N);
    let owned: Vec<Particle> = src.iter().map(|p| p.to_owned()).collect();
    b.iter(|| {
        let v: ParticleVec = black_box(&owned).iter().cloned().collect();
        black_box(v)
    });
}

fn from_iter_filtered(b: &mut Bencher) {
    let src = build_particles(N);
    b.iter(|| {
        let v: ParticleVec = black_box(&src)
            .iter()
            .filter(|p| *p.mass == 0.0)
            .map(|p| p.to_owned())
            .collect();
        black_box(v)
    });
}

fn compact_from_iter_filtered(b: &mut Bencher) {
    let src = build_compact_bools(N, |i| i % 977 == 0);
    b.iter(|| {
        let v: CompactVec<bool> =
            black_box(&src).iter().filter(|c| c.get()).collect();
        black_box(v)
    });
}

// --- push throughput: compact column vs plain byte Vec ---

fn compact_push_build(b: &mut Bencher) {
    b.iter(|| black_box(build_compact_bools(N, |i| i % 3 == 0)));
}

fn plain_bool_push_build(b: &mut Bencher) {
    b.iter(|| {
        let mut v = Vec::with_capacity(N);
        for i in 0..N {
            v.push(i % 3 == 0);
        }
        black_box(v)
    });
}

benchmark_group!(
    perf,
    from_iter_exact,
    from_iter_filtered,
    compact_from_iter_filtered,
    compact_push_build,
    plain_bool_push_build
);
benchmark_main!(perf);
