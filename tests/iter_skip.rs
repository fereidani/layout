//! `nth`, `nth_back` and `last` on the generated iterators skip every
//! column cursor in one step, including bit-packed and nested columns.

use layout::{Compact, SOA};

#[derive(Debug, Clone, Copy, PartialEq, SOA)]
#[layout(Debug, Clone, PartialEq)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, SOA)]
#[layout(Debug, Clone, PartialEq)]
pub struct Unit {
    pub id: usize,
    #[nested_soa]
    pub pos: Pos,
    pub alive: Compact<bool>,
}

fn unit(i: usize) -> Unit {
    Unit {
        id: i,
        pos: Pos {
            x: i as i32,
            y: -(i as i32),
        },
        alive: Compact(i % 3 == 0),
    }
}

/// Spans several packed words, with a partial last word.
const N: usize = 200;

fn units() -> (UnitVec, Vec<Unit>) {
    let reference: Vec<Unit> = (0..N).map(unit).collect();
    (reference.iter().cloned().collect(), reference)
}

fn check(row: Option<UnitRef<'_>>, want: Option<&Unit>) {
    match (row, want) {
        (None, None) => {}
        (Some(row), Some(want)) => {
            assert_eq!(*row.id, want.id);
            assert_eq!(*row.pos.x, want.pos.x);
            assert_eq!(*row.pos.y, want.pos.y);
            assert_eq!(row.alive, want.alive);
        }
        (row, want) => panic!(
            "mismatch: got {:?}, want {:?}",
            row.map(|r| *r.id),
            want.map(|w| w.id)
        ),
    }
}

#[test]
fn nth_matches_a_vec_iterator() {
    let (v, reference) = units();
    for step in [0, 1, 2, 7, 63, 64, 65, 130, N - 1, N, N + 5] {
        let mut ours = v.iter();
        let mut theirs = reference.iter();
        for _ in 0..4 {
            check(ours.nth(step), theirs.nth(step));
            assert_eq!(ours.len(), theirs.len());
        }
    }
}

#[test]
fn nth_back_matches_a_vec_iterator() {
    let (v, reference) = units();
    for step in [0, 1, 3, 63, 64, 100, N] {
        let mut ours = v.iter();
        let mut theirs = reference.iter();
        for _ in 0..4 {
            check(ours.nth_back(step), theirs.nth_back(step));
            assert_eq!(ours.len(), theirs.len());
        }
    }
}

#[test]
fn skipping_from_both_ends_meets_in_the_middle() {
    let (v, reference) = units();
    let mut ours = v.iter();
    let mut theirs = reference.iter();
    for k in 0..40 {
        if k % 2 == 0 {
            check(ours.nth(k % 9), theirs.nth(k % 9));
        } else {
            check(ours.nth_back(k % 5), theirs.nth_back(k % 5));
        }
        check(ours.next(), theirs.next());
        assert_eq!(ours.len(), theirs.len());
    }
}

#[test]
fn step_by_and_skip_and_last() {
    let (v, reference) = units();
    let ours: Vec<usize> = v.iter().step_by(9).map(|u| *u.id).collect();
    let theirs: Vec<usize> =
        reference.iter().step_by(9).map(|u| u.id).collect();
    assert_eq!(ours, theirs);

    let alive: usize = v.iter().skip(70).filter(|u| u.alive.get()).count();
    let want = reference.iter().skip(70).filter(|u| u.alive.get()).count();
    assert_eq!(alive, want);

    check(v.iter().last(), reference.last());
    check(v.iter().skip(N).last(), None);
}

#[test]
fn iter_mut_skips_then_writes_the_right_rows() {
    let (mut v, _) = units();
    for mut u in v.iter_mut().step_by(10) {
        *u.pos.x = -1;
        u.alive.set(true);
    }
    {
        let mut it = v.iter_mut();
        let u = it.nth_back(4).expect("row");
        *u.id = 9999;
        let last = it.last().expect("row");
        assert_eq!(*last.id, N - 6);
    }
    for (i, u) in v.iter().enumerate() {
        let hit = i % 10 == 0;
        assert_eq!(*u.pos.x == -1, hit, "row {i}");
        assert_eq!(u.alive.get(), hit || i % 3 == 0, "row {i}");
    }
    assert_eq!(*v.index(N - 5).id, 9999);
}
