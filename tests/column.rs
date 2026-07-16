//! Regression coverage for plain columns stored as `Column<T>`.
//!
//! `Column<T>` dereferences to `[T]`, so reads and in-place element writes work
//! as on a slice, but its length-mutating methods are `unsafe`: the only safe
//! way to grow or shrink is the composite `Vec` API, which keeps every column
//! the same length. These tests pin both behaviors.

use layout::{Column, SOA};

#[derive(Clone, Debug, PartialEq, SOA)]
#[layout(Clone, Debug, PartialEq)]
struct Row {
    id: u32,
    mass: f64,
}

#[test]
fn column_deref_read_and_write() {
    let mut v = RowVec::new();
    v.push(Row { id: 1, mass: 10.0 });
    v.push(Row { id: 2, mass: 20.0 });

    // Reads through `Deref<Target = [T]>`.
    assert_eq!(v.id[0], 1);
    assert_eq!(v.mass[1], 20.0);
    let ids: Vec<u32> = v.id.iter().copied().collect();
    assert_eq!(ids, vec![1, 2]);

    // In-place element write through `DerefMut`.
    v.mass[0] = 99.0;
    assert_eq!(v.mass[0], 99.0);

    // The columns really are `Column<T>`.
    let _: &Column<u32> = &v.id;
    let _: &Column<f64> = &v.mass;
}

#[test]
fn composite_ops_keep_columns_synced() {
    let mut v = RowVec::new();
    v.push(Row { id: 1, mass: 10.0 });
    v.push(Row { id: 2, mass: 20.0 });
    v.push(Row { id: 3, mass: 30.0 });

    assert_eq!(v.remove(1), Row { id: 2, mass: 20.0 });
    assert_eq!(v.len(), 2);
    assert_eq!(v.id[1], 3);

    v.insert(0, Row { id: 9, mass: 99.0 });
    assert_eq!(v.id[0], 9);

    v.truncate(1);
    assert_eq!(v.len(), 1);
    assert_eq!(v.id[0], 9);
    assert_eq!(v.mass[0], 99.0);
}

#[test]
fn column_clone_and_eq() {
    let mut v = RowVec::new();
    v.push(Row { id: 1, mass: 10.0 });
    let copy = v.clone();
    assert_eq!(v, copy);

    // `Column` is itself `Clone`.
    let cloned: Column<u32> = v.id.clone();
    assert_eq!(&cloned[..], &[1][..]);
}
