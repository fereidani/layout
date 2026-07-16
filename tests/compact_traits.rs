//! Trait impls on `CompactVec<T>`: `PartialEq`/`Eq`, `Hash`,
//! `FromIterator`/`Extend`, and value-listing `Debug`.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use layout::{Compact, CompactRepr, CompactVec};

/// 3 variants (max discriminant 2) -> 2-bit storage.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, CompactRepr)]
enum Kind {
    Red,
    Green,
    Blue,
}

fn hash_of<T: Hash>(value: &T) -> u64 {
    let mut state = DefaultHasher::new();
    value.hash(&mut state);
    state.finish()
}

// ---------------------------------------------------------------------------
// PartialEq / Eq
// ---------------------------------------------------------------------------

#[test]
fn bool_column_eq() {
    let a: CompactVec<bool> =
        [true, false, true].iter().copied().map(Compact).collect();
    let b: CompactVec<bool> =
        [true, false, true].iter().copied().map(Compact).collect();
    assert_eq!(a, b);
}

#[test]
fn bool_column_ne_by_element() {
    let a: CompactVec<bool> =
        [true, false, true].iter().copied().map(Compact).collect();
    let b: CompactVec<bool> =
        [true, true, true].iter().copied().map(Compact).collect();
    assert_ne!(a, b);
}

#[test]
fn bool_column_ne_by_length() {
    let a: CompactVec<bool> =
        [true, false].iter().copied().map(Compact).collect();
    let b: CompactVec<bool> =
        [true, false, true].iter().copied().map(Compact).collect();
    assert_ne!(a, b);
}

#[test]
fn enum_column_eq_and_ne() {
    let a: CompactVec<Kind> = [Kind::Red, Kind::Green, Kind::Blue]
        .iter()
        .copied()
        .map(Compact)
        .collect();
    let b: CompactVec<Kind> = [Kind::Red, Kind::Green, Kind::Blue]
        .iter()
        .copied()
        .map(Compact)
        .collect();
    let c: CompactVec<Kind> = [Kind::Red, Kind::Blue, Kind::Blue]
        .iter()
        .copied()
        .map(Compact)
        .collect();

    assert_eq!(a, b);
    assert_ne!(a, c);
}

// ---------------------------------------------------------------------------
// Hash
// ---------------------------------------------------------------------------

#[test]
fn equal_columns_hash_equally() {
    let a: CompactVec<bool> =
        [true, false, true].iter().copied().map(Compact).collect();
    let b: CompactVec<bool> =
        [true, false, true].iter().copied().map(Compact).collect();
    assert_eq!(hash_of(&a), hash_of(&b));
}

#[test]
fn unequal_columns_hash_differently() {
    let a: CompactVec<bool> =
        [true, false, true].iter().copied().map(Compact).collect();
    let b: CompactVec<bool> =
        [true, true, true].iter().copied().map(Compact).collect();
    assert_ne!(hash_of(&a), hash_of(&b));
}

#[test]
fn enum_column_hash_matches_contents() {
    let a: CompactVec<Kind> = [Kind::Red, Kind::Green]
        .iter()
        .copied()
        .map(Compact)
        .collect();
    let b: CompactVec<Kind> = [Kind::Red, Kind::Green]
        .iter()
        .copied()
        .map(Compact)
        .collect();
    let c: CompactVec<Kind> = [Kind::Red, Kind::Blue]
        .iter()
        .copied()
        .map(Compact)
        .collect();
    assert_eq!(hash_of(&a), hash_of(&b));
    assert_ne!(hash_of(&a), hash_of(&c));
}

// ---------------------------------------------------------------------------
// FromIterator / Extend
// ---------------------------------------------------------------------------

#[test]
fn from_iter_builds_contents() {
    let v: CompactVec<bool> =
        [true, false, true].iter().copied().map(Compact).collect();
    assert_eq!(v.len(), 3);
    assert_eq!(v.get(0), Some(Compact(true)));
    assert_eq!(v.get(1), Some(Compact(false)));
    assert_eq!(v.get(2), Some(Compact(true)));
}

#[test]
fn from_iter_empty() {
    let v: CompactVec<bool> = std::iter::empty::<Compact<bool>>().collect();
    assert!(v.is_empty());
}

#[test]
fn extend_appends() {
    let mut v: CompactVec<bool> = [true].iter().copied().map(Compact).collect();
    v.extend([false, true].iter().copied().map(Compact));
    assert_eq!(v.len(), 3);
    assert_eq!(v.get(0), Some(Compact(true)));
    assert_eq!(v.get(1), Some(Compact(false)));
    assert_eq!(v.get(2), Some(Compact(true)));
}

#[test]
fn extend_empty_is_noop() {
    let mut v: CompactVec<Kind> =
        [Kind::Red].iter().copied().map(Compact).collect();
    v.extend(std::iter::empty::<Compact<Kind>>());
    assert_eq!(v.len(), 1);
    assert_eq!(v.get(0), Some(Compact(Kind::Red)));
}

// ---------------------------------------------------------------------------
// Debug (value-listing)
// ---------------------------------------------------------------------------

#[test]
fn debug_bool_lists_values() {
    let v: CompactVec<bool> =
        [true, false].iter().copied().map(Compact).collect();
    let s = format!("{:?}", v);
    assert!(s.contains("true"), "got: {}", s);
    assert!(s.contains("false"), "got: {}", s);
    // Must NOT be the old `len`-only form.
    assert!(!s.starts_with("CompactVec { len:"), "got: {}", s);
}

#[test]
fn debug_enum_lists_values() {
    let v: CompactVec<Kind> = [Kind::Red, Kind::Green, Kind::Blue]
        .iter()
        .copied()
        .map(Compact)
        .collect();
    let s = format!("{:?}", v);
    assert!(s.contains("Red"), "got: {}", s);
    assert!(s.contains("Green"), "got: {}", s);
    assert!(s.contains("Blue"), "got: {}", s);
}

#[test]
fn debug_slice_lists_values() {
    let v: CompactVec<bool> =
        [true, false].iter().copied().map(Compact).collect();
    let s = format!("{:?}", v.as_slice());
    assert!(s.contains("true"), "got: {}", s);
    assert!(s.contains("false"), "got: {}", s);
}

#[test]
fn debug_slice_mut_lists_values() {
    let mut v: CompactVec<bool> =
        [true, false].iter().copied().map(Compact).collect();
    let s = format!("{:?}", v.as_mut_slice());
    assert!(s.contains("true"), "got: {}", s);
    assert!(s.contains("false"), "got: {}", s);
}
