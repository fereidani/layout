//! Ergonomic surface of `Compact<T>`: deref to the inner value, direct
//! comparisons against `T`, and `Into`-accepting `CompactVec` mutators.

use layout::{Compact, CompactVec};

#[test]
fn deref_and_compare() {
    let c = Compact::new(true);
    assert!(*c);
    assert_eq!(c, true);
    assert!(c > false);

    let mut m = Compact::new(false);
    *m = true;
    assert!(m.get());
}

#[test]
fn mutators_accept_plain_values() {
    let mut v: CompactVec<bool> = CompactVec::new();
    v.push(true);
    v.push(Compact::new(false));
    v.insert(1, true);
    assert_eq!(v.len(), 3);
    assert!(v.get(1).unwrap().get());

    v.set(0, false);
    assert!(!v.get(0).unwrap().get());

    let old = v.replace(2, true);
    assert!(!old.get());
    assert!(v.get(2).unwrap().get());

    v.resize(70, true);
    assert_eq!(v.len(), 70);
    assert!(v.get(69).unwrap().get());
    v.resize(2, false);
    assert_eq!(v.len(), 2);
}

#[repr(u8)]
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, layout::CompactRepr,
)]
enum Kind {
    A,
    B,
}

#[test]
fn enum_columns_take_plain_variants() {
    let mut v: CompactVec<Kind> = CompactVec::new();
    v.push(Kind::A);
    v.push(Kind::B);
    assert_eq!(v.get(0).unwrap(), Kind::A);
    assert_eq!(*v.get(1).unwrap(), Kind::B);
}
