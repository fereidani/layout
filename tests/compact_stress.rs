//! Stress test for the compact column types, exercising the raw-pointer-heavy
//! paths (`splice`, `drain`, `sort`, `chunks_mut`, ptr read/write,
//! `split_*_mut`, `Default`) under Miri.
use layout::{Compact, CompactVec};

fn bv(xs: &[bool]) -> CompactVec<bool> {
    let mut v = CompactVec::new();
    for &x in xs {
        v.push(Compact(x));
    }
    v
}

#[test]
fn miri_insert_remove_swap_remove_replace_set() {
    let mut v = bv(&[true, false, true, true, false]);
    v.insert(2, Compact(false));
    assert_eq!(v.len(), 6);
    assert!(!v.get(2).unwrap().get());
    let r = v.remove(0);
    assert!(r.get());
    v.swap_remove(0);
    let _old = v.replace(0, Compact(true));
    v.set(v.len() - 1, Compact(false));
    assert!(!v.get(v.len() - 1).unwrap().get());
}

#[test]
fn miri_drain_shifts_on_drop() {
    let mut v = bv(&[true, false, true, false, true, false]);
    let drained: Vec<bool> = v.drain(1..4).map(|c| c.get()).collect();
    assert_eq!(drained, vec![false, true, false]);
    assert_eq!(v.len(), 3);
    assert!(v.get(0).unwrap().get());
    assert!(v.get(1).unwrap().get());
    assert!(!v.get(2).unwrap().get());
}

#[test]
fn miri_splice_shrink_and_grow() {
    let mut v = bv(&[true, false, true, true, false]);
    let removed: Vec<bool> = v
        .splice(1..4, [Compact(false)])
        .into_iter()
        .map(|c| c.get())
        .collect();
    assert_eq!(removed, vec![false, true, true]);
    assert_eq!(v.len(), 3);
    let _ = v.splice(1..2, [Compact(true), Compact(false), Compact(true)]);
    assert_eq!(v.len(), 5);
}

#[test]
fn miri_split_and_swap() {
    let v = bv(&[true, false, true, false, true]);
    let (a, b) = v.as_slice().split_at(2);
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 3);
    if let Some((first, rest)) = v.as_slice().split_first() {
        assert!(first.get());
        assert_eq!(rest.len(), 4);
    }
    let mut v = bv(&[true, false, true, false, true]);
    let mut sm = v.as_mut_slice();
    sm.swap(0, 4);
    assert!(sm.get(0).unwrap().get());
    assert!(sm.get(4).unwrap().get());
    {
        let sm = v.as_mut_slice();
        let (mut left, mut right) = sm.split_at_mut(2);
        left.get_mut(0).unwrap().set(false);
        right.get_mut(0).unwrap().set(true);
    }
    {
        let sm = v.as_mut_slice();
        if let Some((mut f, r)) = sm.split_first_mut() {
            f.set(true);
            assert_eq!(r.len(), 4);
        }
    }
}

#[test]
fn miri_chunks_and_exact() {
    let v = bv(&[true, false, true, false, true]);
    let total: usize = v.as_slice().chunks(2).map(|c| c.len()).sum();
    assert_eq!(total, 5);
    let exact: usize = v.as_slice().chunks_exact(2).map(|c| c.len()).sum();
    assert_eq!(exact, 4);
    let mut g = v.clone();
    let mut counts = 0;
    for mut c in g.as_mut_slice().chunks_mut(2) {
        c.get_mut(0).unwrap().set(true);
        counts += c.len();
    }
    assert_eq!(counts, 5);
}

#[test]
fn miri_iter_mut_writeback() {
    let mut v = bv(&[true, false, true, false]);
    for mut r in v.as_mut_slice().iter_mut() {
        r.set(!r.get());
    }
    let got: Vec<bool> = v.as_slice().iter().map(|c| c.get()).collect();
    assert_eq!(got, vec![false, true, false, true]);
}

#[test]
fn miri_sort() {
    let mut v = bv(&[true, false, true, false, false, true]);
    v.as_mut_slice().sort();
    let got: Vec<bool> = v.as_slice().iter().map(|c| c.get()).collect();
    assert_eq!(got, vec![false, false, false, true, true, true]);
}

#[test]
fn miri_binary_search() {
    let v = bv(&[false, false, true, true]);
    let i = v
        .as_slice()
        .binary_search_by(|c| c.get().cmp(&true))
        .unwrap();
    assert_eq!(i, 2);
}

#[test]
fn miri_unchecked_and_ptr_ops() {
    let mut v = bv(&[true, false, true]);
    unsafe {
        let val = v.as_slice().get_unchecked(1);
        assert!(!val.get());
    }
    {
        let mut sm = v.as_mut_slice();
        unsafe {
            sm.get_unchecked_mut(2).set(false);
        }
    }
    let p = v.as_ptr();
    unsafe {
        let p1 = p.add(1);
        let r = p1.read();
        assert!(!r.get());
    }
    unsafe {
        let pm = v.as_mut_ptr();
        pm.add(0).write(Compact(false));
    }
    assert!(!v.get(0).unwrap().get());
}

#[test]
fn miri_append_split_off_clone_to_vec() {
    let mut a = bv(&[true, false]);
    let mut b = bv(&[true, true, false]);
    a.append(&mut b);
    assert_eq!(a.len(), 5);
    assert!(b.is_empty());
    let c = a.split_off(3);
    assert_eq!(c.len(), 2);
    let cloned = a.clone();
    assert_eq!(cloned.len(), a.len());
    let vec = a.as_slice().to_vec();
    assert_eq!(vec.len(), a.len());
}

#[test]
fn miri_direct_mode_handle() {
    // Exercise the direct (owned) backing mode of CompactRefMut.
    let mut c = Compact::new(true);
    assert!(c.as_mut().get());
    c.as_mut().set(false);
    assert!(!c.as_mut().get());
}

#[repr(u8)]
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, layout::CompactRepr,
)]
enum Kind {
    A,
    B,
    C,
    D,
}

fn kv(xs: &[Kind]) -> CompactVec<Kind> {
    let mut v = CompactVec::new();
    for &x in xs {
        v.push(Compact(x));
    }
    v
}

#[test]
fn miri_enum_sort_and_iter_mut() {
    let mut v = kv(&[Kind::C, Kind::A, Kind::D, Kind::A, Kind::B]);
    v.as_mut_slice().sort_by(|a, b| a.get().cmp(&b.get()));
    let got: Vec<Kind> = v.as_slice().iter().map(|c| c.get()).collect();
    assert_eq!(got, vec![Kind::A, Kind::A, Kind::B, Kind::C, Kind::D]);
    for mut r in v.as_mut_slice().iter_mut() {
        let cur = r.get();
        r.set(cur);
    }
}

#[test]
fn miri_enum_splice_drain() {
    let mut v = kv(&[Kind::A, Kind::B, Kind::C, Kind::D]);
    let _ =
        v.splice(1..3, [Compact(Kind::A), Compact(Kind::A), Compact(Kind::B)]);
    assert_eq!(v.len(), 5);
    let drained: Vec<Kind> = v.drain(0..2).map(|c| c.get()).collect();
    assert_eq!(drained.len(), 2);
}

#[test]
fn miri_empty_slice_default() {
    // The dangling-pointer-backed empty-slice Default.
    let s = <layout::CompactSlice<'_, bool>>::default();
    assert!(s.is_empty());
    let sm = <layout::CompactSliceMut<'_, bool>>::default();
    assert!(sm.is_empty());
}

#[test]
fn miri_count_bool() {
    let v = bv(&[true, false, true, true, false, true]);
    assert_eq!(v.count(true), 4);
    assert_eq!(v.count(false), 2);
    assert_eq!(v.as_slice().slice(0..3).count(true), 2);
    assert_eq!(v.as_slice().slice(0..3).count(false), 1);
    assert_eq!(v.as_slice().count(true), 4);
}

#[test]
fn miri_count_enum() {
    let v = kv(&[Kind::A, Kind::B, Kind::A, Kind::C, Kind::A]);
    assert_eq!(v.count(Kind::A), 3);
    assert_eq!(v.count(Kind::B), 1);
    assert_eq!(v.count(Kind::C), 1);
    assert_eq!(v.as_slice().slice(0..4).count(Kind::A), 2);
}

// ---------------------------------------------------------------------------
// Regression: CompactVec::swap_remove must not write out of bounds when
// removing the last element. After pop(), `index == inner.len()`; the old guard
// `index < inner.len() + 1` was always true here, so `set(index, ...)` wrote one
// past the logical end (debug panic, release UB). The fix guards on
// `index < inner.len()`.
// ---------------------------------------------------------------------------

#[test]
fn miri_swap_remove_last_element_bool() {
    // Removing the last element: the swap-into-place step must be skipped.
    let mut v = bv(&[true, false, true]); // indices 0,1,2; remove 2 (last)
    let removed = v.swap_remove(2);
    assert!(removed.get());
    assert_eq!(v.len(), 2);
    assert!(v.get(0).unwrap().get());
    assert!(!v.get(1).unwrap().get());
}

#[test]
fn miri_swap_remove_only_element_bool() {
    // Removing the sole element: index 0 is also the last.
    let mut v = bv(&[true]);
    let removed = v.swap_remove(0);
    assert!(removed.get());
    assert!(v.is_empty());
}

#[test]
fn miri_swap_remove_middle_still_swaps_bool() {
    // Regression for the normal path: removing a middle element must still
    // move the last element into the freed slot.
    let mut v = bv(&[true, false, true]); // A=true, B=false, C=true; remove 0
    let removed = v.swap_remove(0);
    assert!(removed.get()); // A
    assert_eq!(v.len(), 2);
    assert!(v.get(0).unwrap().get()); // C moved into slot 0
    assert!(!v.get(1).unwrap().get()); // B untouched
}

#[test]
fn miri_swap_remove_last_element_enum() {
    // Same bug via the 2-bit enum column path.
    let mut v = kv(&[Kind::A, Kind::B, Kind::C]); // remove 2 (last)
    let removed = v.swap_remove(2);
    assert_eq!(removed.get(), Kind::C);
    assert_eq!(v.len(), 2);
    assert_eq!(v.get(0).unwrap().get(), Kind::A);
    assert_eq!(v.get(1).unwrap().get(), Kind::B);
}

// ---------------------------------------------------------------------------
// Regression: CompactSlice::count must not dereference the dangling pointer of
// a `Default`-constructed (empty) slice. Every other accessor guards on
// `is_empty()` first; `count` did not, which is UB (caught by Miri).
// ---------------------------------------------------------------------------

#[test]
fn miri_count_on_default_empty_slice_bool() {
    let s = <layout::CompactSlice<'_, bool>>::default();
    assert!(s.is_empty());
    assert_eq!(s.count(true), 0);
    assert_eq!(s.count(false), 0);
}

#[test]
fn miri_count_on_default_empty_slice_enum() {
    let s = <layout::CompactSlice<'_, Kind>>::default();
    assert!(s.is_empty());
    assert_eq!(s.count(Kind::A), 0);
    assert_eq!(s.count(Kind::D), 0);
}
