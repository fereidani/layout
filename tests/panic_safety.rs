//! Column operations that unwind partway through the columns.
//!
//! A panicking `Clone` or `Drop` stops a column operation after some columns
//! were changed and before the others were; the vector must still come back
//! with every column at one length, holding whole rows.

use std::{
    cell::Cell,
    ops::{Bound, RangeBounds},
    panic::{catch_unwind, AssertUnwindSafe},
    rc::Rc,
};

use layout::{SoAAppendVec, SOA};

/// Panics when cloned if `bomb` is set.
#[derive(Debug, PartialEq)]
pub struct CloneBomb {
    pub id: u64,
    pub bomb: bool,
}

impl Clone for CloneBomb {
    fn clone(&self) -> Self {
        assert!(!self.bomb, "clone bomb");
        CloneBomb {
            id: self.id,
            bomb: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, SOA)]
#[layout(Debug, Clone, PartialEq)]
pub struct Cloned {
    pub id: u64,
    pub payload: CloneBomb,
}

fn cloned(id: u64, bomb: bool) -> Cloned {
    Cloned {
        id,
        payload: CloneBomb { id, bomb },
    }
}

fn assert_whole_rows(v: &ClonedVec) {
    assert_eq!(v.id.len(), v.payload.len());
    for row in v.iter() {
        assert_eq!(*row.id, row.payload.id);
    }
}

#[test]
fn extend_from_slice_keeps_whole_rows_when_clone_panics() {
    let mut src = ClonedVec::new();
    for i in 0..5 {
        src.push(cloned(i, i == 3));
    }
    let mut v = ClonedVec::new();
    v.push(cloned(100, false));
    let r = catch_unwind(AssertUnwindSafe(|| {
        v.extend_from_slice(src.as_slice());
    }));
    assert!(r.is_err());
    // Some prefix of rows 0..3 may have been kept, but never half a row.
    assert!((1..=4).contains(&v.len()));
    assert_eq!(*v.index(0).id, 100);
    assert_whole_rows(&v);
}

#[test]
fn resize_keeps_whole_rows_when_clone_panics() {
    let mut v = ClonedVec::new();
    v.push(cloned(7, false));
    let r = catch_unwind(AssertUnwindSafe(|| {
        v.resize(4, cloned(7, true));
    }));
    assert!(r.is_err());
    assert_eq!(v.len(), 1);
    assert_whole_rows(&v);
}

/// Counts its drops, and panics when dropped if `bomb` is set.
pub struct DropBomb {
    pub drops: Rc<Cell<usize>>,
    pub bomb: bool,
}

impl Drop for DropBomb {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        if self.bomb {
            panic!("drop bomb");
        }
    }
}

// The dropping column comes first, so a panic there leaves the others
// untouched unless the vector restores them.
#[derive(SOA)]
pub struct Dropped {
    pub payload: DropBomb,
    pub id: u64,
}

fn dropped_vec(n: u64, bomb_at: u64, drops: &Rc<Cell<usize>>) -> DroppedVec {
    let mut v = DroppedVec::new();
    for i in 0..n {
        v.push(Dropped {
            payload: DropBomb {
                drops: Rc::clone(drops),
                bomb: i == bomb_at,
            },
            id: i,
        });
    }
    v
}

#[test]
fn truncate_keeps_columns_equal_when_drop_panics() {
    let drops = Rc::new(Cell::new(0));
    let mut v = dropped_vec(5, 2, &drops);
    let r = catch_unwind(AssertUnwindSafe(|| v.truncate(1)));
    assert!(r.is_err());
    assert_eq!(v.payload.len(), 1);
    assert_eq!(v.id.len(), 1);
    // Every truncated element was dropped once, the panicking one included.
    assert_eq!(drops.get(), 4);
    drop(v);
    assert_eq!(drops.get(), 5);
}

#[test]
fn clear_keeps_columns_equal_when_drop_panics() {
    let drops = Rc::new(Cell::new(0));
    let mut v = dropped_vec(3, 0, &drops);
    let r = catch_unwind(AssertUnwindSafe(|| v.clear()));
    assert!(r.is_err());
    assert!(v.payload.is_empty());
    assert!(v.id.is_empty());
    assert_eq!(drops.get(), 3);
}

/// Answers `..0` to its first `end_bound` call and `..` afterwards.
#[derive(Clone)]
struct Fickle<'a>(&'a Cell<usize>);

impl RangeBounds<usize> for Fickle<'_> {
    fn start_bound(&self) -> Bound<&usize> {
        Bound::Unbounded
    }

    fn end_bound(&self) -> Bound<&usize> {
        let calls = self.0.get();
        self.0.set(calls + 1);
        if calls == 0 {
            Bound::Excluded(&0)
        } else {
            Bound::Unbounded
        }
    }
}

#[test]
fn drain_resolves_the_range_once() {
    let mut v = ClonedVec::new();
    for i in 0..4 {
        v.push(cloned(i, false));
    }
    let calls = Cell::new(0);
    assert_eq!(v.drain(Fickle(&calls)).count(), 0);
    assert_eq!(v.len(), 4);
    assert_whole_rows(&v);
}

#[test]
fn drain_rejects_an_overflowing_inclusive_end() {
    let mut v = ClonedVec::new();
    v.push(cloned(0, false));
    let r = catch_unwind(AssertUnwindSafe(|| {
        v.drain(..=usize::MAX).count();
    }));
    assert!(r.is_err());
    assert_eq!(v.len(), 1);
}
