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

// --- compact bulk ops (word-aligned copies) vs a byte Vec clone ---

fn compact_to_vec(b: &mut Bencher) {
    let src = build_compact_bools(N, |i| i % 2 == 0);
    b.iter(|| black_box(src.as_slice().to_vec()));
}

fn compact_extend_from_slice(b: &mut Bencher) {
    let src = build_compact_bools(N, |i| i % 2 == 0);
    b.iter(|| {
        let mut dst: CompactVec<bool> = CompactVec::with_capacity(N);
        dst.extend_from_slice(src.as_slice());
        black_box(dst)
    });
}

fn compact_split_off(b: &mut Bencher) {
    let src = build_compact_bools(N, |i| i % 2 == 0);
    b.iter(|| {
        let mut a = src.clone();
        let tail = a.split_off(N / 2);
        black_box((a, tail))
    });
}

fn plain_bytes_clone(b: &mut Bencher) {
    let src: Vec<u8> = (0..N).map(|i| (i % 2) as u8).collect();
    b.iter(|| black_box(src.clone()));
}

// --- CompactIter value iteration vs the specialized count ---

fn compact_iter_count(b: &mut Bencher) {
    let src = build_compact_bools(N, |i| i % 3 == 0);
    b.iter(|| black_box(src.iter().filter(|c| c.get()).count()));
}

fn compact_count_specialized(b: &mut Bencher) {
    let src = build_compact_bools(N, |i| i % 3 == 0);
    b.iter(|| black_box(src.count(true)));
}

// --- word-batched equality of two equal columns ---

fn compact_eq(b: &mut Bencher) {
    let x = build_compact_bools(N, |i| i % 2 == 0);
    let y = x.clone();
    b.iter(|| black_box(x == y));
}

// --- permutation apply (in-place reverse, no clone per iteration) ---

fn apply_index_reverse(b: &mut Bencher) {
    use layout::SoAVec;
    let mut v = build_particles(N);
    let perm: Vec<usize> = (0..N).rev().collect();
    b.iter(|| {
        v.apply_index(&perm);
        black_box(&v);
    });
}

// --- decorated sort_by_key (keys evaluated once) ---

fn sort_by_key_mass(b: &mut Bencher) {
    let base = build_particles(N);
    b.iter(|| {
        let mut v = base.clone();
        v.as_mut_slice().sort_by_key(|r| *r.mass as u64);
        black_box(v)
    });
}

benchmark_group!(
    perf,
    from_iter_exact,
    from_iter_filtered,
    compact_from_iter_filtered,
    compact_push_build,
    plain_bool_push_build,
    compact_to_vec,
    compact_extend_from_slice,
    compact_split_off,
    plain_bytes_clone,
    compact_iter_count,
    compact_count_specialized,
    compact_eq,
    apply_index_reverse,
    sort_by_key_mass
);
benchmark_main!(perf);
