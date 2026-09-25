//! Columns left with different lengths by safe code.
//!
//! The columns of a generated vector or slice are public, so safe code can
//! replace or swap one, or change the length of a nested or compact column
//! through its own methods. Row access must stay in bounds anyway: the
//! length is the shortest column's. Debug builds reject the state as soon
//! as the length is read, so there these tests expect that panic instead.

use layout::{Compact, SOA};

#[derive(Debug, Clone, PartialEq, SOA)]
#[layout(Debug, Clone, PartialEq)]
pub struct Point {
    pub x: u64,
    pub y: u64,
}

#[derive(Debug, Clone, PartialEq, SOA)]
#[layout(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: u64,
    #[nested_soa]
    pub point: Point,
    pub flag: Compact<bool>,
}

fn row(i: u64) -> Row {
    Row {
        id: i,
        point: Point { x: i, y: 10 * i },
        flag: Compact(i % 2 == 0),
    }
}

fn rows(n: u64) -> RowVec {
    (0..n).map(row).collect()
}

fn points(n: u64) -> PointVec {
    (0..n).map(|i| Point { x: i, y: 10 * i }).collect()
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "different lengths"))]
fn taken_plain_column_bounds_every_row_access() {
    let mut v = points(3);
    let _taken = core::mem::take(&mut v.y);
    assert_eq!(v.len(), 0);
    assert!(v.is_empty());
    assert!(v.get(0).is_none());
    assert_eq!(v.iter().count(), 0);
    assert_eq!(v.iter_mut().count(), 0);
    assert!(v.pop().is_none());
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "different lengths"))]
fn swapped_plain_columns_use_the_shortest() {
    let mut long = points(4);
    let mut short = points(2);
    core::mem::swap(&mut long.y, &mut short.y);
    assert_eq!(long.len(), 2);
    assert_eq!(short.len(), 2);
    let ys: Vec<u64> = long.iter().map(|p| *p.y).collect();
    assert_eq!(ys, [0, 10]);
    assert!(long.get(2).is_none());
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "different lengths"))]
fn popped_nested_column_then_pop_row() {
    let mut v = rows(2);
    assert!(v.point.pop().is_some());
    assert_eq!(v.len(), 1);
    // Every column still has at least one element left.
    let last = v.pop().expect("one row");
    assert_eq!(last.id, 1);
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "different lengths"))]
fn cleared_compact_column_bounds_index() {
    let mut v = rows(3);
    v.flag.clear();
    assert_eq!(v.len(), 0);
    assert!(v.get(2).is_none());
    let r = std::panic::catch_unwind(|| v.index(2).id);
    assert!(r.is_err());
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "different lengths"))]
fn pushed_compact_column_is_ignored() {
    let mut v = rows(2);
    v.flag.push(true);
    assert_eq!(v.len(), 2);
    assert_eq!(v.iter().count(), 2);
    v.retain(|r| *r.id == 1);
    assert_eq!(v.len(), 1);
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "different lengths"))]
fn mismatched_slice_literal() {
    let xs = [1u64, 2, 3];
    let ys = [10u64];
    let s = PointSlice { x: &xs, y: &ys };
    assert_eq!(s.len(), 1);
    assert_eq!(*s.index(0).x, 1);
    assert!(s.get(1).is_none());
    assert_eq!(s.iter().map(|p| *p.y).sum::<u64>(), 10);
    assert!(std::panic::catch_unwind(|| *s.index(2).y).is_err());
}

#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "different lengths"))]
fn mismatched_slice_mut_sorts_and_swaps_rows_only() {
    let mut xs = [3u64, 1, 2, 9];
    let mut ys = [30u64, 10, 20];
    let mut s = PointSliceMut {
        x: &mut xs,
        y: &mut ys,
    };
    assert_eq!(s.len(), 3);
    s.sort_by_key(|p| *p.x);
    s.swap(0, 2);
    assert!(std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
        s.swap(0, 3)
    }))
    .is_err());
    let chunks: usize = s.chunks_mut(2).map(|c| c.len()).sum();
    assert_eq!(chunks, 3);
    // The extra element of the longer column is never moved.
    assert_eq!(xs, [3, 2, 1, 9]);
    assert_eq!(ys, [30, 20, 10]);
}

#[cfg(feature = "serde")]
mod serde_input {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Clone, PartialEq, SOA)]
    #[layout(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Sample {
        pub a: u32,
        pub b: u32,
    }

    #[test]
    #[cfg_attr(debug_assertions, should_panic(expected = "different lengths"))]
    fn deserialized_columns_of_different_lengths() {
        let v: SampleVec =
            serde_json::from_str(r#"{"a":[1,2,3],"b":[4]}"#).unwrap();
        assert_eq!(v.len(), 1);
        assert!(v.get(2).is_none());
    }
}

#[test]
fn equal_columns_do_not_trip_the_assertion() {
    let mut v = rows(5);
    v.retain(|r| *r.id != 2);
    v.truncate(3);
    assert_eq!(v.len(), 3);
}
