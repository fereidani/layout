#![allow(unexpected_cfgs)]
#![cfg(rustc_is_at_least_1_78)]

mod particles;
use core::ops::Bound;

use layout::{SoASlice, SoASliceMut, SoAVec};
use particles::{Particle, ParticleVec};

fn filled() -> ParticleVec {
    let mut v = ParticleVec::new();
    for m in 0..5 {
        v.push(Particle::new(m.to_string(), f64::from(m)));
    }
    v
}

// An `Excluded` *start* bound means "begin after index i", i.e. i + 1 —
// the same semantics RangeBounds has everywhere else in std (e.g.
// `BTreeMap::range`). `(Excluded(1), Unbounded)` over 0,1,2,3,4 must
// therefore yield 2,3,4.

#[test]
fn vec_slice_excluded_start() {
    let v = filled();
    let sub = SoAVec::slice(&v, (Bound::Excluded(1usize), Bound::Unbounded));
    assert_eq!(sub.mass, &[2.0, 3.0, 4.0]);
}

#[test]
fn vec_slice_mut_excluded_start() {
    let mut v = filled();
    let sub =
        SoAVec::slice_mut(&mut v, (Bound::Excluded(1usize), Bound::Unbounded));
    assert_eq!(sub.mass, &[2.0, 3.0, 4.0]);
}

#[test]
fn slice_slice_excluded_start() {
    let v = filled();
    let s = SoAVec::as_slice(&v);
    let sub = SoASlice::slice(&s, (Bound::Excluded(1usize), Bound::Unbounded));
    assert_eq!(sub.mass, &[2.0, 3.0, 4.0]);
}

#[test]
fn slice_mut_slice_excluded_start() {
    let mut v = filled();
    let s = v.as_mut_slice();
    let sub =
        SoASliceMut::slice(&s, (Bound::Excluded(1usize), Bound::Unbounded));
    assert_eq!(sub.mass, &[2.0, 3.0, 4.0]);
}

#[test]
fn slice_mut_slice_mut_excluded_start() {
    let mut v = filled();
    let mut s = v.as_mut_slice();
    let sub = SoASliceMut::slice_mut(
        &mut s,
        (Bound::Excluded(1usize), Bound::Unbounded),
    );
    assert_eq!(sub.mass, &[2.0, 3.0, 4.0]);
}

#[test]
fn excluded_start_with_included_end() {
    let v = filled();
    let sub =
        SoAVec::slice(&v, (Bound::Excluded(0usize), Bound::Included(3usize)));
    assert_eq!(sub.mass, &[1.0, 2.0, 3.0]);
}

#[test]
fn excluded_start_and_excluded_end() {
    let v = filled();
    let sub =
        SoAVec::slice(&v, (Bound::Excluded(1usize), Bound::Excluded(4usize)));
    assert_eq!(sub.mass, &[2.0, 3.0]);
}

// Included / Unbounded starts were always correct; pin them so the fix
// cannot disturb them.

#[test]
fn included_start_unchanged() {
    let v = filled();
    let sub = SoAVec::slice(&v, (Bound::Included(1usize), Bound::Unbounded));
    assert_eq!(sub.mass, &[1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn unbounded_start_unchanged() {
    let v = filled();
    let sub = SoAVec::slice(&v, ..3usize);
    assert_eq!(sub.mass, &[0.0, 1.0, 2.0]);
}

// Degenerate case: excluding the last valid index yields an empty slice
// rather than a one-element slice starting at the wrong spot.

#[test]
fn excluded_last_index_is_empty() {
    let v = filled();
    let sub = SoAVec::slice(&v, (Bound::Excluded(4usize), Bound::Unbounded));
    assert!(sub.is_empty());
}

// An inclusive end past the last valid index must panic, like `slice[a..=b]`.
// The end-bound decoder previously clamped it to `len`.

#[test]
#[should_panic]
fn included_end_out_of_bounds_panics() {
    let v = filled(); // len 5
    let _ = SoAVec::slice(&v, (Bound::Unbounded, Bound::Included(10usize)));
}

#[test]
#[should_panic]
fn included_end_out_of_bounds_panics_mut() {
    let mut v = filled(); // len 5
    let _ =
        SoAVec::slice_mut(&mut v, (Bound::Unbounded, Bound::Included(10usize)));
}

#[test]
#[should_panic]
fn included_end_out_of_bounds_panics_slice() {
    let mut v = filled();
    let mut s = v.as_mut_slice();
    let _ = SoASliceMut::slice_mut(
        &mut s,
        (Bound::Unbounded, Bound::Included(10usize)),
    );
}
