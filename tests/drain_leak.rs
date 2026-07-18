//! Leak safety of drains: forgetting a drain (safe code) must leave every
//! column short but consistent, mirroring `Vec::drain` semantics. A leaked
//! composite drain previously left compact columns at their full pre-drain
//! length while plain columns were already shortened, desynchronizing the
//! shared-length invariant that unchecked indexing relies on.

use core::mem::forget;

use layout::{bitpack::PackedArray, Compact, CompactVec, SOA};

#[derive(SOA)]
pub struct Item {
    // Compact column FIRST: the generated `len()` reads field 0, so this is
    // the order that previously turned a leaked drain into out-of-bounds
    // reads on the plain column.
    pub flag: Compact<bool>,
    pub name: String,
}

fn build_items(n: usize) -> ItemVec {
    let mut v = ItemVec::new();
    for i in 0..n {
        v.push(Item {
            flag: Compact::new(i % 2 == 0),
            name: format!("s{i}"),
        });
    }
    v
}

// The String-column tests below leak the drained elements ON PURPOSE
// (`mem::forget` of a live drain is the scenario under test), so Miri's
// leak checker rightly reports them; they are ignored under Miri. The
// `copy_columns` variants further down cover the same length-consistency
// logic with nothing to leak and do run under Miri.

#[test]
#[cfg_attr(miri, ignore = "leaks drained Strings by design")]
fn leaked_composite_drain_keeps_columns_consistent() {
    let mut v = build_items(4);
    forget(v.drain(1..3));
    // All columns agree on the shortened length.
    assert_eq!(v.flag.len(), 1);
    assert_eq!(v.name.len(), 1);
    assert_eq!(v.len(), 1);
    assert_eq!(v.get(0).unwrap().name, "s0");
    assert!(v.get(1).is_none());
    // The vec stays fully usable.
    v.push(Item {
        flag: Compact::new(false),
        name: "new".into(),
    });
    assert_eq!(v.len(), 2);
    assert!(!v.get(1).unwrap().flag.get());
    assert_eq!(v.get(1).unwrap().name, "new");
}

#[test]
#[cfg_attr(miri, ignore = "leaks the tail Strings by design")]
fn leaked_empty_range_drain_matches_vec_semantics() {
    // `forget(vec.drain(2..2))` on a std Vec leaves len == 2 (the tail is
    // leaked); the composite drain must do the same on every column.
    let mut v = build_items(5);
    forget(v.drain(2..2));
    assert_eq!(v.len(), 2);
    assert_eq!(v.flag.len(), 2);
    assert_eq!(v.name.len(), 2);
}

#[test]
fn leaked_compact_drain_shortens_column() {
    let mut cv: CompactVec<bool> =
        (0..10).map(|i| Compact::new(i % 3 == 0)).collect();
    forget(cv.drain(3..7));
    assert_eq!(cv.len(), 3);
    for i in 0..3 {
        assert_eq!(cv.get(i).unwrap().get(), i % 3 == 0, "at {i}");
    }
    // Push after the leak must clear stale lane bits.
    cv.push(Compact::new(false));
    cv.push(Compact::new(true));
    assert_eq!(cv.len(), 5);
    assert!(!cv.get(3).unwrap().get());
    assert!(cv.get(4).unwrap().get());
}

#[test]
fn leaked_word_aligned_drain_then_bulk_ops() {
    // Leak with a word-aligned remaining length so the store keeps stale
    // trailing words; the bulk fast paths must detect the slack store and
    // fall back to per-lane copies.
    let n = 200;
    let mut cv: CompactVec<bool> =
        (0..n).map(|i| Compact::new(i % 3 == 0)).collect();
    forget(cv.drain(64..n));
    assert_eq!(cv.len(), 64);

    // append (aligned destination, slack words).
    let mut other: CompactVec<bool> =
        (0..130).map(|i| Compact::new(i % 5 == 0)).collect();
    cv.append(&mut other);
    assert_eq!(cv.len(), 194);
    for i in 0..64 {
        assert_eq!(cv.get(i).unwrap().get(), i % 3 == 0, "kept at {i}");
    }
    for i in 0..130 {
        assert_eq!(
            cv.get(64 + i).unwrap().get(),
            i % 5 == 0,
            "appended at {i}"
        );
    }

    // resize growth (extend_fill) on a freshly leaked slack store.
    let mut cv2: CompactVec<bool> =
        (0..n).map(|_| Compact::new(true)).collect();
    forget(cv2.drain(64..n));
    cv2.resize(300, Compact::new(false));
    assert_eq!(cv2.len(), 300);
    for i in 0..64 {
        assert!(cv2.get(i).unwrap().get(), "kept at {i}");
    }
    for i in 64..300 {
        assert!(!cv2.get(i).unwrap().get(), "filled at {i}");
    }
}

#[test]
fn completed_drain_still_shifts_and_tightens() {
    // The reworked drain must keep the pre-existing (non-leaked) behavior.
    let mut cv: CompactVec<bool> =
        (0..150).map(|i| Compact::new(i % 2 == 0)).collect();
    let drained: Vec<bool> = cv.drain(10..100).map(|c| c.get()).collect();
    assert_eq!(drained.len(), 90);
    for (i, &d) in drained.iter().enumerate() {
        assert_eq!(d, (10 + i) % 2 == 0, "drained at {i}");
    }
    assert_eq!(cv.len(), 60);
    for i in 0..10 {
        assert_eq!(cv.get(i).unwrap().get(), i % 2 == 0, "head at {i}");
    }
    for i in 10..60 {
        assert_eq!(cv.get(i).unwrap().get(), (i + 90) % 2 == 0, "tail at {i}");
    }
    // Word-copy fast paths work after the drain (the store is tight again).
    let mut other: CompactVec<bool> =
        (0..64).map(|_| Compact::new(true)).collect();
    let mut base = CompactVec::<bool>::new();
    base.append(&mut other);
    assert_eq!(base.len(), 64);
    assert!(base.iter().all(|c| c.get()));
}

#[test]
#[cfg_attr(miri, ignore = "leaks drained Strings by design")]
fn partially_consumed_leaked_drain_is_consistent() {
    let mut v = build_items(6);
    let mut d = v.drain(1..5);
    assert_eq!(d.next().unwrap().name, "s1");
    assert_eq!(d.next_back().unwrap().name, "s4");
    forget(d);
    assert_eq!(v.len(), 1);
    assert_eq!(v.flag.len(), 1);
    assert_eq!(v.name.len(), 1);
    assert_eq!(v.get(0).unwrap().name, "s0");
}

// Miri-runnable equivalents of the leaky tests above: a Copy payload column
// means a forgotten drain leaks no heap allocation, while the column
// length-consistency logic under test is identical.

#[derive(SOA)]
pub struct CopyItem {
    pub flag: Compact<bool>,
    pub id: u32,
}

fn build_copy_items(n: usize) -> CopyItemVec {
    let mut v = CopyItemVec::new();
    for i in 0..n {
        v.push(CopyItem {
            flag: Compact::new(i % 2 == 0),
            id: i as u32,
        });
    }
    v
}

#[test]
fn leaked_composite_drain_consistent_copy_columns() {
    let mut v = build_copy_items(4);
    forget(v.drain(1..3));
    assert_eq!(v.flag.len(), 1);
    assert_eq!(v.id.len(), 1);
    assert_eq!(v.len(), 1);
    assert_eq!(*v.get(0).unwrap().id, 0);
    assert!(v.get(1).is_none());
    v.push(CopyItem {
        flag: Compact::new(false),
        id: 9,
    });
    assert_eq!(v.len(), 2);
    assert!(!v.get(1).unwrap().flag.get());
    assert_eq!(*v.get(1).unwrap().id, 9);
}

#[test]
fn leaked_empty_range_drain_copy_columns() {
    let mut v = build_copy_items(5);
    forget(v.drain(2..2));
    assert_eq!(v.len(), 2);
    assert_eq!(v.flag.len(), 2);
    assert_eq!(v.id.len(), 2);
}

#[test]
fn partially_consumed_leaked_drain_copy_columns() {
    let mut v = build_copy_items(6);
    let mut d = v.drain(1..5);
    // The composite drain yields owned rows, so `id` is a plain `u32`.
    assert_eq!(d.next().unwrap().id, 1);
    assert_eq!(d.next_back().unwrap().id, 4);
    forget(d);
    assert_eq!(v.len(), 1);
    assert_eq!(v.flag.len(), 1);
    assert_eq!(v.id.len(), 1);
    assert_eq!(*v.get(0).unwrap().id, 0);
}

#[test]
fn packed_array_set_len_interplay() {
    // Direct store-level checks of the relaxed words invariant.
    let mut a = PackedArray::<1>::new();
    for i in 0..200 {
        a.push(i % 2);
    }
    // Lower the length while keeping the words (what a drain does), then
    // leak that state and keep using the store.
    // SAFETY: lanes < 5 are initialized and their word is allocated.
    unsafe { a.set_len(5) };
    assert_eq!(a.len(), 5);
    for i in 0..5 {
        assert_eq!(a.get(i), i % 2, "kept at {i}");
    }
    // push into a stale word must clear the old bits.
    a.push(0);
    a.push(0);
    assert_eq!(a.get(5), 0);
    assert_eq!(a.get(6), 0);
    // pop trims the stale words back to a tight store.
    assert_eq!(a.pop(), Some(0));
    let mut b = PackedArray::<1>::new();
    b.push(1);
    a.append(&mut b);
    assert_eq!(a.len(), 7);
    assert_eq!(a.get(6), 1);
}
