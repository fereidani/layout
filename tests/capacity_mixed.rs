//! Regression test for `capacity()` on structs that mix a plain (`Vec`)
//! column with a `Compact<T>` column.
//!
//! `Compact` columns have word-granular capacity (e.g. 256 for a 1-bit
//! column) while plain `Vec` columns have a smaller, element-granular
//! capacity. The old `capacity()` body debug-asserted that every column's
//! capacity was equal, which panicked for any mixed struct. The new body
//! returns the minimum capacity across all columns.

use layout::{Compact, CompactBool, SOA};

#[derive(SOA)]
struct C {
    id: u32,
    flag: CompactBool,
}

#[test]
fn capacity_does_not_panic_for_mixed_struct() {
    let mut v = CVec::new();
    v.push(C {
        id: 1,
        flag: Compact(true),
    });
    v.push(C {
        id: 2,
        flag: Compact(false),
    });

    // This alone is the regression: under the old `debug_assert_eq!` body this
    // debug-panicked because the compact column's word-granular capacity (256
    // for a 1-bit column) differs from the plain `Vec<u32>` capacity (4).
    let _ = v.capacity();
}

#[test]
fn capacity_is_min_after_reserve() {
    let mut v = CVec::new();
    v.push(C {
        id: 1,
        flag: Compact(true),
    });
    v.push(C {
        id: 2,
        flag: Compact(false),
    });

    v.reserve(1000);

    // The min capacity is a valid lower bound: after reserving 1000 additional
    // slots on every column, the binding (minimum) column must still have at
    // least len + 1000 free.
    assert!(v.capacity() >= v.len() + 1000);
}

#[test]
fn capacity_equals_min_of_columns() {
    let mut v = CVec::new();
    v.push(C {
        id: 1,
        flag: Compact(true),
    });
    v.push(C {
        id: 2,
        flag: Compact(false),
    });

    // Lock in the documented semantics: the returned value is exactly the
    // minimum across the columns.
    assert_eq!(v.capacity(), v.id.capacity().min(v.flag.capacity()));
}
