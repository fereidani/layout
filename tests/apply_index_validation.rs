//! `apply_index` must reject indices that are not a permutation of
//! `0..len` *before* touching any element, so malformed input can never
//! reach the unsafe cycle-walk (double-free) or spin forever.

use layout::{Compact, SoASliceMut, SOA};

#[derive(SOA)]
pub struct Item {
    pub name: String,
}

#[derive(SOA)]
pub struct Flags {
    pub flag: Compact<bool>,
}

#[test]
#[should_panic(expected = "does not match")]
fn apply_index_wrong_length_panics_cleanly() {
    let mut v = ItemVec::new();
    v.push(Item {
        name: "a".repeat(36),
    });
    v.push(Item {
        name: "b".repeat(36),
    });
    let mut s = v.as_mut_slice();
    // One index for two elements: must panic before any move, not corrupt
    // the heap during unwind.
    s.apply_index(&[0usize]);
}

#[test]
#[should_panic(expected = "duplicate")]
fn apply_index_duplicate_index_panics() {
    let mut v = ItemVec::new();
    for i in 0..3 {
        v.push(Item {
            name: i.to_string(),
        });
    }
    let mut s = v.as_mut_slice();
    // Not a permutation: previously walked a cycle forever.
    s.apply_index(&[1, 2, 2]);
}

#[test]
#[should_panic(expected = "out of bounds")]
fn apply_index_out_of_range_index_panics() {
    let mut v = ItemVec::new();
    for i in 0..3 {
        v.push(Item {
            name: i.to_string(),
        });
    }
    let mut s = v.as_mut_slice();
    s.apply_index(&[0, 1, 7]);
}

#[test]
fn apply_index_valid_permutation_still_works() {
    let mut v = ItemVec::new();
    for i in 0..4 {
        v.push(Item {
            name: i.to_string(),
        });
    }
    let mut s = v.as_mut_slice();
    s.apply_index(&[3, 1, 0, 2]);
    let names: Vec<_> = v.iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, ["3", "1", "0", "2"]);
}

#[test]
fn vec_apply_index_wrong_length_leaves_heap_intact() {
    // The double-free scenario: panic mid-cycle used to leave a bitwise
    // duplicate of a `String` alive in the slice while `temp` dropped the
    // same buffer during unwind. With up-front validation the panic fires
    // before any `ptr::read`, so dropping the vec afterwards is safe.
    let mut v = ItemVec::new();
    v.push(Item {
        name: "a".repeat(36),
    });
    v.push(Item {
        name: "b".repeat(36),
    });
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut s = v.as_mut_slice();
        s.apply_index(&[0usize]);
    }));
    assert!(r.is_err());
    // Elements must be untouched and safely droppable.
    assert_eq!(v.len(), 2);
    drop(v);
}

// Compact columns take a separate permutation path over packed storage
// (`CompactSliceMut::__private_apply_permutation`); an all-compact struct
// exercises it without any plain column catching the error first.

#[test]
#[should_panic(expected = "does not match")]
fn compact_apply_index_too_few_indices_panics_cleanly() {
    let mut v = FlagsVec::new();
    for i in 0..3 {
        v.push(Flags {
            flag: Compact::new(i % 2 == 0),
        });
    }
    let mut s = v.as_mut_slice();
    s.apply_index(&[0usize]);
}

#[test]
#[should_panic(expected = "does not match")]
fn compact_apply_index_too_many_indices_panics_cleanly() {
    let mut v = FlagsVec::new();
    for _ in 0..3 {
        v.push(Flags {
            flag: Compact::new(true),
        });
    }
    let mut s = v.as_mut_slice();
    // A valid 6-permutation, but the slice only has 3 elements.
    s.apply_index(&[3, 4, 5, 0, 1, 2]);
}

#[test]
#[should_panic(expected = "duplicate")]
fn compact_apply_index_duplicate_index_panics() {
    let mut v = FlagsVec::new();
    for _ in 0..3 {
        v.push(Flags {
            flag: Compact::new(true),
        });
    }
    let mut s = v.as_mut_slice();
    s.apply_index(&[1, 2, 2]);
}

#[test]
fn compact_apply_index_never_writes_outside_the_slice() {
    // An oversized permutation used to write packed lanes past the slice
    // extent, silently flipping live elements that belong to the rest of
    // the vec, before dying on an internal bounds check.
    let mut v = FlagsVec::new();
    for _ in 0..3 {
        v.push(Flags {
            flag: Compact::new(false),
        });
    }
    for _ in 0..3 {
        v.push(Flags {
            flag: Compact::new(true),
        });
    }
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut s = v.slice_mut(0..3);
        s.apply_index(&[3, 4, 5, 0, 1, 2]);
    }));
    assert!(r.is_err());
    let values: Vec<bool> = v.iter().map(|c| c.flag.get()).collect();
    assert_eq!(values, [false, false, false, true, true, true]);
}

#[test]
fn compact_apply_index_valid_permutation_still_works() {
    let mut v = FlagsVec::new();
    let pattern = [true, false, false, true];
    for &b in &pattern {
        v.push(Flags {
            flag: Compact::new(b),
        });
    }
    let mut s = v.as_mut_slice();
    s.apply_index(&[3, 1, 0, 2]);
    let values: Vec<bool> = v.iter().map(|c| c.flag.get()).collect();
    assert_eq!(values, [pattern[3], pattern[1], pattern[0], pattern[2]]);
}
