//! `retain` / `retain_mut` compact rows in place: kept rows move down in
//! order, rejected rows are dropped exactly once, and a panicking predicate
//! leaves the vector consistent (rejected rows dropped, the rest intact).

use std::{
    cell::RefCell,
    panic::{catch_unwind, AssertUnwindSafe},
    rc::Rc,
};

use layout::{Compact, SOA};

#[derive(SOA, Clone, Debug, PartialEq)]
#[layout(Clone, Debug, PartialEq)]
struct Inner {
    x: u32,
    y: u32,
}

struct Tracker {
    id: u32,
    log: Rc<RefCell<Vec<u32>>>,
}

impl Drop for Tracker {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.id);
    }
}

#[derive(SOA)]
struct Row {
    id: u32,
    name: String,
    tracker: Tracker,
    flag: Compact<bool>,
    #[nested_soa]
    inner: Inner,
}

fn lcg(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

fn build(n: u32, log: &Rc<RefCell<Vec<u32>>>) -> RowVec {
    let mut v = RowVec::new();
    for id in 0..n {
        v.push(Row {
            id,
            name: format!("row{id}"),
            tracker: Tracker {
                id,
                log: log.clone(),
            },
            flag: Compact::new(id % 2 == 0),
            inner: Inner {
                x: id * 10,
                y: id * 100,
            },
        });
    }
    v
}

fn ids(v: &RowVec) -> Vec<u32> {
    v.iter().map(|r| *r.id).collect()
}

/// Every column of every row must still describe the same `id`.
fn check_rows_consistent(v: &RowVec) {
    for r in v.iter() {
        let id = *r.id;
        assert_eq!(r.name, &format!("row{id}"));
        assert_eq!(r.tracker.id, id);
        assert_eq!(r.flag.get(), id % 2 == 0);
        assert_eq!(*r.inner.x, id * 10);
        assert_eq!(*r.inner.y, id * 100);
    }
}

#[test]
fn retain_keeps_order_and_drops_rejected_once() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut v = build(100, &log);
    v.retain(|r| *r.id % 3 != 0);
    let want: Vec<u32> = (0..100).filter(|id| id % 3 != 0).collect();
    assert_eq!(ids(&v), want);
    check_rows_consistent(&v);
    let dropped: Vec<u32> = (0..100).filter(|id| id % 3 == 0).collect();
    assert_eq!(*log.borrow(), dropped, "rejected rows drop in order");
    drop(v);
    let mut all = log.borrow().clone();
    all.sort_unstable();
    let want_all: Vec<u32> = (0..100).collect();
    assert_eq!(all, want_all, "every row dropped exactly once");
}

#[test]
fn retain_edge_shapes() {
    for n in [0u32, 1, 2, 3, 64, 65] {
        for shape in 0..4 {
            let log = Rc::new(RefCell::new(Vec::new()));
            let mut v = build(n, &log);
            let keep = |id: u32| match shape {
                0 => true,
                1 => false,
                2 => id != 0,
                _ => id + 1 != n,
            };
            v.retain(|r| keep(*r.id));
            let want: Vec<u32> = (0..n).filter(|&id| keep(id)).collect();
            assert_eq!(ids(&v), want, "n={n} shape={shape}");
            check_rows_consistent(&v);
            let mut dropped = log.borrow().clone();
            dropped.sort_unstable();
            let want_dropped: Vec<u32> =
                (0..n).filter(|&id| !keep(id)).collect();
            assert_eq!(dropped, want_dropped, "n={n} shape={shape}");
        }
    }
}

#[test]
fn retain_matches_vec_retain_on_random_predicates() {
    let mut seed = 7u64;
    let sizes: &[u32] = if cfg!(miri) {
        &[0, 1, 5, 64, 65, 130]
    } else {
        &[0, 1, 5, 63, 64, 65, 200, 1000]
    };
    for &n in sizes {
        for _ in 0..4 {
            let keep: Vec<bool> =
                (0..n).map(|_| lcg(&mut seed) % 4 != 0).collect();
            let log = Rc::new(RefCell::new(Vec::new()));
            let mut v = build(n, &log);
            v.retain(|r| keep[*r.id as usize]);
            let mut oracle: Vec<u32> = (0..n).collect();
            oracle.retain(|&id| keep[id as usize]);
            assert_eq!(ids(&v), oracle, "n={n}");
            check_rows_consistent(&v);
            assert_eq!(log.borrow().len() as u32, n - oracle.len() as u32);
        }
    }
}

#[test]
fn retain_mut_mutates_kept_rows() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut v = build(20, &log);
    v.retain_mut(|mut r| {
        r.name.make_ascii_uppercase();
        let flag = r.flag.get();
        r.flag.set(!flag);
        *r.inner.x += 1;
        *r.id % 4 != 1
    });
    let want: Vec<u32> = (0..20).filter(|id| id % 4 != 1).collect();
    assert_eq!(ids(&v), want);
    for r in v.iter() {
        let id = *r.id;
        assert_eq!(r.name, &format!("ROW{id}"));
        assert_eq!(r.flag.get(), id % 2 != 0, "flag toggled");
        assert_eq!(*r.inner.x, id * 10 + 1);
        assert_eq!(*r.inner.y, id * 100);
    }
}

#[test]
fn panicking_predicate_drops_rejected_and_keeps_the_rest() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut v = build(100, &log);
    let result = catch_unwind(AssertUnwindSafe(|| {
        v.retain(|r| {
            if *r.id == 50 {
                panic!("predicate failure");
            }
            *r.id % 3 != 0
        });
    }));
    assert!(result.is_err());
    // Rows before the panic were filtered; the row that panicked and every
    // row after it are untouched.
    let want: Vec<u32> =
        (0..50).filter(|id| id % 3 != 0).chain(50..100).collect();
    assert_eq!(ids(&v), want);
    check_rows_consistent(&v);
    let dropped: Vec<u32> = (0..50).filter(|id| id % 3 == 0).collect();
    assert_eq!(*log.borrow(), dropped);
    drop(v);
    let mut all = log.borrow().clone();
    all.sort_unstable();
    let want_all: Vec<u32> = (0..100).collect();
    assert_eq!(all, want_all, "every row dropped exactly once");
}

#[test]
fn panicking_predicate_before_any_rejection_changes_nothing() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut v = build(10, &log);
    let result = catch_unwind(AssertUnwindSafe(|| {
        v.retain(|r| {
            if *r.id == 4 {
                panic!("predicate failure");
            }
            true
        });
    }));
    assert!(result.is_err());
    let want: Vec<u32> = (0..10).collect();
    assert_eq!(ids(&v), want);
    check_rows_consistent(&v);
    assert!(log.borrow().is_empty());
}
