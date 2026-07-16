//! Compact (bit-packed) columns for struct-of-arrays layouts.
//!
//! [`Compact<T>`] is the owning value wrapper that marks a field for
//! compaction. When a struct containing a `Compact<T>` field is derived with
//! `#[derive(SOA)]`, that column is stored bit-packed (one `T` per
//! [`CompactRepr::BITS`] bits) instead of as a `Vec<T>`.
//!
//! `T` must implement [`CompactRepr`]. This is implemented for `bool` (1 bit)
//! and, via `#[derive(CompactRepr)]`, for any fieldless enum with an unsigned
//! `#[repr(uN)]` repr (2 / 4 / 8 / 16 bits depending on the largest
//! discriminant).
//!
//! # Access
//!
//! Access is uniform across the owning value, the immutable generated `Ref`
//! and the mutable generated `RefMut`, so the same expression works in a
//! `#[soa_impl]` method body whether it is invoked on the owned struct, the
//! generated `Ref` or the generated `RefMut`:
//!
//! * **read**: `self.flag.get()`
//! * **write** (mutable only): `self.flag.set(value)`
//!
//! No dereference or write-back-on-drop is involved: a mutable handle writes
//! the packed word immediately.

use alloc::{vec, vec::Vec};
use core::{marker::PhantomData, ops::Range};

use crate::bitpack::BitPack;

/// Bit-packed backing store type backing a `T` compact column.
type Store<T> = <T as CompactRepr>::Storage;

/// Compact representation: how a `Copy` value is encoded into and decoded
/// from a small unsigned integer, and which [`BitPack`] storage backs it.
///
/// Implemented for `bool` (1 bit) and, via `#[derive(CompactRepr)]`, for
/// fieldless enums. Implementors (and their storage) must be `'static` so that
/// borrowed column views of any lifetime are well-formed.
pub trait CompactRepr: Copy + Sized + 'static {
    /// The bit-packed storage backing a column of this type. Each impl picks
    /// a concrete `PackedArray<N>`.
    type Storage: BitPack + 'static;

    /// Number of bits used per element (`1` for `bool`; `2`/`4`/`8`/`16` for
    /// enums).
    const BITS: u32;

    /// Encode `self` into the raw integer stored in the packed words.
    fn encode(self) -> usize;

    /// Decode a raw integer read from the packed words back into a value.
    ///
    /// # Safety contract (implementors)
    /// `raw` is always a value previously produced by [`encode`](Self::encode),
    /// since the compact column never stores anything else.
    fn decode(raw: usize) -> Self;
}

impl CompactRepr for bool {
    type Storage = crate::bitpack::PackedArray<1>;
    const BITS: u32 = 1;

    #[inline(always)]
    fn encode(self) -> usize {
        self as usize
    }

    #[inline(always)]
    fn decode(raw: usize) -> Self {
        raw != 0
    }
}

// ---------------------------------------------------------------------------
// Compact<T> - owning value
// ---------------------------------------------------------------------------

/// Owning compact value. Marks a struct field for bit-packed storage when the
/// containing struct is derived with `#[derive(SOA)]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Compact<T: CompactRepr>(pub T);

/// 1-bit compact boolean, for backwards-compatible naming.
pub type CompactBool = Compact<bool>;

impl<T: CompactRepr> Compact<T> {
    /// Wrap a value for compact storage.
    #[inline(always)]
    pub fn new(value: T) -> Self {
        Compact(value)
    }

    /// Read the contained value.
    #[inline(always)]
    pub fn get(&self) -> T {
        self.0
    }

    /// Overwrite the contained value.
    #[inline(always)]
    pub fn set(&mut self, value: T) {
        self.0 = value;
    }

    /// Build a mutable handle to this value (used by the generated `RefMut`
    /// construction). Writes through the handle propagate back here
    /// immediately via [`CompactRefMut::set`].
    #[inline(always)]
    pub fn as_mut(&mut self) -> CompactRefMut<'_, T> {
        CompactRefMut::from_value(&mut self.0)
    }

    /// Returns a snapshot pointer carrying this value inline. Immutable
    /// access yields a value snapshot with no backing storage, so there is no
    /// raw address to expose; carrying the value keeps the pointer non-null
    /// and recoverable via `as_ref`/`read`, so an element pointer derived
    /// from a `Ref` is never misclassified as null.
    #[inline(always)]
    pub fn as_ptr(&self) -> CompactPtr<T> {
        CompactPtr {
            packed: core::ptr::null(),
            index: 0,
            value: Some(self.0),
        }
    }
}

impl<T: CompactRepr> From<T> for Compact<T> {
    #[inline(always)]
    fn from(value: T) -> Self {
        Compact(value)
    }
}

#[cfg(feature = "serde")]
impl<T: CompactRepr + serde::Serialize> serde::Serialize for Compact<T> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de, T: CompactRepr + serde::Deserialize<'de>> serde::Deserialize<'de>
    for Compact<T>
{
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Compact)
    }
}

// ---------------------------------------------------------------------------
// CompactRefMut - read/write handle to a single packed (or owned) element.
// No Deref, no Drop: get() reads live, set() writes immediately.
// ---------------------------------------------------------------------------

pub struct CompactRefMut<'a, T: CompactRepr> {
    // Exactly one of `direct` / `packed` is non-null.
    direct: *mut T,
    packed: *mut Store<T>,
    index: usize,
    _marker: PhantomData<&'a mut ()>,
}

impl<'a, T: CompactRepr> CompactRefMut<'a, T> {
    #[inline(always)]
    pub(crate) fn from_packed(packed: &'a mut Store<T>, index: usize) -> Self {
        Self {
            direct: core::ptr::null_mut(),
            packed: packed as *mut Store<T>,
            index,
            _marker: PhantomData,
        }
    }

    /// Construct a handle from a raw pointer to packed storage.
    ///
    /// # Safety
    /// `packed` must point to a live, properly aligned `Store<T>` whose length
    /// exceeds `index` for the handle's lifetime; the resulting handle must not
    /// outlive that storage.
    #[inline(always)]
    pub(crate) unsafe fn from_packed_ptr(
        packed: *mut Store<T>,
        index: usize,
    ) -> Self {
        Self {
            direct: core::ptr::null_mut(),
            packed,
            index,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    fn from_value(value: &'a mut T) -> Self {
        Self {
            direct: value as *mut T,
            packed: core::ptr::null_mut(),
            index: 0,
            _marker: PhantomData,
        }
    }

    /// Read the current value live from the backing storage.
    #[inline]
    pub fn get(&self) -> T {
        // The packed (column) path is the overwhelmingly common one; the
        // direct path only fires for an owned value viewed as a RefMut.
        if ::branches::likely(!self.packed.is_null()) {
            // SAFETY: `packed` aliases borrowed storage that is still live;
            // `index` is in bounds by construction.
            unsafe { T::decode((*self.packed).get(self.index)) }
        } else {
            // SAFETY: `direct` aliases the borrowed `&mut T` which is still
            // live.
            unsafe { *self.direct }
        }
    }

    /// Write `value` to the backing storage immediately.
    #[inline]
    pub fn set(&mut self, value: T) {
        if ::branches::likely(!self.packed.is_null()) {
            // SAFETY: as above.
            unsafe {
                (*self.packed).set(self.index, T::encode(value));
            }
        } else {
            // SAFETY: as above.
            unsafe {
                *self.direct = value;
            }
        }
    }

    #[inline]
    pub fn to_owned(&self) -> Compact<T> {
        Compact::new(self.get())
    }

    pub fn replace(&mut self, val: Compact<T>) -> Compact<T> {
        let old = Compact::new(self.get());
        self.set(val.0);
        old
    }

    pub fn as_ptr(&self) -> CompactPtr<T> {
        if self.packed.is_null() {
            // Direct/owned mode: no backing storage to point into, so carry
            // a snapshot of the current value instead of going null.
            CompactPtr {
                packed: core::ptr::null(),
                index: 0,
                value: Some(self.get()),
            }
        } else {
            CompactPtr {
                packed: self.packed as *const Store<T>,
                index: self.index,
                value: None,
            }
        }
    }

    pub fn as_mut_ptr(&mut self) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed,
            index: self.index,
        }
    }
}

impl<T: CompactRepr + core::fmt::Debug> core::fmt::Debug
    for CompactRefMut<'_, T>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CompactRefMut")
            .field("value", &self.get())
            .finish()
    }
}

impl<'a, T: CompactRepr + PartialEq> PartialEq for CompactRefMut<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl<'a, T: CompactRepr + Eq> Eq for CompactRefMut<'a, T> {}

impl<'a, T: CompactRepr + core::hash::Hash> core::hash::Hash
    for CompactRefMut<'a, T>
{
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.get().hash(state);
    }
}

impl<'a, T: CompactRepr> From<CompactRefMut<'a, T>> for Compact<T> {
    #[inline]
    fn from(value: CompactRefMut<'a, T>) -> Self {
        value.to_owned()
    }
}

impl<'a, T: CompactRepr> From<&'a CompactRefMut<'a, T>> for Compact<T> {
    #[inline]
    fn from(value: &'a CompactRefMut<'a, T>) -> Self {
        value.to_owned()
    }
}

// ---------------------------------------------------------------------------
// Trait wiring: Compact<T> behaves as a nested SoA column.
// ---------------------------------------------------------------------------

impl<T: CompactRepr> crate::SOA for Compact<T> {
    type Type = CompactVec<T>;
}

impl<'a, T: CompactRepr> crate::SoAIter<'a> for Compact<T> {
    type Ref = Compact<T>;
    type RefMut = CompactRefMut<'a, T>;
    type Iter = CompactIter<'a, T>;
    type IterMut = CompactIterMut<'a, T>;
}

impl<T: CompactRepr> crate::SoAPointers for Compact<T> {
    type Ptr = CompactPtr<T>;
    type MutPtr = CompactPtrMut<T>;
}

// ---------------------------------------------------------------------------
// CompactVec
// ---------------------------------------------------------------------------

pub struct CompactVec<T: CompactRepr> {
    inner: Store<T>,
}

impl<T: CompactRepr> Default for CompactVec<T> {
    #[inline]
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

impl<T: CompactRepr + core::fmt::Debug> core::fmt::Debug for CompactVec<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list()
            .entries(
                (0..self.len()).map(|i| Compact(T::decode(self.inner.get(i)))),
            )
            .finish()
    }
}

impl<T: CompactRepr> Clone for CompactVec<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: CompactRepr + PartialEq> PartialEq for CompactVec<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && (0..self.len()).all(|i| {
                T::decode(self.inner.get(i)) == T::decode(other.inner.get(i))
            })
    }
}

impl<T: CompactRepr + Eq> Eq for CompactVec<T> {}

impl<T: CompactRepr + core::hash::Hash> core::hash::Hash for CompactVec<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for i in 0..self.len() {
            T::decode(self.inner.get(i)).hash(state);
        }
    }
}

impl<T: CompactRepr> core::iter::FromIterator<Compact<T>> for CompactVec<T> {
    fn from_iter<I: IntoIterator<Item = Compact<T>>>(iter: I) -> Self {
        let iterator = iter.into_iter();
        let capacity = iterator.size_hint().1.unwrap_or(0);
        let mut result = CompactVec::<T>::with_capacity(capacity);
        for item in iterator {
            result.push(item);
        }
        result
    }
}

impl<T: CompactRepr> core::iter::Extend<Compact<T>> for CompactVec<T> {
    fn extend<I: IntoIterator<Item = Compact<T>>>(&mut self, iter: I) {
        for item in iter {
            self.push(item);
        }
    }
}

#[allow(dead_code)]
impl<T: CompactRepr> CompactVec<T> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Store::<T>::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional);
    }

    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) {
        self.inner.reserve_exact(additional);
    }

    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.inner.shrink_to_fit();
    }

    #[inline]
    pub fn push(&mut self, value: Compact<T>) {
        self.inner.push(T::encode(value.0));
    }

    #[inline]
    pub fn pop(&mut self) -> Option<Compact<T>> {
        self.inner.pop().map(|w| Compact(T::decode(w)))
    }

    pub fn insert(&mut self, index: usize, element: Compact<T>) {
        assert!(
            index <= self.len(),
            "insertion index (is {}) should be <= len (is {})",
            index,
            self.len()
        );
        self.push(Compact::new(element.0));
        for i in (index + 1..self.len()).rev() {
            let v = self.inner.get(i - 1);
            self.inner.set(i, v);
        }
        self.inner.set(index, T::encode(element.0));
    }

    pub fn remove(&mut self, index: usize) -> Compact<T> {
        assert!(
            index < self.len(),
            "index out of bounds: the len is {} but the index is {}",
            self.len(),
            index
        );
        let val = Compact(T::decode(self.inner.get(index)));
        for i in index..self.len() - 1 {
            let v = self.inner.get(i + 1);
            self.inner.set(i, v);
        }
        self.inner.pop();
        val
    }

    pub fn swap_remove(&mut self, index: usize) -> Compact<T> {
        assert!(
            index < self.len(),
            "index out of bounds: the len is {} but the index is {}",
            self.len(),
            index
        );
        let val = Compact(T::decode(self.inner.get(index)));
        let last = match self.inner.pop() {
            Some(v) => v,
            None => return val,
        };
        if index < self.inner.len() {
            self.inner.set(index, last);
        }
        val
    }

    pub fn replace(&mut self, index: usize, element: Compact<T>) -> Compact<T> {
        assert!(
            index < self.len(),
            "index out of bounds: the len is {} but the index is {}",
            self.len(),
            index
        );
        let old = Compact(T::decode(self.inner.get(index)));
        self.inner.set(index, T::encode(element.0));
        old
    }

    /// Set the element at `index` to `value`.
    ///
    /// # Panics
    ///
    /// Panics if `index >= self.len()`.
    #[inline]
    pub fn set(&mut self, index: usize, value: Compact<T>) {
        assert!(
            index < self.len(),
            "index out of bounds: the len is {} but the index is {}",
            self.len(),
            index
        );
        self.inner.set(index, T::encode(value.0));
    }

    #[inline]
    pub fn truncate(&mut self, len: usize) {
        self.inner.truncate(len);
    }

    /// Resizes the column to `new_len`, pushing copies of `value` to grow or
    /// truncating to shrink (analogous to [`Vec::resize`]).
    pub fn resize(&mut self, new_len: usize, value: Compact<T>) {
        let cur = self.inner.len();
        if new_len <= cur {
            self.inner.truncate(new_len);
        } else {
            let encoded = T::encode(value.0);
            self.inner.reserve(new_len - cur);
            for _ in cur..new_len {
                self.inner.push(encoded);
            }
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub fn split_off(&mut self, at: usize) -> CompactVec<T> {
        assert!(
            at <= self.len(),
            "the len is {} but the index is {}",
            self.len(),
            at
        );
        let mut other = Store::<T>::with_capacity(self.len() - at);
        for i in at..self.inner.len() {
            other.push(self.inner.get(i));
        }
        self.inner.truncate(at);
        CompactVec { inner: other }
    }

    pub fn append(&mut self, other: &mut CompactVec<T>) {
        self.inner.append(&mut other.inner);
    }

    /// Append every element of `other` to the end (analogous to
    /// [`Vec::extend_from_slice`]). The generated `#[layout(Clone)]`
    /// `extend_from_slice` dispatches here for compact columns.
    pub fn extend_from_slice(&mut self, other: CompactSlice<'_, T>) {
        let n = other.len();
        self.inner.reserve(n);
        for val in other {
            self.inner.push(T::encode(val.0));
        }
    }

    pub fn as_slice(&self) -> CompactSlice<'_, T> {
        CompactSlice {
            packed: &self.inner as *const Store<T>,
            start: 0,
            len: self.inner.len(),
            _marker: PhantomData,
        }
    }

    pub fn as_mut_slice(&mut self) -> CompactSliceMut<'_, T> {
        let len = self.inner.len();
        CompactSliceMut {
            packed: &mut self.inner as *mut Store<T>,
            start: 0,
            len,
            _marker: PhantomData,
        }
    }

    /// Iterate by value over the column (analogous to `Vec::iter`, yielding
    /// `Compact<T>` snapshots). Also lets `soa_zip!` zip compact columns and
    /// `for x in &compact_vec` compile.
    #[inline]
    pub fn iter(&self) -> CompactIter<'_, T> {
        CompactIter {
            packed: &self.inner as *const Store<T>,
            pos: 0,
            end: self.inner.len(),
            _marker: PhantomData,
        }
    }

    /// Iterate by mutable handle over the column (analogous to
    /// `Vec::iter_mut`).
    #[inline]
    pub fn iter_mut(&mut self) -> CompactIterMut<'_, T> {
        CompactIterMut {
            packed: &mut self.inner as *mut Store<T>,
            pos: 0,
            end: self.inner.len(),
            _marker: PhantomData,
        }
    }

    pub fn slice(&self, range: Range<usize>) -> CompactSlice<'_, T> {
        assert!(range.start <= range.end && range.end <= self.len());
        CompactSlice {
            packed: &self.inner as *const Store<T>,
            start: range.start,
            len: range.end - range.start,
            _marker: PhantomData,
        }
    }

    pub fn slice_mut(&mut self, range: Range<usize>) -> CompactSliceMut<'_, T> {
        assert!(range.start <= range.end && range.end <= self.len());
        CompactSliceMut {
            packed: &mut self.inner as *mut Store<T>,
            start: range.start,
            len: range.end - range.start,
            _marker: PhantomData,
        }
    }

    pub fn get(&self, index: usize) -> Option<Compact<T>> {
        if index < self.inner.len() {
            Some(Compact(T::decode(self.inner.get(index))))
        } else {
            None
        }
    }

    /// Count elements equal to `value`. The value is encoded once and searched
    /// for across the packed words; for 1-bit `T` (`bool` and 1-bit enums) this
    /// uses `count_ones`/`count_zeros` over the words.
    #[inline]
    pub fn count(&self, value: T) -> usize {
        self.inner.count_in(0, self.inner.len(), T::encode(value))
    }

    pub fn get_mut(&mut self, index: usize) -> Option<CompactRefMut<'_, T>> {
        if index < self.inner.len() {
            Some(CompactRefMut::from_packed(&mut self.inner, index))
        } else {
            None
        }
    }

    pub fn as_ptr(&self) -> CompactPtr<T> {
        CompactPtr {
            packed: &self.inner as *const Store<T>,
            index: 0,
            value: None,
        }
    }

    pub fn as_mut_ptr(&mut self) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: &mut self.inner as *mut Store<T>,
            index: 0,
        }
    }

    /// Reconstruct a [`CompactVec`] from the raw packed-storage pointer
    /// obtained from [`CompactVec::as_mut_ptr`].
    ///
    /// Unlike `Vec::from_raw_parts`, no `len`/`capacity` are required: the
    /// [`PackedArray`](crate::bitpack::PackedArray) (`data.packed`) carries its
    /// own element count and word-vector capacity.
    ///
    /// # Safety
    /// `data` must originate from [`CompactVec::as_mut_ptr`], the source
    /// [`CompactVec`] must have been forgotten (e.g. via [`core::mem::forget`])
    /// and its storage not reused or freed since, and `data.packed` must still
    /// point to valid, properly aligned `Store<T>` storage. This constructor
    /// moves that storage out of the (forgotten) source, so the source must
    /// never be used or dropped again.
    pub unsafe fn from_raw_parts(data: CompactPtrMut<T>) -> CompactVec<T> {
        CompactVec {
            // SAFETY: `data.packed` points to a live, owned, inline
            // `PackedArray` that the caller has given up (forgotten
            // the source). We move it out by value; the source must
            // not be dropped again.
            inner: unsafe { core::ptr::read(data.packed) },
        }
    }

    pub fn drain<R: core::ops::RangeBounds<usize>>(
        &mut self,
        range: R,
    ) -> CompactDrain<'_, T> {
        let start = match range.start_bound() {
            core::ops::Bound::Included(&i) => i,
            core::ops::Bound::Excluded(&i) => i + 1,
            core::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            core::ops::Bound::Included(&i) => i + 1,
            core::ops::Bound::Excluded(&i) => i,
            core::ops::Bound::Unbounded => self.inner.len(),
        };
        assert!(start <= end && end <= self.inner.len());
        CompactDrain {
            packed: &mut self.inner,
            drain_start: start,
            drain_end: end,
            pos: start,
            back: end,
        }
    }

    #[allow(clippy::forget_non_drop)]
    pub fn splice<R, I>(&mut self, range: R, replace_with: I) -> Vec<Compact<T>>
    where
        R: core::ops::RangeBounds<usize> + Clone,
        I: core::iter::IntoIterator<Item = Compact<T>>,
    {
        let start = match range.start_bound() {
            core::ops::Bound::Included(&i) => i,
            core::ops::Bound::Excluded(&i) => i + 1,
            core::ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            core::ops::Bound::Included(&i) => i + 1,
            core::ops::Bound::Excluded(&i) => i,
            core::ops::Bound::Unbounded => self.inner.len(),
        };
        assert!(
            start <= end && end <= self.inner.len(),
            "splice range out of bounds: the len is {} but the range is {}..{}",
            self.inner.len(),
            start,
            end
        );
        let mut removed = Vec::new();
        for i in start..end {
            removed.push(Compact(T::decode(self.inner.get(i))));
        }
        let remove_count = end - start;
        let replacement: Vec<Compact<T>> = replace_with.into_iter().collect();
        let insert_count = replacement.len();
        if insert_count < remove_count {
            for (i, val) in replacement.iter().enumerate() {
                self.inner.set(start + i, T::encode(val.0));
            }
            let shift_from = end;
            let shift_to = start + insert_count;
            let tail_len = self.inner.len() - shift_from;
            for i in 0..tail_len {
                let v = self.inner.get(shift_from + i);
                self.inner.set(shift_to + i, v);
            }
            for _ in 0..remove_count - insert_count {
                self.inner.pop();
            }
        } else {
            for _ in 0..insert_count - remove_count {
                self.inner.push(0);
            }
            let shift_from = start + remove_count;
            let shift_to = start + insert_count;
            let tail_len = self.inner.len() - shift_to;
            for i in (0..tail_len).rev() {
                let v = self.inner.get(shift_from + i);
                self.inner.set(shift_to + i, v);
            }
            for (i, val) in replacement.iter().enumerate() {
                self.inner.set(start + i, T::encode(val.0));
            }
        }
        // No `mem::forget` needed: `Compact<T>: Copy` (no `Drop`), and `encode`
        // copies `val.0`, so `replacement` owns intact values and drops
        // normally.
        removed
    }
}

impl<'a, T: CompactRepr> IntoIterator for &'a CompactVec<T> {
    type Item = Compact<T>;
    type IntoIter = CompactIter<'a, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: CompactRepr> IntoIterator for &'a mut CompactVec<T> {
    type Item = CompactRefMut<'a, T>;
    type IntoIter = CompactIterMut<'a, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(feature = "serde")]
impl<T: CompactRepr + serde::Serialize> serde::Serialize for CompactVec<T> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for val in self.iter() {
            seq.serialize_element(&val.0)?;
        }
        seq.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: CompactRepr + serde::Deserialize<'de>> serde::Deserialize<'de>
    for CompactVec<T>
{
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Self, D::Error> {
        use core::fmt;

        use serde::de::{SeqAccess, Visitor};

        struct CompactVecVisitor<T: CompactRepr>(PhantomData<T>);
        impl<'de, T: CompactRepr + serde::Deserialize<'de>> Visitor<'de>
            for CompactVecVisitor<T>
        {
            type Value = CompactVec<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a sequence of compact values")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut v = CompactVec::<T>::with_capacity(
                    seq.size_hint().unwrap_or(0),
                );
                while let Some(elem) = seq.next_element::<T>()? {
                    v.push(Compact(elem));
                }
                Ok(v)
            }
        }
        deserializer.deserialize_seq(CompactVecVisitor(PhantomData))
    }
}

// ---------------------------------------------------------------------------
// CompactSlice
// ---------------------------------------------------------------------------

#[derive(Copy, Clone)]
pub struct CompactSlice<'a, T: CompactRepr> {
    // Raw pointer (not a reference) so an empty `Default` slice can hold a
    // dangling pointer soundly with zero allocation. Reads go through
    // `unsafe { &*self.packed }` / `(*self.packed)` and are only performed
    // when `len > 0`, i.e. when the slice is backed by valid storage. The
    // lifetime is carried by `_marker` (mirrors CompactSliceMut).
    packed: *const Store<T>,
    start: usize,
    len: usize,
    _marker: PhantomData<&'a Store<T>>,
}

impl<'a, T: CompactRepr> Default for CompactSlice<'a, T> {
    fn default() -> Self {
        // Empty slice: `len == 0` guarantees the dangling storage is never
        // dereferenced. A dangling raw pointer (not a reference) is sound and
        // allocation-free.
        CompactSlice {
            packed: core::ptr::NonNull::<Store<T>>::dangling().as_ptr(),
            start: 0,
            len: 0,
            _marker: PhantomData,
        }
    }
}

#[allow(dead_code)]
impl<'a, T: CompactRepr> CompactSlice<'a, T> {
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn read(&self, offset: usize) -> Compact<T> {
        // SAFETY: callers only invoke `read` for `offset < len`, and `len > 0`
        // implies the slice is backed by valid storage.
        unsafe { Compact(T::decode((*self.packed).get(self.start + offset))) }
    }

    pub fn first(&self) -> Option<Compact<T>> {
        if self.is_empty() {
            None
        } else {
            Some(self.read(0))
        }
    }

    pub fn last(&self) -> Option<Compact<T>> {
        if self.is_empty() {
            None
        } else {
            Some(self.read(self.len - 1))
        }
    }

    pub fn split_first(&self) -> Option<(Compact<T>, CompactSlice<'a, T>)> {
        if self.is_empty() {
            return None;
        }
        Some((
            self.read(0),
            CompactSlice {
                packed: self.packed,
                start: self.start + 1,
                len: self.len - 1,
                _marker: PhantomData,
            },
        ))
    }

    pub fn split_last(&self) -> Option<(Compact<T>, CompactSlice<'a, T>)> {
        if self.is_empty() {
            return None;
        }
        Some((
            self.read(self.len - 1),
            CompactSlice {
                packed: self.packed,
                start: self.start,
                len: self.len - 1,
                _marker: PhantomData,
            },
        ))
    }

    pub fn split_at(
        &self,
        mid: usize,
    ) -> (CompactSlice<'a, T>, CompactSlice<'a, T>) {
        assert!(mid <= self.len);
        (
            CompactSlice {
                packed: self.packed,
                start: self.start,
                len: mid,
                _marker: PhantomData,
            },
            CompactSlice {
                packed: self.packed,
                start: self.start + mid,
                len: self.len - mid,
                _marker: PhantomData,
            },
        )
    }

    pub fn get(&self, index: usize) -> Option<Compact<T>> {
        if index < self.len {
            Some(self.read(index))
        } else {
            None
        }
    }

    /// Count elements equal to `value` within this slice. Encodes `value` once
    /// and counts matching packed lanes (1-bit `T` uses `count_ones`/
    /// `count_zeros`).
    #[inline]
    pub fn count(&self, value: T) -> usize {
        if self.len == 0 {
            return 0;
        }
        // SAFETY: `len > 0` implies the slice is backed by valid storage.
        unsafe {
            (*self.packed).count_in(self.start, self.len, T::encode(value))
        }
    }

    /// Returns the element at `index` without bounds checking.
    ///
    /// # Safety
    ///
    /// `index` must be in bounds (`index < self.len`) and the slice must
    /// reference initialized backing storage.
    pub unsafe fn get_unchecked(&self, index: usize) -> Compact<T> {
        self.read(index)
    }

    pub fn index(&self, index: usize) -> Compact<T> {
        assert!(
            index < self.len,
            "index out of bounds: the len is {} but the index is {}",
            self.len,
            index
        );
        self.read(index)
    }

    pub fn reborrow<'b>(&'b self) -> CompactSlice<'b, T>
    where
        'a: 'b,
    {
        *self
    }

    pub fn slice(&self, range: Range<usize>) -> CompactSlice<'a, T> {
        assert!(range.start <= range.end && range.end <= self.len);
        CompactSlice {
            packed: self.packed,
            start: self.start + range.start,
            len: range.end - range.start,
            _marker: PhantomData,
        }
    }

    pub fn as_ptr(&self) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed as *const Store<T>,
            index: self.start,
            value: None,
        }
    }

    /// Reassembles a `CompactSlice` from a raw pointer and length.
    ///
    /// # Safety
    ///
    /// `data.packed` must point to valid `Store<T>` storage holding at least
    /// `data.index + len` initialized elements, and the returned slice must
    /// not outlive that storage.
    pub unsafe fn from_raw_parts<'b>(
        data: CompactPtr<T>,
        len: usize,
    ) -> CompactSlice<'b, T> {
        CompactSlice {
            packed: data.packed,
            start: data.index,
            len,
            _marker: PhantomData,
        }
    }

    pub fn iter(&self) -> CompactIter<'a, T> {
        CompactIter {
            packed: self.packed,
            pos: self.start,
            end: self.start + self.len,
            _marker: PhantomData,
        }
    }

    pub fn to_vec(&self) -> CompactVec<T> {
        let mut v = CompactVec::<T>::with_capacity(self.len);
        for i in 0..self.len {
            // SAFETY: `i < len`, and `len > 0` implies valid backing storage.
            v.inner.push(unsafe { (*self.packed).get(self.start + i) });
        }
        v
    }

    pub fn chunks(&self, chunk_size: usize) -> CompactChunks<'a, T> {
        assert!(chunk_size != 0, "chunk size must be non-zero");
        CompactChunks {
            slice: *self,
            chunk_size,
            pos: 0,
        }
    }

    pub fn chunks_exact(&self, chunk_size: usize) -> CompactChunksExact<'a, T> {
        assert!(chunk_size != 0, "chunk size must be non-zero");
        let rem = self.len % chunk_size;
        CompactChunksExact {
            slice: *self,
            chunk_size,
            pos: 0,
            end: self.len - rem,
        }
    }

    pub fn binary_search_by<F>(&self, mut f: F) -> Result<usize, usize>
    where
        F: FnMut(Compact<T>) -> core::cmp::Ordering,
    {
        let mut left = 0usize;
        let mut right = self.len;
        while left < right {
            let mid = left + (right - left) / 2;
            match f(self.index(mid)) {
                core::cmp::Ordering::Less => left = mid + 1,
                core::cmp::Ordering::Greater => right = mid,
                core::cmp::Ordering::Equal => return Ok(mid),
            }
        }
        Err(left)
    }

    pub fn binary_search_by_key<K, F>(
        &self,
        key: &K,
        mut f: F,
    ) -> Result<usize, usize>
    where
        K: core::cmp::Ord,
        F: FnMut(Compact<T>) -> K,
    {
        self.binary_search_by(|probe| f(probe).cmp(key))
    }
}

impl<'a, T: CompactRepr> IntoIterator for CompactSlice<'a, T> {
    type Item = Compact<T>;
    type IntoIter = CompactIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: CompactRepr + core::fmt::Debug> core::fmt::Debug
    for CompactSlice<'_, T>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'a, T: CompactRepr + PartialEq> PartialEq for CompactSlice<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

impl<'a, T: CompactRepr + Eq> Eq for CompactSlice<'a, T> {}

impl<'a, T: CompactRepr + core::hash::Hash> core::hash::Hash
    for CompactSlice<'a, T>
{
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.len.hash(state);
        for val in self.iter() {
            val.hash(state);
        }
    }
}

// ---------------------------------------------------------------------------
// CompactSliceMut
// ---------------------------------------------------------------------------

pub struct CompactSliceMut<'a, T: CompactRepr> {
    packed: *mut Store<T>,
    start: usize,
    len: usize,
    _marker: PhantomData<&'a mut Store<T>>,
}

impl<'a, T: CompactRepr> Default for CompactSliceMut<'a, T> {
    fn default() -> Self {
        CompactSliceMut {
            packed: core::ptr::NonNull::<Store<T>>::dangling().as_ptr(),
            start: 0,
            len: 0,
            _marker: PhantomData,
        }
    }
}

#[allow(dead_code)]
impl<'a, T: CompactRepr> CompactSliceMut<'a, T> {
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_ref(&self) -> CompactSlice<'_, T> {
        CompactSlice {
            packed: self.packed as *const Store<T>,
            start: self.start,
            len: self.len,
            _marker: PhantomData,
        }
    }

    pub fn as_slice(&self) -> CompactSlice<'_, T> {
        self.as_ref()
    }

    unsafe fn read(&self, offset: usize) -> Compact<T> {
        Compact(T::decode((*self.packed).get(self.start + offset)))
    }

    pub fn first_mut(&mut self) -> Option<CompactRefMut<'_, T>> {
        if self.is_empty() {
            None
        } else {
            unsafe {
                Some(CompactRefMut::from_packed_ptr(self.packed, self.start))
            }
        }
    }

    pub fn last_mut(&mut self) -> Option<CompactRefMut<'_, T>> {
        if self.is_empty() {
            None
        } else {
            unsafe {
                Some(CompactRefMut::from_packed_ptr(
                    self.packed,
                    self.start + self.len - 1,
                ))
            }
        }
    }

    pub fn split_first_mut(
        self,
    ) -> Option<(CompactRefMut<'a, T>, CompactSliceMut<'a, T>)> {
        if self.is_empty() {
            return None;
        }
        unsafe {
            Some((
                CompactRefMut::from_packed_ptr(self.packed, self.start),
                CompactSliceMut {
                    packed: self.packed,
                    start: self.start + 1,
                    len: self.len - 1,
                    _marker: PhantomData,
                },
            ))
        }
    }

    pub fn split_last_mut(
        self,
    ) -> Option<(CompactRefMut<'a, T>, CompactSliceMut<'a, T>)> {
        if self.is_empty() {
            return None;
        }
        unsafe {
            Some((
                CompactRefMut::from_packed_ptr(
                    self.packed,
                    self.start + self.len - 1,
                ),
                CompactSliceMut {
                    packed: self.packed,
                    start: self.start,
                    len: self.len - 1,
                    _marker: PhantomData,
                },
            ))
        }
    }

    pub fn split_at_mut(
        self,
        mid: usize,
    ) -> (CompactSliceMut<'a, T>, CompactSliceMut<'a, T>) {
        assert!(mid <= self.len);
        (
            CompactSliceMut {
                packed: self.packed,
                start: self.start,
                len: mid,
                _marker: PhantomData,
            },
            CompactSliceMut {
                packed: self.packed,
                start: self.start + mid,
                len: self.len - mid,
                _marker: PhantomData,
            },
        )
    }

    pub fn swap(&mut self, a: usize, b: usize) {
        assert!(
            a < self.len && b < self.len,
            "index out of bounds: the len is {} but indices are {} and {}",
            self.len,
            a,
            b
        );
        unsafe {
            let pa = &mut *self.packed;
            let va = pa.get(self.start + a);
            let vb = pa.get(self.start + b);
            pa.set(self.start + a, vb);
            pa.set(self.start + b, va);
        }
    }

    pub fn get(&self, index: usize) -> Option<Compact<T>> {
        if index < self.len {
            unsafe { Some(self.read(index)) }
        } else {
            None
        }
    }

    /// Returns the element at `index` without bounds checking.
    ///
    /// # Safety
    ///
    /// `index` must be in bounds (`index < self.len`) and the slice must
    /// reference initialized backing storage.
    pub unsafe fn get_unchecked(&self, index: usize) -> Compact<T> {
        self.read(index)
    }

    pub fn index(&self, index: usize) -> Compact<T> {
        assert!(
            index < self.len,
            "index out of bounds: the len is {} but the index is {}",
            self.len,
            index
        );
        unsafe { self.read(index) }
    }

    pub fn get_mut(&mut self, index: usize) -> Option<CompactRefMut<'_, T>> {
        if index < self.len {
            unsafe {
                Some(CompactRefMut::from_packed_ptr(
                    self.packed,
                    self.start + index,
                ))
            }
        } else {
            None
        }
    }

    /// Returns a mutable reference to the element at `index` without bounds
    /// checking.
    ///
    /// # Safety
    ///
    /// `index` must be in bounds (`index < self.len`), the slice must reference
    /// initialized backing storage, and the caller must ensure no other
    /// references to the same element exist (no aliasing).
    pub unsafe fn get_unchecked_mut(
        &mut self,
        index: usize,
    ) -> CompactRefMut<'_, T> {
        CompactRefMut::from_packed_ptr(self.packed, self.start + index)
    }

    pub fn index_mut(&mut self, index: usize) -> CompactRefMut<'_, T> {
        assert!(
            index < self.len,
            "index out of bounds: the len is {} but the index is {}",
            self.len,
            index
        );
        unsafe {
            CompactRefMut::from_packed_ptr(self.packed, self.start + index)
        }
    }

    pub fn reborrow<'b>(&'b mut self) -> CompactSliceMut<'b, T>
    where
        'a: 'b,
    {
        CompactSliceMut {
            packed: self.packed,
            start: self.start,
            len: self.len,
            _marker: PhantomData,
        }
    }

    pub fn slice(&self, range: Range<usize>) -> CompactSlice<'_, T> {
        assert!(range.start <= range.end && range.end <= self.len);
        CompactSlice {
            packed: self.packed as *const Store<T>,
            start: self.start + range.start,
            len: range.end - range.start,
            _marker: PhantomData,
        }
    }

    pub fn as_ptr(&self) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed as *const Store<T>,
            index: self.start,
            value: None,
        }
    }

    pub fn as_mut_ptr(&mut self) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed,
            index: self.start,
        }
    }

    /// Reassembles a `CompactSliceMut` from a raw mutable pointer and length.
    ///
    /// # Safety
    ///
    /// `data.packed` must point to valid `Store<T>` storage holding at least
    /// `data.index + len` initialized elements, the caller must ensure no
    /// other references to that storage exist (unique mutable access), and the
    /// returned slice must not outlive the storage.
    pub unsafe fn from_raw_parts_mut<'b>(
        data: CompactPtrMut<T>,
        len: usize,
    ) -> CompactSliceMut<'b, T> {
        CompactSliceMut {
            packed: data.packed,
            start: data.index,
            len,
            _marker: PhantomData,
        }
    }

    pub fn __private_apply_permutation(&mut self, dest: &[usize]) {
        let len = self.len;
        let mut visited = vec![false; len];
        unsafe {
            let pa = &mut *self.packed;
            for start in 0..len {
                if visited[start] {
                    continue;
                }
                // `dest` maps each current index to its destination index.
                // Rotate each cycle into place using a single saved value so no
                // element is lost.
                visited[start] = true;
                let mut temp = pa.get(self.start + start);
                let mut current = start;
                loop {
                    let next = dest[current];
                    if next == start {
                        pa.set(self.start + start, temp);
                        break;
                    }
                    let saved = pa.get(self.start + next);
                    pa.set(self.start + next, temp);
                    temp = saved;
                    visited[next] = true;
                    current = next;
                }
            }
        }
    }

    pub fn iter(&self) -> CompactIter<'_, T> {
        CompactIter {
            packed: self.packed as *const Store<T>,
            pos: self.start,
            end: self.start + self.len,
            _marker: PhantomData,
        }
    }

    pub fn iter_mut(&mut self) -> CompactIterMut<'_, T> {
        CompactIterMut {
            packed: self.packed,
            pos: self.start,
            end: self.start + self.len,
            _marker: PhantomData,
        }
    }

    pub fn to_vec(&self) -> CompactVec<T> {
        self.as_ref().to_vec()
    }

    pub fn chunks_mut<'b>(
        &'b mut self,
        chunk_size: usize,
    ) -> CompactChunksMut<'b, T>
    where
        'a: 'b,
    {
        assert!(chunk_size != 0, "chunk size must be non-zero");
        CompactChunksMut {
            packed: self.packed,
            start: self.start,
            len: self.len,
            chunk_size,
            pos: 0,
            _marker: PhantomData,
        }
    }

    pub fn chunks_exact_mut<'b>(
        &'b mut self,
        chunk_size: usize,
    ) -> CompactChunksExactMut<'b, T>
    where
        'a: 'b,
    {
        assert!(chunk_size != 0, "chunk size must be non-zero");
        let rem = self.len % chunk_size;
        CompactChunksExactMut {
            packed: self.packed,
            start: self.start,
            len: self.len,
            chunk_size,
            pos: 0,
            end: self.len - rem,
            _marker: PhantomData,
        }
    }

    pub fn sort_by<F>(&mut self, mut f: F)
    where
        F: FnMut(Compact<T>, Compact<T>) -> core::cmp::Ordering,
    {
        let mut argsort: Vec<usize> = (0..self.len).collect();
        argsort.sort_by(|j, k| f(self.index(*j), self.index(*k)));
        let dest = crate::__invert_permutation(&argsort);
        self.__private_apply_permutation(&dest);
    }

    pub fn sort_by_key<F, K>(&mut self, mut f: F)
    where
        F: FnMut(Compact<T>) -> K,
        K: Ord,
    {
        let mut argsort: Vec<usize> = (0..self.len).collect();
        argsort.sort_by_key(|i| f(self.index(*i)));
        let dest = crate::__invert_permutation(&argsort);
        self.__private_apply_permutation(&dest);
    }

    pub fn sort(&mut self)
    where
        T: Ord,
    {
        self.sort_by(|a, b| a.0.cmp(&b.0));
    }
}

impl<'a, T: CompactRepr> IntoIterator for CompactSliceMut<'a, T> {
    type Item = CompactRefMut<'a, T>;
    type IntoIter = CompactIterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        CompactIterMut {
            packed: self.packed,
            pos: self.start,
            end: self.start + self.len,
            _marker: PhantomData,
        }
    }
}

impl<T: CompactRepr + core::fmt::Debug> core::fmt::Debug
    for CompactSliceMut<'_, T>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'a, T: CompactRepr + PartialEq> PartialEq for CompactSliceMut<'a, T> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

impl<'a, T: CompactRepr + Eq> Eq for CompactSliceMut<'a, T> {}

impl<'a, T: CompactRepr + core::hash::Hash> core::hash::Hash
    for CompactSliceMut<'a, T>
{
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.len.hash(state);
        for val in self.iter() {
            val.hash(state);
        }
    }
}

// ---------------------------------------------------------------------------
// CompactPtr / CompactPtrMut. A CompactPtr is either storage-backed (packed +
// index into a live column) or a value snapshot (derived from an owned
// `Compact<T>`, which has no backing storage to point into). Snapshot
// pointers are not null: they identify a real element and `as_ref`/`read`
// recover its value. CompactPtrMut has no snapshot mode — writes through a
// snapshot could not be observed anywhere, so the direct/owned mutable path
// still yields a null pointer.
// ---------------------------------------------------------------------------

#[derive(Copy, Clone)]
pub struct CompactPtr<T: CompactRepr> {
    packed: *const Store<T>,
    index: usize,
    value: Option<T>,
}

#[derive(Copy, Clone)]
pub struct CompactPtrMut<T: CompactRepr> {
    packed: *mut Store<T>,
    index: usize,
}

// Pointer-like: Debug/PartialEq/Eq/Hash over (packed address, index), matching
// raw pointers (no `T` bound needed). This lets `#[layout(Debug/PartialEq/Eq/
// Hash)]` derive on the generated Ptr/PtrMut types compile for compact columns.
impl<T: CompactRepr> core::fmt::Debug for CompactPtr<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CompactPtr")
            .field("packed", &self.packed)
            .field("index", &self.index)
            .field("value", &self.value.map(T::encode))
            .finish()
    }
}

impl<T: CompactRepr> PartialEq for CompactPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.packed == other.packed
            && self.index == other.index
            && self.value.map(T::encode) == other.value.map(T::encode)
    }
}

impl<T: CompactRepr> Eq for CompactPtr<T> {}

impl<T: CompactRepr> core::hash::Hash for CompactPtr<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.packed.hash(state);
        self.index.hash(state);
        self.value.map(T::encode).hash(state);
    }
}

impl<T: CompactRepr> core::fmt::Debug for CompactPtrMut<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CompactPtrMut")
            .field("packed", &self.packed)
            .field("index", &self.index)
            .finish()
    }
}

impl<T: CompactRepr> PartialEq for CompactPtrMut<T> {
    fn eq(&self, other: &Self) -> bool {
        self.packed == other.packed && self.index == other.index
    }
}

impl<T: CompactRepr> Eq for CompactPtrMut<T> {}

impl<T: CompactRepr> core::hash::Hash for CompactPtrMut<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.packed.hash(state);
        self.index.hash(state);
    }
}

#[allow(dead_code)]
impl<T: CompactRepr> CompactPtr<T> {
    pub fn is_null(self) -> bool {
        self.packed.is_null() && self.value.is_none()
    }

    /// Returns the element the pointer references, or `None` if it is null.
    ///
    /// # Safety
    ///
    /// If storage-backed and non-null, `self.packed` must point to valid
    /// `Store<T>` storage and `self.index` must address an initialized
    /// element within it. The caller must ensure the backing storage remains
    /// live while the returned value is used. Snapshot pointers dereference
    /// nothing and simply return the carried value.
    pub unsafe fn as_ref(self) -> Option<Compact<T>> {
        if let Some(value) = self.value {
            Some(Compact(value))
        } else if self.packed.is_null() {
            None
        } else {
            Some(Compact(T::decode((*self.packed).get(self.index))))
        }
    }

    /// Snapshot pointers convert to a null mutable pointer: there is no
    /// backing storage a write could be observed in.
    pub fn as_mut_ptr(&self) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed as *mut Store<T>,
            index: self.index,
        }
    }

    /// Produces a pointer offset by `count` elements (analogous to
    /// [`pointer::offset`](core::pointer::offset)).
    ///
    /// # Safety
    ///
    /// The resulting index (`self.index + count`) must be in bounds or one
    /// past the end of the same allocated backing storage.
    pub unsafe fn offset(self, count: isize) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed,
            index: (self.index as isize + count) as usize,
            value: self.value,
        }
    }

    pub fn wrapping_offset(self, count: isize) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed,
            index: self.index.wrapping_add(count as usize),
            value: self.value,
        }
    }

    /// Produces a pointer advanced by `count` elements (analogous to
    /// [`pointer::add`](core::pointer::add)).
    ///
    /// # Safety
    ///
    /// The resulting index (`self.index + count`) must be in bounds or one
    /// past the end of the same allocated backing storage.
    pub unsafe fn add(self, count: usize) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed,
            index: self.index + count,
            value: self.value,
        }
    }

    /// Produces a pointer moved back by `count` elements (analogous to
    /// [`pointer::sub`](core::pointer::sub)).
    ///
    /// # Safety
    ///
    /// `count` must not exceed `self.index`; the resulting index must be in
    /// bounds of the same allocated backing storage.
    pub unsafe fn sub(self, count: usize) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed,
            index: self.index - count,
            value: self.value,
        }
    }

    pub fn wrapping_add(self, count: usize) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed,
            index: self.index.wrapping_add(count),
            value: self.value,
        }
    }

    pub fn wrapping_sub(self, count: usize) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed,
            index: self.index.wrapping_sub(count),
            value: self.value,
        }
    }

    /// Reads the element the pointer references (analogous to
    /// [`pointer::read`](core::pointer::read)).
    ///
    /// # Safety
    ///
    /// For storage-backed pointers, `self.packed` must point to valid
    /// `Store<T>` storage and `self.index` must address an initialized
    /// element within it. Snapshot pointers dereference nothing and simply
    /// return the carried value.
    pub unsafe fn read(self) -> Compact<T> {
        if let Some(value) = self.value {
            Compact(value)
        } else {
            Compact(T::decode((*self.packed).get(self.index)))
        }
    }

    /// Reads the element the pointer references (analogous to
    /// [`pointer::read_volatile`](core::pointer::read_volatile)).
    ///
    /// # Safety
    ///
    /// See [`read`](CompactPtr::read).
    pub unsafe fn read_volatile(self) -> Compact<T> {
        self.read()
    }

    /// Reads the element the pointer references (analogous to
    /// [`pointer::read_unaligned`](core::pointer::read_unaligned)).
    ///
    /// # Safety
    ///
    /// See [`read`](CompactPtr::read).
    pub unsafe fn read_unaligned(self) -> Compact<T> {
        self.read()
    }
}

#[allow(dead_code)]
impl<T: CompactRepr> CompactPtrMut<T> {
    pub fn is_null(self) -> bool {
        self.packed.is_null()
    }

    /// Returns the element the pointer references, or `None` if it is null.
    ///
    /// # Safety
    ///
    /// If non-null, `self.packed` must point to valid `Store<T>` storage and
    /// `self.index` must address an initialized element within it.
    pub unsafe fn as_ref(self) -> Option<Compact<T>> {
        if self.is_null() {
            None
        } else {
            Some(Compact(T::decode((*self.packed).get(self.index))))
        }
    }

    /// Returns a mutable reference to the element the pointer references, or
    /// `None` if it is null.
    ///
    /// # Safety
    ///
    /// If non-null, `self.packed` must point to valid `Store<T>` storage,
    /// `self.index` must address an initialized element within it, and the
    /// caller must ensure no other references to the same element exist (no
    /// aliasing).
    pub unsafe fn as_mut<'a>(self) -> Option<CompactRefMut<'a, T>> {
        if self.is_null() {
            None
        } else {
            Some(CompactRefMut::from_packed_ptr(self.packed, self.index))
        }
    }

    pub fn as_ptr(&self) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed,
            index: self.index,
            value: None,
        }
    }

    /// Produces a pointer offset by `count` elements (analogous to
    /// [`pointer::offset`](core::pointer::offset)).
    ///
    /// # Safety
    ///
    /// The resulting index (`self.index + count`) must be in bounds or one
    /// past the end of the same allocated backing storage.
    pub unsafe fn offset(self, count: isize) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed,
            index: (self.index as isize + count) as usize,
        }
    }

    pub fn wrapping_offset(self, count: isize) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed,
            index: self.index.wrapping_add(count as usize),
        }
    }

    /// Produces a pointer advanced by `count` elements (analogous to
    /// [`pointer::add`](core::pointer::add)).
    ///
    /// # Safety
    ///
    /// The resulting index (`self.index + count`) must be in bounds or one
    /// past the end of the same allocated backing storage.
    pub unsafe fn add(self, count: usize) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed,
            index: self.index + count,
        }
    }

    /// Produces a pointer moved back by `count` elements (analogous to
    /// [`pointer::sub`](core::pointer::sub)).
    ///
    /// # Safety
    ///
    /// `count` must not exceed `self.index`; the resulting index must be in
    /// bounds of the same allocated backing storage.
    pub unsafe fn sub(self, count: usize) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed,
            index: self.index - count,
        }
    }

    pub fn wrapping_add(self, count: usize) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed,
            index: self.index.wrapping_add(count),
        }
    }

    pub fn wrapping_sub(self, count: usize) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed,
            index: self.index.wrapping_sub(count),
        }
    }

    /// Reads the element the pointer references (analogous to
    /// [`pointer::read`](core::pointer::read)).
    ///
    /// # Safety
    ///
    /// `self.packed` must point to valid `Store<T>` storage and `self.index`
    /// must address an initialized element within it.
    pub unsafe fn read(self) -> Compact<T> {
        Compact(T::decode((*self.packed).get(self.index)))
    }

    /// Reads the element the pointer references (analogous to
    /// [`pointer::read_volatile`](core::pointer::read_volatile)).
    ///
    /// # Safety
    ///
    /// See [`read`](CompactPtrMut::read).
    pub unsafe fn read_volatile(self) -> Compact<T> {
        self.read()
    }

    /// Reads the element the pointer references (analogous to
    /// [`pointer::read_unaligned`](core::pointer::read_unaligned)).
    ///
    /// # Safety
    ///
    /// See [`read`](CompactPtrMut::read).
    pub unsafe fn read_unaligned(self) -> Compact<T> {
        self.read()
    }

    /// Overwrites the element the pointer references (analogous to
    /// [`pointer::write`](core::pointer::write)).
    ///
    /// # Safety
    ///
    /// `self.packed` must point to valid `Store<T>` storage, `self.index` must
    /// address a writable element within it, and the caller must ensure no
    /// other references to the same element exist (no aliasing).
    #[allow(clippy::forget_non_drop)]
    pub unsafe fn write(self, val: Compact<T>) {
        (*self.packed).set(self.index, T::encode(val.0));
    }

    /// Overwrites the element the pointer references (analogous to
    /// [`pointer::write_volatile`](core::pointer::write_volatile)).
    ///
    /// # Safety
    ///
    /// See [`write`](CompactPtrMut::write).
    #[allow(clippy::forget_non_drop)]
    pub unsafe fn write_volatile(self, val: Compact<T>) {
        self.write(val);
    }

    /// Overwrites the element the pointer references (analogous to
    /// [`pointer::write_unaligned`](core::pointer::write_unaligned)).
    ///
    /// # Safety
    ///
    /// See [`write`](CompactPtrMut::write).
    #[allow(clippy::forget_non_drop)]
    pub unsafe fn write_unaligned(self, val: Compact<T>) {
        self.write(val);
    }
}

// ---------------------------------------------------------------------------
// CompactIter / CompactIterMut
// ---------------------------------------------------------------------------

pub struct CompactIter<'a, T: CompactRepr> {
    packed: *const Store<T>,
    pos: usize,
    end: usize,
    _marker: PhantomData<&'a Store<T>>,
}

impl<'a, T: CompactRepr> Iterator for CompactIter<'a, T> {
    type Item = Compact<T>;
    #[inline]
    fn next(&mut self) -> Option<Compact<T>> {
        if self.pos < self.end {
            // SAFETY: `pos < end`, and a non-empty range implies the iterator
            // was built from valid storage.
            let v = unsafe { Compact(T::decode((*self.packed).get(self.pos))) };
            self.pos += 1;
            Some(v)
        } else {
            None
        }
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let r = self.end - self.pos;
        (r, Some(r))
    }
}

impl<'a, T: CompactRepr> DoubleEndedIterator for CompactIter<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Compact<T>> {
        if self.pos < self.end {
            self.end -= 1;
            // SAFETY: `end` is still within the valid range.
            Some(unsafe { Compact(T::decode((*self.packed).get(self.end))) })
        } else {
            None
        }
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactIter<'_, T> {}

pub struct CompactIterMut<'a, T: CompactRepr> {
    packed: *mut Store<T>,
    pos: usize,
    end: usize,
    _marker: PhantomData<&'a mut Store<T>>,
}

impl<'a, T: CompactRepr> Iterator for CompactIterMut<'a, T> {
    type Item = CompactRefMut<'a, T>;
    #[inline]
    fn next(&mut self) -> Option<CompactRefMut<'a, T>> {
        if self.pos < self.end {
            let i = self.pos;
            self.pos += 1;
            // SAFETY: `i` is in `[pos, end)` ⊆ `[0, len)` of the live storage.
            Some(unsafe { CompactRefMut::from_packed_ptr(self.packed, i) })
        } else {
            None
        }
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let r = self.end - self.pos;
        (r, Some(r))
    }
}

impl<'a, T: CompactRepr> DoubleEndedIterator for CompactIterMut<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<CompactRefMut<'a, T>> {
        if self.pos < self.end {
            self.end -= 1;
            // SAFETY: `end` is in `[pos, end)` ⊆ `[0, len)` of the live
            // storage.
            Some(unsafe {
                CompactRefMut::from_packed_ptr(self.packed, self.end)
            })
        } else {
            None
        }
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactIterMut<'_, T> {}

// ---------------------------------------------------------------------------
// CompactDrain
// ---------------------------------------------------------------------------

pub struct CompactDrain<'a, T: CompactRepr> {
    packed: &'a mut Store<T>,
    // Original drain window; used by `Drop` to shift the tail regardless of
    // how many elements were yielded by the iterator.
    drain_start: usize,
    drain_end: usize,
    // Live iteration cursors.
    pos: usize,
    back: usize,
}

impl<T: CompactRepr> Iterator for CompactDrain<'_, T> {
    type Item = Compact<T>;
    #[inline]
    fn next(&mut self) -> Option<Compact<T>> {
        if self.pos < self.back {
            let v = Compact(T::decode(self.packed.get(self.pos)));
            self.pos += 1;
            Some(v)
        } else {
            None
        }
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let r = self.back - self.pos;
        (r, Some(r))
    }
}

impl<T: CompactRepr> DoubleEndedIterator for CompactDrain<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Compact<T>> {
        if self.pos < self.back {
            self.back -= 1;
            Some(Compact(T::decode(self.packed.get(self.back))))
        } else {
            None
        }
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactDrain<'_, T> {}

impl<T: CompactRepr> Drop for CompactDrain<'_, T> {
    fn drop(&mut self) {
        // Shift the tail [drain_end, len) down to [drain_start, ...) and drop
        // the drained length. Uses the ORIGINAL window so this runs even after
        // the iterator was fully (or partially) consumed.
        let drain_len = self.drain_end - self.drain_start;
        if drain_len == 0 {
            return;
        }
        let src = self.drain_end;
        let dst = self.drain_start;
        let shift = self.packed.len() - src;
        for i in 0..shift {
            let v = self.packed.get(src + i);
            self.packed.set(dst + i, v);
        }
        self.packed.truncate(self.packed.len() - drain_len);
    }
}

// ---------------------------------------------------------------------------
// Chunk iterators
// ---------------------------------------------------------------------------

pub struct CompactChunks<'a, T: CompactRepr> {
    slice: CompactSlice<'a, T>,
    chunk_size: usize,
    pos: usize,
}

impl<'a, T: CompactRepr> Iterator for CompactChunks<'a, T> {
    type Item = CompactSlice<'a, T>;
    #[inline]
    fn next(&mut self) -> Option<CompactSlice<'a, T>> {
        if self.pos >= self.slice.len || self.chunk_size == 0 {
            return None;
        }
        let end = (self.pos + self.chunk_size).min(self.slice.len);
        let result = CompactSlice {
            packed: self.slice.packed,
            start: self.slice.start + self.pos,
            len: end - self.pos,
            _marker: PhantomData,
        };
        self.pos = end;
        Some(result)
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.chunk_size == 0 {
            return (0, Some(0));
        }
        let r = self.slice.len.saturating_sub(self.pos);
        let c = (r + self.chunk_size - 1) / self.chunk_size;
        (c, Some(c))
    }
    #[inline]
    fn count(self) -> usize {
        self.size_hint().0
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactChunks<'_, T> {}

pub struct CompactChunksExact<'a, T: CompactRepr> {
    slice: CompactSlice<'a, T>,
    chunk_size: usize,
    pos: usize,
    end: usize,
}

impl<'a, T: CompactRepr> Iterator for CompactChunksExact<'a, T> {
    type Item = CompactSlice<'a, T>;
    #[inline]
    fn next(&mut self) -> Option<CompactSlice<'a, T>> {
        if self.pos >= self.end || self.chunk_size == 0 {
            return None;
        }
        let result = CompactSlice {
            packed: self.slice.packed,
            start: self.slice.start + self.pos,
            len: self.chunk_size,
            _marker: PhantomData,
        };
        self.pos += self.chunk_size;
        Some(result)
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.chunk_size == 0 {
            return (0, Some(0));
        }
        let r = self.end.saturating_sub(self.pos);
        let c = r / self.chunk_size;
        (c, Some(c))
    }
    #[inline]
    fn count(self) -> usize {
        self.size_hint().0
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactChunksExact<'_, T> {}

#[allow(dead_code)]
impl<'a, T: CompactRepr> CompactChunksExact<'a, T> {
    pub fn remainder(&self) -> CompactSlice<'a, T> {
        let rem_start = self.end.min(self.slice.len);
        CompactSlice {
            packed: self.slice.packed,
            start: self.slice.start + rem_start,
            len: self.slice.len - rem_start,
            _marker: PhantomData,
        }
    }
}

pub struct CompactChunksMut<'a, T: CompactRepr> {
    packed: *mut Store<T>,
    start: usize,
    len: usize,
    chunk_size: usize,
    pos: usize,
    _marker: PhantomData<&'a mut Store<T>>,
}

impl<'a, T: CompactRepr> Iterator for CompactChunksMut<'a, T> {
    type Item = CompactSliceMut<'a, T>;
    #[inline]
    fn next(&mut self) -> Option<CompactSliceMut<'a, T>> {
        if self.pos >= self.len || self.chunk_size == 0 {
            return None;
        }
        let end = (self.pos + self.chunk_size).min(self.len);
        let result = CompactSliceMut {
            packed: self.packed,
            start: self.start + self.pos,
            len: end - self.pos,
            _marker: PhantomData,
        };
        self.pos = end;
        Some(result)
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.chunk_size == 0 {
            return (0, Some(0));
        }
        let r = self.len.saturating_sub(self.pos);
        let c = (r + self.chunk_size - 1) / self.chunk_size;
        (c, Some(c))
    }
    #[inline]
    fn count(self) -> usize {
        self.size_hint().0
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactChunksMut<'_, T> {}

pub struct CompactChunksExactMut<'a, T: CompactRepr> {
    packed: *mut Store<T>,
    start: usize,
    len: usize,
    chunk_size: usize,
    pos: usize,
    end: usize,
    _marker: PhantomData<&'a mut Store<T>>,
}

impl<'a, T: CompactRepr> Iterator for CompactChunksExactMut<'a, T> {
    type Item = CompactSliceMut<'a, T>;
    #[inline]
    fn next(&mut self) -> Option<CompactSliceMut<'a, T>> {
        if self.pos >= self.end || self.chunk_size == 0 {
            return None;
        }
        let result = CompactSliceMut {
            packed: self.packed,
            start: self.start + self.pos,
            len: self.chunk_size,
            _marker: PhantomData,
        };
        self.pos += self.chunk_size;
        Some(result)
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.chunk_size == 0 {
            return (0, Some(0));
        }
        let r = self.end.saturating_sub(self.pos);
        let c = r / self.chunk_size;
        (c, Some(c))
    }
    #[inline]
    fn count(self) -> usize {
        self.size_hint().0
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactChunksExactMut<'_, T> {}

#[allow(dead_code)]
impl<'a, T: CompactRepr> CompactChunksExactMut<'a, T> {
    pub fn into_remainder(self) -> CompactSliceMut<'a, T> {
        let rem_start = self.end.min(self.len);
        CompactSliceMut {
            packed: self.packed,
            start: self.start + rem_start,
            len: self.len - rem_start,
            _marker: PhantomData,
        }
    }
}

// ---------------------------------------------------------------------------
// SoAIndex / SoAIndexMut for CompactSlice / CompactSliceMut
// ---------------------------------------------------------------------------

impl<'a, T: CompactRepr> crate::SoAIndex<CompactSlice<'a, T>> for usize {
    type RefOutput = Compact<T>;
    #[inline]
    fn get(self, slice: CompactSlice<'a, T>) -> Option<Self::RefOutput> {
        slice.get(self)
    }
    #[inline]
    unsafe fn get_unchecked(
        self,
        slice: CompactSlice<'a, T>,
    ) -> Self::RefOutput {
        slice.get_unchecked(self)
    }
    #[inline]
    fn index(self, slice: CompactSlice<'a, T>) -> Self::RefOutput {
        slice.index(self)
    }
}

impl<'a, T: CompactRepr> crate::SoAIndexMut<CompactSliceMut<'a, T>> for usize {
    type MutOutput = CompactRefMut<'a, T>;
    #[inline]
    fn get_mut(self, slice: CompactSliceMut<'a, T>) -> Option<Self::MutOutput> {
        if self < slice.len {
            unsafe {
                Some(CompactRefMut::from_packed_ptr(
                    slice.packed,
                    slice.start + self,
                ))
            }
        } else {
            None
        }
    }
    #[inline]
    unsafe fn get_unchecked_mut(
        self,
        slice: CompactSliceMut<'a, T>,
    ) -> Self::MutOutput {
        CompactRefMut::from_packed_ptr(slice.packed, slice.start + self)
    }
    #[inline]
    fn index_mut(self, slice: CompactSliceMut<'a, T>) -> Self::MutOutput {
        assert!(self < slice.len);
        unsafe {
            CompactRefMut::from_packed_ptr(slice.packed, slice.start + self)
        }
    }
}

impl<'a, T: CompactRepr> crate::SoAIndex<CompactSlice<'a, T>>
    for core::ops::Range<usize>
{
    type RefOutput = CompactSlice<'a, T>;
    #[inline]
    fn get(self, slice: CompactSlice<'a, T>) -> Option<Self::RefOutput> {
        if self.start <= self.end && self.end <= slice.len {
            Some(CompactSlice {
                packed: slice.packed,
                start: slice.start + self.start,
                len: self.end - self.start,
                _marker: PhantomData,
            })
        } else {
            None
        }
    }
    #[inline]
    unsafe fn get_unchecked(
        self,
        slice: CompactSlice<'a, T>,
    ) -> Self::RefOutput {
        CompactSlice {
            packed: slice.packed,
            start: slice.start + self.start,
            len: self.end - self.start,
            _marker: PhantomData,
        }
    }
    #[inline]
    fn index(self, slice: CompactSlice<'a, T>) -> Self::RefOutput {
        assert!(self.start <= self.end && self.end <= slice.len);
        CompactSlice {
            packed: slice.packed,
            start: slice.start + self.start,
            len: self.end - self.start,
            _marker: PhantomData,
        }
    }
}

impl<'a, T: CompactRepr> crate::SoAIndexMut<CompactSliceMut<'a, T>>
    for core::ops::Range<usize>
{
    type MutOutput = CompactSliceMut<'a, T>;
    #[inline]
    fn get_mut(self, slice: CompactSliceMut<'a, T>) -> Option<Self::MutOutput> {
        if self.start <= self.end && self.end <= slice.len {
            Some(CompactSliceMut {
                packed: slice.packed,
                start: slice.start + self.start,
                len: self.end - self.start,
                _marker: PhantomData,
            })
        } else {
            None
        }
    }
    #[inline]
    unsafe fn get_unchecked_mut(
        self,
        slice: CompactSliceMut<'a, T>,
    ) -> Self::MutOutput {
        CompactSliceMut {
            packed: slice.packed,
            start: slice.start + self.start,
            len: self.end - self.start,
            _marker: PhantomData,
        }
    }
    #[inline]
    fn index_mut(self, slice: CompactSliceMut<'a, T>) -> Self::MutOutput {
        assert!(self.start <= self.end && self.end <= slice.len);
        CompactSliceMut {
            packed: slice.packed,
            start: slice.start + self.start,
            len: self.end - self.start,
            _marker: PhantomData,
        }
    }
}

impl<'a, T: CompactRepr> crate::SoAIndex<CompactSlice<'a, T>>
    for core::ops::RangeTo<usize>
{
    type RefOutput = CompactSlice<'a, T>;
    #[inline]
    fn get(self, s: CompactSlice<'a, T>) -> Option<Self::RefOutput> {
        crate::SoAIndex::get(0..self.end, s)
    }
    #[inline]
    unsafe fn get_unchecked(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        crate::SoAIndex::get_unchecked(0..self.end, s)
    }
    #[inline]
    fn index(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        crate::SoAIndex::index(0..self.end, s)
    }
}

impl<'a, T: CompactRepr> crate::SoAIndexMut<CompactSliceMut<'a, T>>
    for core::ops::RangeTo<usize>
{
    type MutOutput = CompactSliceMut<'a, T>;
    #[inline]
    fn get_mut(self, s: CompactSliceMut<'a, T>) -> Option<Self::MutOutput> {
        crate::SoAIndexMut::get_mut(0..self.end, s)
    }
    #[inline]
    unsafe fn get_unchecked_mut(
        self,
        s: CompactSliceMut<'a, T>,
    ) -> Self::MutOutput {
        crate::SoAIndexMut::get_unchecked_mut(0..self.end, s)
    }
    #[inline]
    fn index_mut(self, s: CompactSliceMut<'a, T>) -> Self::MutOutput {
        crate::SoAIndexMut::index_mut(0..self.end, s)
    }
}

impl<'a, T: CompactRepr> crate::SoAIndex<CompactSlice<'a, T>>
    for core::ops::RangeFrom<usize>
{
    type RefOutput = CompactSlice<'a, T>;
    #[inline]
    fn get(self, s: CompactSlice<'a, T>) -> Option<Self::RefOutput> {
        if self.start <= s.len {
            Some(CompactSlice {
                packed: s.packed,
                start: s.start + self.start,
                len: s.len - self.start,
                _marker: PhantomData,
            })
        } else {
            None
        }
    }
    #[inline]
    unsafe fn get_unchecked(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        CompactSlice {
            packed: s.packed,
            start: s.start + self.start,
            len: s.len - self.start,
            _marker: PhantomData,
        }
    }
    #[inline]
    fn index(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        assert!(self.start <= s.len);
        CompactSlice {
            packed: s.packed,
            start: s.start + self.start,
            len: s.len - self.start,
            _marker: PhantomData,
        }
    }
}

impl<'a, T: CompactRepr> crate::SoAIndexMut<CompactSliceMut<'a, T>>
    for core::ops::RangeFrom<usize>
{
    type MutOutput = CompactSliceMut<'a, T>;
    #[inline]
    fn get_mut(self, s: CompactSliceMut<'a, T>) -> Option<Self::MutOutput> {
        if self.start <= s.len {
            Some(CompactSliceMut {
                packed: s.packed,
                start: s.start + self.start,
                len: s.len - self.start,
                _marker: PhantomData,
            })
        } else {
            None
        }
    }
    #[inline]
    unsafe fn get_unchecked_mut(
        self,
        s: CompactSliceMut<'a, T>,
    ) -> Self::MutOutput {
        CompactSliceMut {
            packed: s.packed,
            start: s.start + self.start,
            len: s.len - self.start,
            _marker: PhantomData,
        }
    }
    #[inline]
    fn index_mut(self, s: CompactSliceMut<'a, T>) -> Self::MutOutput {
        assert!(self.start <= s.len);
        CompactSliceMut {
            packed: s.packed,
            start: s.start + self.start,
            len: s.len - self.start,
            _marker: PhantomData,
        }
    }
}

impl<'a, T: CompactRepr> crate::SoAIndex<CompactSlice<'a, T>>
    for core::ops::RangeFull
{
    type RefOutput = CompactSlice<'a, T>;
    #[inline]
    fn get(self, s: CompactSlice<'a, T>) -> Option<Self::RefOutput> {
        Some(s)
    }
    #[inline]
    unsafe fn get_unchecked(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        s
    }
    #[inline]
    fn index(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        s
    }
}

impl<'a, T: CompactRepr> crate::SoAIndexMut<CompactSliceMut<'a, T>>
    for core::ops::RangeFull
{
    type MutOutput = CompactSliceMut<'a, T>;
    #[inline]
    fn get_mut(self, s: CompactSliceMut<'a, T>) -> Option<Self::MutOutput> {
        Some(s)
    }
    #[inline]
    unsafe fn get_unchecked_mut(
        self,
        s: CompactSliceMut<'a, T>,
    ) -> Self::MutOutput {
        s
    }
    #[inline]
    fn index_mut(self, s: CompactSliceMut<'a, T>) -> Self::MutOutput {
        s
    }
}

impl<'a, T: CompactRepr> crate::SoAIndex<CompactSlice<'a, T>>
    for core::ops::RangeInclusive<usize>
{
    type RefOutput = CompactSlice<'a, T>;
    #[inline]
    fn get(self, s: CompactSlice<'a, T>) -> Option<Self::RefOutput> {
        if *self.end() == usize::MAX {
            None
        } else {
            crate::SoAIndex::get(*self.start()..self.end().saturating_add(1), s)
        }
    }
    #[inline]
    unsafe fn get_unchecked(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        crate::SoAIndex::get_unchecked(
            *self.start()..self.end().saturating_add(1),
            s,
        )
    }
    #[inline]
    fn index(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        crate::SoAIndex::index(*self.start()..self.end().saturating_add(1), s)
    }
}

impl<'a, T: CompactRepr> crate::SoAIndexMut<CompactSliceMut<'a, T>>
    for core::ops::RangeInclusive<usize>
{
    type MutOutput = CompactSliceMut<'a, T>;
    #[inline]
    fn get_mut(self, s: CompactSliceMut<'a, T>) -> Option<Self::MutOutput> {
        if *self.end() == usize::MAX {
            None
        } else {
            crate::SoAIndexMut::get_mut(
                *self.start()..self.end().saturating_add(1),
                s,
            )
        }
    }
    #[inline]
    unsafe fn get_unchecked_mut(
        self,
        s: CompactSliceMut<'a, T>,
    ) -> Self::MutOutput {
        crate::SoAIndexMut::get_unchecked_mut(
            *self.start()..self.end().saturating_add(1),
            s,
        )
    }
    #[inline]
    fn index_mut(self, s: CompactSliceMut<'a, T>) -> Self::MutOutput {
        crate::SoAIndexMut::index_mut(
            *self.start()..self.end().saturating_add(1),
            s,
        )
    }
}

impl<'a, T: CompactRepr> crate::SoAIndex<CompactSlice<'a, T>>
    for core::ops::RangeToInclusive<usize>
{
    type RefOutput = CompactSlice<'a, T>;
    #[inline]
    fn get(self, s: CompactSlice<'a, T>) -> Option<Self::RefOutput> {
        if self.end == usize::MAX {
            None
        } else {
            crate::SoAIndex::get(0..self.end + 1, s)
        }
    }
    #[inline]
    unsafe fn get_unchecked(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        crate::SoAIndex::get_unchecked(0..self.end + 1, s)
    }
    #[inline]
    fn index(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        crate::SoAIndex::index(0..self.end + 1, s)
    }
}

impl<'a, T: CompactRepr> crate::SoAIndexMut<CompactSliceMut<'a, T>>
    for core::ops::RangeToInclusive<usize>
{
    type MutOutput = CompactSliceMut<'a, T>;
    #[inline]
    fn get_mut(self, s: CompactSliceMut<'a, T>) -> Option<Self::MutOutput> {
        if self.end == usize::MAX {
            None
        } else {
            crate::SoAIndexMut::get_mut(0..self.end + 1, s)
        }
    }
    #[inline]
    unsafe fn get_unchecked_mut(
        self,
        s: CompactSliceMut<'a, T>,
    ) -> Self::MutOutput {
        crate::SoAIndexMut::get_unchecked_mut(0..self.end + 1, s)
    }
    #[inline]
    fn index_mut(self, s: CompactSliceMut<'a, T>) -> Self::MutOutput {
        crate::SoAIndexMut::index_mut(0..self.end + 1, s)
    }
}
