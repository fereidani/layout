//! Out-of-line panic helpers for the generated code.
//!
//! A generated `Vec`/slice checks one shared length and then reads every
//! column unchecked, so each bounds failure has exactly one reporting site.
//! Formatting a message inline at that site would pull `core::fmt` into the
//! caller and duplicate the whole panic path in every generated type; these
//! helpers are `#[cold]` and `#[inline(never)]`, so the failure path is a
//! single call and the hot path keeps only its compare.

/// Report an out-of-bounds element index.
#[cold]
#[inline(never)]
#[track_caller]
pub fn index_out_of_bounds(index: usize, len: usize) -> ! {
    panic!(
        "index out of bounds: the len is {} but the index is {}",
        len, index
    )
}

/// Report a range whose start is past its end.
#[cold]
#[inline(never)]
#[track_caller]
pub fn slice_index_order_fail(start: usize, end: usize) -> ! {
    panic!("slice index starts at {} but ends at {}", start, end)
}

/// Report a range end past the length.
#[cold]
#[inline(never)]
#[track_caller]
pub fn slice_end_index_len_fail(end: usize, len: usize) -> ! {
    panic!(
        "range end index {} out of range for slice of length {}",
        end, len
    )
}

/// Report an out-of-bounds insertion index.
#[cold]
#[inline(never)]
#[track_caller]
pub fn insert_index_fail(index: usize, len: usize) -> ! {
    panic!(
        "insertion index (is {}) should be <= len (is {})",
        index, len
    )
}
