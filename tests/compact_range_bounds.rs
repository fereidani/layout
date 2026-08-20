//! Range-bound handling for the compact column's range-taking methods.
//!
//! An inclusive end of `usize::MAX` has no exclusive equivalent. It must be
//! rejected like `Vec`'s, not wrapped to an empty range that silently does
//! nothing (which is what plain `+ 1` does once overflow checks are off).
extern crate alloc;

use core::ops::Bound;

use layout::{Compact, CompactVec};

fn bv(len: usize) -> CompactVec<bool> {
    (0..len).map(|i| Compact(i % 2 == 0)).collect()
}

#[test]
#[should_panic]
fn drain_inclusive_max_end_panics() {
    let mut v = bv(8);
    let _ = v.drain(..=usize::MAX);
}

#[test]
#[should_panic]
fn splice_inclusive_max_end_panics() {
    let mut v = bv(8);
    let _ = v.splice(..=usize::MAX, [Compact(true)]);
}

#[test]
#[should_panic]
fn drain_excluded_max_start_panics() {
    let mut v = bv(8);
    let _ = v.drain((Bound::Excluded(usize::MAX), Bound::Unbounded));
}

#[test]
fn drain_inclusive_end_within_bounds() {
    let mut v = bv(8);
    let drained: alloc::vec::Vec<_> = v.drain(2..=4).collect();
    assert_eq!(drained.len(), 3);
    assert_eq!(v.len(), 5);
}

#[test]
fn drain_inclusive_last_index() {
    let mut v = bv(8);
    let drained: alloc::vec::Vec<_> = v.drain(5..=7).collect();
    assert_eq!(drained.len(), 3);
    assert_eq!(v.len(), 5);
}

#[test]
fn splice_inclusive_end_within_bounds() {
    let mut v = bv(8);
    let removed = v.splice(1..=2, [Compact(false)]);
    assert_eq!(removed.len(), 2);
    assert_eq!(v.len(), 7);
}
