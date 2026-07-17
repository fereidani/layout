//! The generated `Vec` must drop its rows in forward (insertion) order, like
//! `Vec<T>`. Previously the custom `Drop` drained with `pop()`, dropping
//! last-in first. A `Tracker` records its id on drop into a shared log so the
//! order is observable.

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
fn rows_drop_in_forward_order() {
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
        // `v` drops here; rows must drop as 0, 1, 2, 3, 4 (forward order).
    }
    let recorded = log.lock().unwrap();
    let expected: Vec<u32> = (0..5).collect();
    assert_eq!(
        *recorded, expected,
        "rows should drop in insertion order like Vec<T>"
    );
}

// A no-`Drop` row type must still drop cleanly via the `needs_drop` fast path.
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
