//! An element pointer taken from a `Ref`/`RefMut` must never be classified
//! as null just because the struct has a `Compact<_>` field: compact fields
//! of a `Ref` yield direct pointers to the value carried by the `Ref`, which
//! stay non-null and recoverable via `as_ref` while the `Ref` is alive.

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
    // The Ref must outlive the pointer use: its compact field carries the
    // value the direct pointer borrows.
    let r = e.as_ref();
    let p = r.as_ptr();
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
fn slice_element_ptr_roundtrip_with_compact_field() {
    let mut v = EVec::new();
    v.push(E {
        id: 1,
        flag: Compact::new(false),
    });
    v.push(E {
        id: 2,
        flag: Compact::new(true),
    });

    let s = v.as_slice();
    let p = s.as_ptr();
    assert!(!p.is_null());
    let back = unsafe { p.as_ref() }.expect("pointer to a valid element");
    assert_eq!(*back.id, 1);
    assert!(!back.flag.get());
}

#[test]
fn column_pointers_stay_storage_backed() {
    let mut v = EVec::new();
    v.push(E {
        id: 1,
        flag: Compact::new(false),
    });
    v.push(E {
        id: 2,
        flag: Compact::new(true),
    });

    // Column pointers address the packed store itself: unlike a Ref-derived
    // direct pointer, arithmetic reaches the other elements.
    let p = v.flag.as_ptr();
    assert!(!p.is_null());
    unsafe {
        assert!(!p.as_ref().unwrap().get());
        assert!(p.add(1).as_ref().unwrap().get());
    }
}
