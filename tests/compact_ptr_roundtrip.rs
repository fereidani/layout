//! An element pointer taken from a `Ref`/`RefMut` must never be classified
//! as null just because the struct has a `Compact<_>` field: compact fields
//! carried as value snapshots produce snapshot pointers that stay non-null
//! and recoverable via `as_ref`.

use layout::{Compact, SOA};

#[derive(SOA)]
pub struct E {
    pub id: u32,
    pub flag: Compact<bool>,
}

#[derive(SOA)]
pub struct AllCompact {
    pub flag: Compact<bool>,
}

#[test]
fn ref_ptr_roundtrip_with_compact_field() {
    let mut v = EVec::new();
    v.push(E {
        id: 7,
        flag: Compact::new(true),
    });
    v.push(E {
        id: 9,
        flag: Compact::new(false),
    });

    let r = v.index(1usize);
    let p = r.as_ptr();
    assert!(!p.is_null());
    let back = unsafe { p.as_ref() }.expect("pointer to a valid element");
    assert_eq!(*back.id, 9);
    assert!(!back.flag.get());
}

#[test]
fn ref_mut_ptr_roundtrip_with_compact_field() {
    let mut v = EVec::new();
    v.push(E {
        id: 7,
        flag: Compact::new(true),
    });

    let r = v.index_mut(0usize);
    let p = r.as_ptr();
    assert!(!p.is_null());
    let back = unsafe { p.as_ref() }.expect("pointer to a valid element");
    assert_eq!(*back.id, 7);
    assert!(back.flag.get());
}

#[test]
fn owned_ref_ptr_roundtrip_with_compact_field() {
    let e = E {
        id: 3,
        flag: Compact::new(true),
    };
    let p = e.as_ref().as_ptr();
    assert!(!p.is_null());
    let back = unsafe { p.as_ref() }.expect("pointer to a valid element");
    assert_eq!(*back.id, 3);
    assert!(back.flag.get());
}

#[test]
fn all_compact_ref_ptr_roundtrip() {
    let mut v = AllCompactVec::new();
    v.push(AllCompact {
        flag: Compact::new(true),
    });

    let r = v.index(0usize);
    let p = r.as_ptr();
    assert!(!p.is_null());
    let back = unsafe { p.as_ref() }.expect("pointer to a valid element");
    assert!(back.flag.get());
}

#[test]
fn slice_and_vec_compact_ptrs_stay_storage_backed() {
    let mut v = EVec::new();
    v.push(E {
        id: 1,
        flag: Compact::new(false),
    });

    // Column pointers from the vec/slice read live storage, not a snapshot:
    // a later write must be visible through a previously taken pointer.
    let p = v.as_slice().as_ptr();
    assert!(!p.is_null());
    v.index_mut(0usize).flag.set(true);
    let back = unsafe { p.as_ref() }.expect("pointer to a valid element");
    assert!(back.flag.get());
}
