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

#[derive(SOA)]
struct Tagged {
    id: u64,
    flag: layout::Compact<bool>,
}

// `capacity()` is the minimum across columns; plain and compact columns have
// different granularity, so the fold must consider every column.
#[test]
fn capacity_returns_min_across_columns() {
    use layout::Compact;
    let mut v = TaggedVec::new();
    for i in 0..1000u64 {
        v.push(Tagged {
            id: i,
            flag: Compact::new(i % 2 == 0),
        });
    }
    assert_eq!(v.capacity(), v.id.capacity().min(v.flag.capacity()));
}

// The word-level bulk paths (to_vec / extend_from_slice / split_off / resize)
// must produce the same lanes as element-wise copies, including at lengths
// that leave a partial final word.
#[test]
fn compact_bulk_ops_preserve_lanes() {
    use layout::{Compact, CompactVec};
    for n in [0usize, 1, 63, 64, 65, 200, 1000] {
        let src: CompactVec<bool> =
            (0..n).map(|i| Compact::new(i % 3 == 0)).collect();

        let copied = src.as_slice().to_vec();
        assert!(copied
            .iter()
            .map(|c| c.get())
            .eq((0..n).map(|i| i % 3 == 0)));

        let mut dst: CompactVec<bool> =
            (0..7).map(|i| Compact::new(i % 2 == 0)).collect();
        dst.extend_from_slice(src.as_slice());
        assert_eq!(dst.len(), 7 + n);
        for i in 0..n {
            assert_eq!(dst.get(7 + i).unwrap().get(), i % 3 == 0);
        }

        if n > 0 {
            let mut a = src.clone();
            let at = n / 2;
            let tail = a.split_off(at);
            assert_eq!(a.len(), at);
            assert_eq!(tail.len(), n - at);
            for i in 0..at {
                assert_eq!(a.get(i).unwrap().get(), i % 3 == 0);
            }
            for i in 0..(n - at) {
                assert_eq!(tail.get(i).unwrap().get(), (at + i) % 3 == 0);
            }
        }
    }

    // resize grows with the fill value and truncates back.
    let mut v: CompactVec<bool> = CompactVec::new();
    v.resize(100, Compact::new(true));
    assert_eq!(v.len(), 100);
    assert!(v.iter().all(|c| c.get()));
    v.resize(10, Compact::new(false));
    assert_eq!(v.len(), 10);
    assert!(v.iter().all(|c| c.get()));
}

// The word-cached CompactIter must match a plain oracle forwards, backwards,
// and when both ends are consumed interleaved (which thrashes the cache).
#[test]
fn compact_iter_matches_oracle_all_directions() {
    use layout::{Compact, CompactVec};
    for n in [0usize, 1, 63, 64, 65, 130, 200] {
        let want: Vec<bool> =
            (0..n).map(|i| i % 5 == 0 || i % 7 == 0).collect();
        let v: CompactVec<bool> =
            want.iter().map(|&b| Compact::new(b)).collect();

        assert!(v.iter().map(|c| c.get()).eq(want.iter().copied()));
        assert!(v
            .iter()
            .rev()
            .map(|c| c.get())
            .eq(want.iter().rev().copied()));

        let mut it = v.iter();
        let mut front = Vec::new();
        let mut back = Vec::new();
        while let Some(c) = it.next() {
            front.push(c.get());
            if let Some(c) = it.next_back() {
                back.push(c.get());
            }
        }
        front.extend(back.into_iter().rev());
        assert_eq!(front, want, "interleaved n={n}");
    }
}

// Word-batched equality must ignore stale bits past the length: two columns
// with equal lanes but different tail junk compare equal; a single differing
// lane compares unequal.
#[test]
fn compact_eq_ignores_stale_tail_bits() {
    use layout::{Compact, CompactVec};
    for n in [0usize, 1, 64, 65, 200] {
        let a: CompactVec<bool> =
            (0..n).map(|i| Compact::new(i % 3 == 0)).collect();
        let mut b: CompactVec<bool> =
            (0..n + 40).map(|i| Compact::new(i % 3 == 0)).collect();
        b.truncate(n);
        assert_eq!(a, b);
        assert_eq!(a.as_slice(), b.as_slice());
        if n > 0 {
            let mut c = a.clone();
            let mid = n / 2;
            let old = c.get(mid).unwrap().get();
            c.set(mid, Compact::new(!old));
            assert_ne!(a, c);
        }
    }
}
