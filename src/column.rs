//! Owning storage for a plain struct-of-arrays column.
//!
//! A generated `FooVec` stores each plain field as a [`Column<T>`] rather than
//! a raw `Vec<T>`. `Column<T>` dereferences to `[T]`, so all read and in-place
//! element operations (indexing, `iter`, `iter_mut`, `get`, slicing, ...) work
//! exactly as on a `Vec`/slice. What it deliberately does **not** expose is any
//! safe way to change the column's length.
//!
//! The soundness of a struct-of-arrays `Vec` relies on every column sharing one
//! length: [`FooVec::get`](crate::SoAIndex) bounds-checks against a single
//! length and then reads each column, so a shorter column would be read out of
//! bounds. The only safe way to grow or shrink is therefore the composite
//! `FooVec` API (`push`/`insert`/`remove`/...), which updates every column
//! together. The per-column length operations below are `unsafe`: the generated
//! composite methods call them inside `unsafe` blocks (where the equal-length
//! invariant is preserved), and any other caller would have to write `unsafe`
//! themselves to desynchronize the columns.

use alloc::vec::Vec;
use core::ops::{Deref, DerefMut, RangeBounds};

/// Owning, length-locked storage for one plain SOA column.
///
/// See the [module docs](self) for why length mutation is gated behind
/// `unsafe`.
///
/// `repr(transparent)` makes the layout-identical-to-`Vec<T>` guarantee
/// explicit: the wrapper adds no size, alignment, or field-offset overhead.
#[repr(transparent)]
pub struct Column<T> {
    inner: Vec<T>,
}

impl<T> Column<T> {
    /// Build a column with capacity for `capacity` elements (empty length).
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Column {
            inner: Vec::with_capacity(capacity),
        }
    }

    /// Wrap an existing `Vec<T>` as a column.
    #[inline]
    pub fn from_vec(inner: Vec<T>) -> Self {
        Column { inner }
    }

    /// Reassemble a `Column` from its raw parts, like
    /// [`Vec::from_raw_parts`](alloc::vec::Vec::from_raw_parts).
    ///
    /// # Safety
    ///
    /// The caller must uphold the same contract as
    /// [`Vec::from_raw_parts`](alloc::vec::Vec::from_raw_parts).
    #[inline]
    pub unsafe fn from_raw_parts(
        ptr: *mut T,
        length: usize,
        capacity: usize,
    ) -> Self {
        // SAFETY: forwarded to `Vec::from_raw_parts`; the caller upholds its
        // contract.
        Column {
            inner: unsafe { Vec::from_raw_parts(ptr, length, capacity) },
        }
    }

    /// Total capacity (forwards to [`Vec::capacity`]).
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Reserve capacity (forwards to [`Vec::reserve`]).
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }

    /// Reserve the minimal capacity (forwards to [`Vec::reserve_exact`]).
    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) {
        self.inner.reserve_exact(additional);
    }

    /// Shrink allocated capacity to fit the length (forwards to
    /// [`Vec::shrink_to_fit`]).
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.inner.shrink_to_fit();
    }

    /// Borrow the contents as a slice (forwards to [`Vec::as_slice`]).
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        self.inner.as_slice()
    }

    /// Borrow the contents as a mutable slice (forwards to
    /// [`Vec::as_mut_slice`]).
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.inner.as_mut_slice()
    }

    /// Underlying read pointer (forwards to [`Vec::as_ptr`]).
    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.inner.as_ptr()
    }

    /// Underlying write pointer (forwards to [`Vec::as_mut_ptr`]).
    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.inner.as_mut_ptr()
    }

    // ----- length-mutating operations: `unsafe` by design (see module docs) --

    /// Append an element (forwards to [`Vec::push`]).
    ///
    /// # Safety
    ///
    /// In a struct-of-arrays `Vec`, the caller must push to every sibling
    /// column so they stay the same length. The generated composite `push`
    /// satisfies this; calling this directly on one column desynchronizes the
    /// columns and turns a later `get` into an out-of-bounds access.
    #[inline]
    pub unsafe fn push(&mut self, value: T) {
        self.inner.push(value);
    }

    /// Remove and return the last element (forwards to [`Vec::pop`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must pop every sibling column.
    #[inline]
    pub unsafe fn pop(&mut self) -> Option<T> {
        self.inner.pop()
    }

    /// Insert an element (forwards to [`Vec::insert`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must insert into every sibling column.
    #[inline]
    pub unsafe fn insert(&mut self, index: usize, element: T) {
        self.inner.insert(index, element);
    }

    /// Remove and return an element, shifting the tail (forwards to
    /// [`Vec::remove`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must remove from every sibling column.
    #[inline]
    pub unsafe fn remove(&mut self, index: usize) -> T {
        self.inner.remove(index)
    }

    /// Remove and return an element, swapping in the last (forwards to
    /// [`Vec::swap_remove`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must `swap_remove` from every sibling
    /// column.
    #[inline]
    pub unsafe fn swap_remove(&mut self, index: usize) -> T {
        self.inner.swap_remove(index)
    }

    /// Shorten to `len`, dropping the tail (forwards to [`Vec::truncate`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must truncate every sibling column to
    /// the same length.
    #[inline]
    pub unsafe fn truncate(&mut self, len: usize) {
        self.inner.truncate(len);
    }

    /// Resize to `new_len`, filling new slots with `value` (forwards to
    /// [`Vec::resize`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must resize every sibling column to the
    /// same length.
    #[inline]
    pub unsafe fn resize(&mut self, new_len: usize, value: T)
    where
        T: Clone,
    {
        self.inner.resize(new_len, value);
    }

    /// Clear all elements (forwards to [`Vec::clear`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must clear every sibling column.
    #[inline]
    pub unsafe fn clear(&mut self) {
        self.inner.clear();
    }

    /// Split off the tail into a new `Column` (forwards to [`Vec::split_off`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must split every sibling column at the
    /// same index.
    #[inline]
    pub unsafe fn split_off(&mut self, at: usize) -> Column<T> {
        Column {
            inner: self.inner.split_off(at),
        }
    }

    /// Drain a range into an iterator (forwards to [`Vec::drain`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must drain the same range from every
    /// sibling column.
    #[inline]
    pub unsafe fn drain<R>(&mut self, range: R) -> alloc::vec::Drain<'_, T>
    where
        R: RangeBounds<usize>,
    {
        self.inner.drain(range)
    }

    /// Append a slice (forwards to [`Vec::extend_from_slice`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must extend every sibling column with
    /// the corresponding slice.
    #[inline]
    pub unsafe fn extend_from_slice(&mut self, other: &[T])
    where
        T: Clone,
    {
        self.inner.extend_from_slice(other);
    }

    /// Move all elements out of `other` (forwards to [`Vec::append`]).
    ///
    /// # Safety
    ///
    /// See [`Column::push`]: the caller must append every sibling column.
    #[inline]
    pub unsafe fn append(&mut self, other: &mut Column<T>) {
        self.inner.append(&mut other.inner);
    }
}

impl<T> Deref for Column<T> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &[T] {
        &self.inner
    }
}

impl<T> DerefMut for Column<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.inner
    }
}

impl<'a, T> IntoIterator for &'a Column<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Column<T> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}

impl<T> Default for Column<T> {
    #[inline]
    fn default() -> Self {
        Column { inner: Vec::new() }
    }
}

impl<T: Clone> Clone for Column<T> {
    #[inline]
    fn clone(&self) -> Self {
        Column {
            inner: self.inner.clone(),
        }
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for Column<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.inner, f)
    }
}

impl<T: PartialEq> PartialEq for Column<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<T: Eq> Eq for Column<T> {}

impl<T: PartialOrd> PartialOrd for Column<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<T: Ord> Ord for Column<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl<T: core::hash::Hash> core::hash::Hash for Column<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state);
    }
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for Column<T> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        self.inner.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Column<T> {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        Vec::deserialize(deserializer).map(|inner| Column { inner })
    }
}
