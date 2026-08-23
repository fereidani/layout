//! Column-cursor behaviour of the generated iterators: both ends, ZST
//! columns, and exhaustion.

use layout::SOA;

/// A zero-sized column, to exercise cursor arithmetic on a type whose stride
/// is zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marker;

#[derive(Debug, Clone, PartialEq, SOA)]
#[layout(Clone)]
pub struct Item {
    id: u32,
    weight: f64,
    label: String,
    marker: Marker,
}

fn item(i: u32) -> Item {
    Item {
        id: i,
        weight: f64::from(i) * 0.5,
        label: format!("item-{i}"),
        marker: Marker,
    }
}

fn build(n: u32) -> ItemVec {
    (0..n).map(item).collect()
}

#[test]
fn forward_yields_every_element_in_order() {
    let v = build(64);
    let ids: Vec<u32> = v.iter().map(|r| *r.id).collect();
    assert_eq!(ids, (0..64).collect::<Vec<_>>());
    assert!(v.iter().all(|r| *r.marker == Marker));
}

#[test]
fn backward_yields_every_element_in_reverse() {
    let v = build(64);
    let ids: Vec<u32> = v.iter().rev().map(|r| *r.id).collect();
    assert_eq!(ids, (0..64).rev().collect::<Vec<_>>());
}

#[test]
fn ends_meet_without_overlap() {
    let v = build(9);
    let mut it = v.iter();
    let mut front = Vec::new();
    let mut back = Vec::new();
    // Alternate ends until the iterator is drained; every element must be
    // yielded exactly once.
    loop {
        match it.next() {
            Some(r) => front.push(*r.id),
            None => break,
        }
        match it.next_back() {
            Some(r) => back.push(*r.id),
            None => break,
        }
    }
    assert!(it.next().is_none());
    assert!(it.next_back().is_none());
    back.reverse();
    front.extend(back);
    assert_eq!(front, (0..9).collect::<Vec<_>>());
}

#[test]
fn empty_and_single_element() {
    let empty = build(0);
    assert_eq!(empty.iter().count(), 0);
    assert!(empty.iter().next().is_none());
    assert!(empty.iter().next_back().is_none());

    let one = build(1);
    let mut it = one.iter();
    assert_eq!(*it.next().unwrap().id, 0);
    assert!(it.next_back().is_none());

    let mut it = one.iter();
    assert_eq!(*it.next_back().unwrap().id, 0);
    assert!(it.next().is_none());
}

#[test]
fn len_shrinks_from_both_ends() {
    let v = build(10);
    let mut it = v.iter();
    assert_eq!(it.len(), 10);
    it.next();
    assert_eq!(it.len(), 9);
    it.next_back();
    assert_eq!(it.len(), 8);
    assert_eq!(it.size_hint(), (8, Some(8)));
}

#[test]
fn mut_iteration_touches_each_element_once() {
    let mut v = build(33);
    let mut it = v.iter_mut();
    // Walk both ends, marking every element exactly once.
    while let Some(r) = it.next() {
        *r.id += 1000;
        if let Some(b) = it.next_back() {
            *b.id += 1000;
        }
    }
    assert!(v.iter().enumerate().all(|(i, r)| *r.id == i as u32 + 1000));
}

#[test]
fn mut_iteration_in_reverse() {
    let mut v = build(16);
    for r in v.iter_mut().rev() {
        *r.weight *= 2.0;
    }
    assert!(v
        .iter()
        .enumerate()
        .all(|(i, r)| *r.weight == f64::from(i as u32)));
}

#[test]
fn nth_and_skip_land_on_the_same_element() {
    let v = build(50);
    assert_eq!(*v.iter().nth(17).unwrap().id, 17);
    assert_eq!(*v.iter().nth(17).unwrap().id, 17);
    let mut skipped = v.iter().skip(17);
    assert_eq!(*skipped.next().unwrap().id, 17);
    assert_eq!(*v.iter().rev().nth(3).unwrap().id, 46);
    assert_eq!(*v.iter().last().unwrap().id, 49);
}

#[test]
fn slice_iteration_is_bounded_to_the_slice() {
    let v = build(20);
    let s = v.slice(5..12);
    let ids: Vec<u32> = s.iter().map(|r| *r.id).collect();
    assert_eq!(ids, (5..12).collect::<Vec<_>>());
    let ids: Vec<u32> = s.iter().rev().map(|r| *r.id).collect();
    assert_eq!(ids, (5..12).rev().collect::<Vec<_>>());
}
