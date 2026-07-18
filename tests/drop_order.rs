//! Drop semantics of the generated `Vec`: columns drop one after another in
//! field declaration order, each column's elements in index order. This is
//! deliberate: letting every column (a plain `Vec`) drop itself is a
//! sequential `drop_in_place` per column, far cheaper than reassembling each
//! row through a composite drain just to reproduce `Vec<T>`'s row-major
//! order. A `Tracker` records its id on drop into a shared log so the order
//! is observable.

use std::sync::{Arc, Mutex};

use layout::SOA;

struct Tracker {
    id: u32,
    log: Arc<Mutex<Vec<u32>>>,
}

impl Drop for Tracker {
    fn drop(&mut self) {
        if let Ok(mut g) = self.log.lock() {
            g.push(self.id);
        }
    }
}

#[derive(SOA)]
struct Row {
    tracker: Tracker,
}

#[test]
fn single_column_drops_in_forward_order() {
    let log = Arc::new(Mutex::new(Vec::new()));
    {
        let mut v = RowVec::new();
        for i in 0..5_u32 {
            v.push(Row {
                tracker: Tracker {
                    id: i,
                    log: log.clone(),
                },
            });
        }
        // `v` drops here; the column drops as 0, 1, 2, 3, 4 (index order).
    }
    let recorded = log.lock().unwrap();
    let expected: Vec<u32> = (0..5).collect();
    assert_eq!(
        *recorded, expected,
        "a column should drop its elements in index order"
    );
}

#[derive(SOA)]
struct Pair {
    a: Tracker,
    b: Tracker,
}

#[test]
fn columns_drop_one_after_another() {
    // Two droppable columns: all of `a` drops before any of `b`
    // (column-major), unlike `Vec<Pair>` which would interleave row by row.
    let log = Arc::new(Mutex::new(Vec::new()));
    {
        let mut v = PairVec::new();
        for i in 0..3_u32 {
            v.push(Pair {
                a: Tracker {
                    id: i,
                    log: log.clone(),
                },
                b: Tracker {
                    id: 100 + i,
                    log: log.clone(),
                },
            });
        }
    }
    let recorded = log.lock().unwrap();
    let expected: Vec<u32> = vec![0, 1, 2, 100, 101, 102];
    assert_eq!(
        *recorded, expected,
        "columns should drop in declaration order, elements in index order"
    );
}

// A no-`Drop` row type must still drop cleanly.
#[derive(SOA)]
struct Plain {
    a: u64,
    b: u64,
}

#[test]
fn no_drop_type_drops_cleanly() {
    let mut v = PlainVec::new();
    for i in 0..1000 {
        v.push(Plain { a: i, b: i * 2 });
    }
    drop(v);
    // Reaching here without double-free / UB is the assertion.
}
