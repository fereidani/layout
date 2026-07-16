//! Out-of-bounds access on compact columns must panic in release builds, not
//! just under `debug_assert!`. The inherent `index`/`index_mut`/`set`/`remove`
//! methods were hardened first; this file additionally covers the
//! `SoAIndex(Mut)` trait path that the generated `Vec::index_mut` /
//! `SliceMut::index_mut` dispatch through, plus range-based slicing and drain.
//! Run with `cargo test --release --test compact_index_bounds` to exercise the
//! release-only path.
use layout::{Compact, CompactBool, CompactVec, SOA};

#[derive(Clone, SOA)]
struct Row {
    id: u32,
    flag: CompactBool,
}

fn one() -> RowVec {
    let mut v = RowVec::new();
    v.push(Row {
        id: 1,
        flag: Compact(true),
    });
    v
}

// --- inherent methods (CompactVec / CompactSlice(Mut)) ---

#[test]
#[should_panic]
fn set_oob_panics() {
    let mut v = one();
    v.flag.set(3, Compact(false));
}

#[test]
#[should_panic]
fn slice_index_oob_panics() {
    let v = one();
    let _ = v.flag.as_slice().index(3);
}

#[test]
#[should_panic]
fn slice_index_mut_oob_panics() {
    let mut v = one();
    v.flag.as_mut_slice().index_mut(3).set(false);
}

// --- trait path: generated Vec::index_mut / SliceMut::index_mut route compact
//     columns through `<usize as SoAIndexMut<CompactSliceMut>>::index_mut`,
//     which previously only `debug_assert!`ed. ---

#[test]
#[should_panic]
fn vec_index_mut_oob_panics() {
    let mut v = one();
    v.index_mut(50).flag.set(false);
}

#[test]
#[should_panic]
fn slice_mut_index_mut_oob_panics() {
    let mut v = one();
    v.as_mut_slice().index_mut(50).flag.set(false);
}

// --- range slicing (CompactVec::slice / slice_mut / drain) ---

#[test]
#[should_panic]
fn slice_oob_range_panics() {
    let v = one();
    let _ = v.flag.slice(0..5);
}

#[test]
#[should_panic]
fn slice_mut_oob_range_panics() {
    let mut v = one();
    let _ = v.flag.slice_mut(0..5);
}

#[test]
#[should_panic]
fn drain_oob_range_panics() {
    let mut v = one();
    let _ = v.flag.drain(0..5);
}

// `CompactVec::splice` must reject an out-of-bounds range up front, the way
// `drain` does. Without an explicit check it instead reads stale packed lanes
// and panics deep in the shift arithmetic (an `index out of bounds` /
// `subtract with overflow` from the low-level word ops), not with a clean
// range-validation message.
#[test]
#[should_panic(expected = "splice range out of bounds")]
fn compactvec_splice_oob_range_is_rejected() {
    let mut v = CompactVec::<bool>::new();
    v.push(Compact(true)); // len == 1
    let _: Vec<Compact<bool>> = v.splice(0..5, core::iter::empty::<Compact<bool>>());
}
