//! Compact (bit-packed) columns for struct-of-arrays layouts.
//!
//! [`Compact<T>`] is the owning value wrapper that marks a field for
//! compaction. When a struct containing a `Compact<T>` field is derived with
//! `#[derive(SOA)]`, that column is stored bit-packed (one `T` per
//! [`CompactRepr::BITS`] bits) instead of as a `Vec<T>`.
//!
//! `T` must implement [`CompactRepr`]. This is implemented for `bool` (1 bit)
//! and, via `#[derive(CompactRepr)]`, for any fieldless enum with an unsigned
//! `#[repr(uN)]` repr (2 / 4 bits depending on the largest discriminant).
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

    /// Number of bits used per element (`1` for `bool`; `2`/`4` for enums).
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

    /// Returns a direct pointer to this value, so an element pointer derived
    /// from a `Ref` is never misclassified as null. Unlike column pointers,
    /// it is valid only while this `Compact` lives (for the compact field of
    /// a `Ref`, that is the `Ref` itself, not the underlying vec).
    #[inline(always)]
    pub fn as_ptr(&self) -> CompactPtr<T> {
        CompactPtr {
            packed: (&self.0 as *const T).cast(),
            index: DIRECT_INDEX,
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
    // Storage-backed mode: `packed` points at the column store and `index`
    // addresses the lane. Direct mode (`index == DIRECT_INDEX`): `packed` is
    // a disguised `*mut T` to a standalone owned value. The same sentinel
    // scheme as `CompactPtr` keeps the handle at two words.
    packed: *mut Store<T>,
    index: usize,
    _marker: PhantomData<&'a mut ()>,
}

impl<'a, T: CompactRepr> CompactRefMut<'a, T> {
    #[inline(always)]
    pub(crate) fn from_packed(packed: &'a mut Store<T>, index: usize) -> Self {
        Self {
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
            packed,
            index,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    fn from_value(value: &'a mut T) -> Self {
        Self {
            packed: (value as *mut T).cast(),
            index: DIRECT_INDEX,
            _marker: PhantomData,
        }
    }

    /// Read the current value live from the backing storage.
    #[inline]
    pub fn get(&self) -> T {
        // The packed (column) path is the overwhelmingly common one; the
        // direct path only fires for an owned value viewed as a RefMut.
        if ::branches::likely(self.index != DIRECT_INDEX) {
            // SAFETY: `packed` aliases borrowed storage that is still live;
            // `index` is in bounds by construction.
            unsafe { T::decode((*self.packed).get_unchecked(self.index)) }
        } else {
            // SAFETY: `packed` disguises the borrowed `&mut T`, still live.
            unsafe { *self.packed.cast::<T>() }
        }
    }

    /// Write `value` to the backing storage immediately.
    #[inline]
    pub fn set(&mut self, value: T) {
        if ::branches::likely(self.index != DIRECT_INDEX) {
            // SAFETY: as above.
            unsafe {
                (*self.packed).set_unchecked(self.index, T::encode(value));
            }
        } else {
            // SAFETY: as above.
            unsafe {
                *self.packed.cast::<T>() = value;
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
        // Both modes carry over verbatim (the sentinel travels in `index`).
        CompactPtr {
            packed: self.packed as *const Store<T>,
            index: self.index,
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
            && self.inner.range_eq(0, &other.inner, 0, self.len())
    }
}

impl<T: CompactRepr + Eq> Eq for CompactVec<T> {}

impl<T: CompactRepr + core::hash::Hash> core::hash::Hash for CompactVec<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for i in 0..self.len() {
            self.inner.get(i).hash(state);
        }
    }
}

impl<T: CompactRepr> core::iter::FromIterator<Compact<T>> for CompactVec<T> {
    fn from_iter<I: IntoIterator<Item = Compact<T>>>(iter: I) -> Self {
        let iterator = iter.into_iter();
        // Lower bound like `std`'s `collect`: a filter's upper bound is the
        // source length, so reserving it over-allocates the packed store.
        let mut result = CompactVec::<T>::with_capacity(iterator.size_hint().0);
        for item in iterator {
            result.push(item);
        }
        result
    }
}

impl<T: CompactRepr> core::iter::Extend<Compact<T>> for CompactVec<T> {
    fn extend<I: IntoIterator<Item = Compact<T>>>(&mut self, iter: I) {
        let iterator = iter.into_iter();
        self.reserve(iterator.size_hint().0);
        for item in iterator {
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
        // Grow by one (the pushed value is immediately overwritten by the
        // shift), then move the tail up one lane word-at-a-time.
        self.push(Compact::new(element.0));
        let len = self.len();
        self.inner.copy_lanes(index, index + 1, len - 1 - index);
        // SAFETY: `index < len` (the column just grew past `index <= len`).
        unsafe { self.inner.set_unchecked(index, T::encode(element.0)) };
    }

    pub fn remove(&mut self, index: usize) -> Compact<T> {
        assert!(
            index < self.len(),
            "index out of bounds: the len is {} but the index is {}",
            self.len(),
            index
        );
        // SAFETY: `index < self.len()` was asserted above.
        let val =
            unsafe { Compact(T::decode(self.inner.get_unchecked(index))) };
        // Move the tail down one lane word-at-a-time, then drop the last.
        let len = self.len();
        self.inner.copy_lanes(index + 1, index, len - 1 - index);
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
        // SAFETY: `index < self.len()` was asserted above.
        let val =
            unsafe { Compact(T::decode(self.inner.get_unchecked(index))) };
        let last = match self.inner.pop() {
            Some(v) => v,
            None => return val,
        };
        if index < self.inner.len() {
            // SAFETY: `index < self.inner.len()` just checked.
            unsafe { self.inner.set_unchecked(index, last) };
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
        // SAFETY: `index < self.len()` was asserted above.
        unsafe {
            let old = Compact(T::decode(self.inner.get_unchecked(index)));
            self.inner.set_unchecked(index, T::encode(element.0));
            old
        }
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
        // SAFETY: `index < self.len()` just asserted.
        unsafe { self.inner.set_unchecked(index, T::encode(value.0)) };
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
            self.inner.extend_fill(T::encode(value.0), new_len - cur);
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
        other.extend_from_packed(&self.inner, at, self.inner.len() - at);
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
        if other.is_empty() {
            return;
        }
        // SAFETY: `other.len() > 0` implies `other.packed` points at live
        // storage; it cannot alias `self` (a shared borrow cannot coexist with
        // this `&mut self`). Word-aligned ranges copy wholesale.
        let src: &Store<T> = unsafe { &*other.packed };
        self.inner.extend_from_packed(src, other.start, other.len());
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
        CompactIter::new(&self.inner as *const Store<T>, 0, self.inner.len())
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
            // SAFETY: `index < self.inner.len()` just checked.
            Some(unsafe { Compact(T::decode(self.inner.get_unchecked(index))) })
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
        let old_len = self.inner.len();
        // Leak safety (mirrors `Vec::drain`): shorten to `start` up front
        // while the drained lanes stay alive in the backing words. `Drop`
        // shifts the tail down and restores the final length; a leaked drain
        // leaves a short but consistent column, so sibling columns in a
        // generated struct-of-arrays vec can never end up longer than this
        // one.
        //
        // SAFETY: `start <= old_len`, so every lane `< start` is initialized
        // and backed (`set_len` keeps the words alive).
        unsafe { self.inner.set_len(start) };
        CompactDrain {
            packed: &mut self.inner,
            drain_start: start,
            drain_end: end,
            old_len,
            pos: start,
            back: end,
        }
    }

    /// Replace the elements in `range` with `replace_with`, returning the
    /// removed elements as a (bit-packed) [`CompactVec`].
    ///
    /// Unlike [`Vec::splice`] this is eager, but both the removed elements
    /// and the buffered replacement stay bit-packed, and the tail is shifted
    /// with one word-level move.
    pub fn splice<R, I>(&mut self, range: R, replace_with: I) -> CompactVec<T>
    where
        R: core::ops::RangeBounds<usize>,
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
        let remove_count = end - start;
        // Word-copy the removed range out while it is still contiguous.
        let mut removed = Store::<T>::with_capacity(remove_count);
        removed.extend_from_packed(&self.inner, start, remove_count);
        // Buffer the replacement bit-packed (an iterator of unknown length
        // cannot be written in place while the tail still occupies the
        // range).
        let iterator = replace_with.into_iter();
        let mut replacement = Store::<T>::with_capacity(iterator.size_hint().0);
        for item in iterator {
            replacement.push(T::encode(item.0));
        }
        let insert_count = replacement.len();
        if insert_count < remove_count {
            let tail_len = self.inner.len() - end;
            self.inner.copy_lanes(end, start + insert_count, tail_len);
            self.inner
                .truncate(self.inner.len() - (remove_count - insert_count));
        } else {
            for _ in 0..insert_count - remove_count {
                self.inner.push(0);
            }
            let shift_from = start + remove_count;
            let shift_to = start + insert_count;
            let tail_len = self.inner.len() - shift_to;
            self.inner.copy_lanes(shift_from, shift_to, tail_len);
        }
        for i in 0..insert_count {
            // SAFETY: `start + i < start + insert_count <= self.len()` after
            // the resize above.
            unsafe {
                self.inner
                    .set_unchecked(start + i, replacement.get_unchecked(i));
            }
        }
        CompactVec { inner: removed }
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

/// Owning by-value iterator for [`CompactVec`] (analogous to
/// `std::vec::IntoIter`).
pub struct CompactIntoIter<T: CompactRepr> {
    inner: Store<T>,
    pos: usize,
    end: usize,
}

impl<T: CompactRepr> Iterator for CompactIntoIter<T> {
    type Item = Compact<T>;
    #[inline]
    fn next(&mut self) -> Option<Compact<T>> {
        if self.pos < self.end {
            // SAFETY: `pos < end <= len` of the owned storage.
            let v = Compact(T::decode(unsafe {
                self.inner.get_unchecked(self.pos)
            }));
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

impl<T: CompactRepr> DoubleEndedIterator for CompactIntoIter<T> {
    #[inline]
    fn next_back(&mut self) -> Option<Compact<T>> {
        if self.pos < self.end {
            self.end -= 1;
            // SAFETY: as in `next`.
            Some(Compact(T::decode(unsafe {
                self.inner.get_unchecked(self.end)
            })))
        } else {
            None
        }
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactIntoIter<T> {}

impl<T: CompactRepr> IntoIterator for CompactVec<T> {
    type Item = Compact<T>;
    type IntoIter = CompactIntoIter<T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        let end = self.inner.len();
        CompactIntoIter {
            inner: self.inner,
            pos: 0,
            end,
        }
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

    /// Read the element at `offset` from the backing storage.
    ///
    /// # Safety
    ///
    /// `offset < self.len` must hold, which also implies the slice is backed
    /// by valid storage.
    unsafe fn read(&self, offset: usize) -> Compact<T> {
        unsafe {
            Compact(T::decode(
                (*self.packed).get_unchecked(self.start + offset),
            ))
        }
    }

    pub fn first(&self) -> Option<Compact<T>> {
        if self.is_empty() {
            None
        } else {
            // SAFETY: `0 < self.len`.
            Some(unsafe { self.read(0) })
        }
    }

    pub fn last(&self) -> Option<Compact<T>> {
        if self.is_empty() {
            None
        } else {
            // SAFETY: `self.len - 1 < self.len`.
            Some(unsafe { self.read(self.len - 1) })
        }
    }

    pub fn split_first(&self) -> Option<(Compact<T>, CompactSlice<'a, T>)> {
        if self.is_empty() {
            return None;
        }
        Some((
            // SAFETY: `0 < self.len`.
            unsafe { self.read(0) },
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
            // SAFETY: `self.len - 1 < self.len`.
            unsafe { self.read(self.len - 1) },
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
            // SAFETY: `index < self.len` just checked.
            Some(unsafe { self.read(index) })
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
        // SAFETY: forwarded contract (`index < self.len`).
        unsafe { self.read(index) }
    }

    pub fn index(&self, index: usize) -> Compact<T> {
        assert!(
            index < self.len,
            "index out of bounds: the len is {} but the index is {}",
            self.len,
            index
        );
        // SAFETY: `index < self.len` just asserted.
        unsafe { self.read(index) }
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
        CompactIter::new(self.packed, self.start, self.start + self.len)
    }

    pub fn to_vec(&self) -> CompactVec<T> {
        let mut v = CompactVec::<T>::with_capacity(self.len);
        if self.len > 0 {
            // SAFETY: `len > 0` implies valid backing storage that does not
            // alias the fresh `v`.
            v.inner.extend_from_packed(
                unsafe { &*self.packed },
                self.start,
                self.len,
            );
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
        if self.len != other.len {
            return false;
        }
        if self.len == 0 {
            return true;
        }
        // SAFETY: `len > 0` implies both `packed` point at live storage; shared
        // reads are fine even when the slices overlap. Word-aligned starts
        // compare word-at-a-time.
        let a = unsafe { &*self.packed };
        let b = unsafe { &*other.packed };
        a.range_eq(self.start, b, other.start, self.len)
    }
}

impl<'a, T: CompactRepr + Eq> Eq for CompactSlice<'a, T> {}

impl<'a, T: CompactRepr + core::hash::Hash> core::hash::Hash
    for CompactSlice<'a, T>
{
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.len.hash(state);
        // SAFETY: `packed` is a valid `Store<T>` for the slice's lifetime, and
        // `start + i < start + len`.
        let a = unsafe { &*self.packed };
        for i in 0..self.len {
            a.get(self.start + i).hash(state);
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

    /// # Safety
    ///
    /// `offset < self.len` must hold, which also implies the slice is backed
    /// by valid storage.
    unsafe fn read(&self, offset: usize) -> Compact<T> {
        unsafe {
            Compact(T::decode(
                (*self.packed).get_unchecked(self.start + offset),
            ))
        }
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
        // SAFETY: `a < self.len` and `b < self.len` were just asserted, so
        // both lanes are in bounds of the live backing storage.
        unsafe {
            let pa = &mut *self.packed;
            let va = pa.get_unchecked(self.start + a);
            let vb = pa.get_unchecked(self.start + b);
            pa.set_unchecked(self.start + a, vb);
            pa.set_unchecked(self.start + b, va);
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
        let mut visited = crate::__validate_permutation(dest, self.len);
        // SAFETY: `dest` was just validated as a permutation of `0..len`.
        unsafe {
            self.__private_apply_permutation_unchecked(dest, &mut visited)
        }
    }

    /// Apply a destination permutation without re-validating it, reusing the
    /// caller's scratch bitmap. Generated composite sorts validate once and
    /// then call this per column.
    ///
    /// # Safety
    ///
    /// `dest` must be a permutation of `0..self.len()` and `visited` must
    /// have been created with capacity for at least `self.len()` bits.
    #[doc(hidden)]
    pub unsafe fn __private_apply_permutation_unchecked(
        &mut self,
        dest: &[usize],
        visited: &mut crate::VisitedBits,
    ) {
        let len = self.len;
        visited.clear();
        // SAFETY: every index in `dest` is `< len` per the caller's
        // permutation contract, so `self.start + i` stays within the slice's
        // live backing storage.
        unsafe {
            let pa = &mut *self.packed;
            for start in 0..len {
                if visited.test(start) {
                    continue;
                }
                // `dest` maps each current index to its destination index.
                // Rotate each cycle into place using a single saved value so no
                // element is lost.
                visited.set(start);
                let mut temp = pa.get_unchecked(self.start + start);
                let mut current = start;
                loop {
                    let next = *dest.get_unchecked(current);
                    if next == start {
                        pa.set_unchecked(self.start + start, temp);
                        break;
                    }
                    let saved = pa.get_unchecked(self.start + next);
                    pa.set_unchecked(self.start + next, temp);
                    temp = saved;
                    visited.set(next);
                    current = next;
                }
            }
        }
    }

    pub fn iter(&self) -> CompactIter<'_, T> {
        CompactIter::new(
            self.packed as *const Store<T>,
            self.start,
            self.start + self.len,
        )
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

    /// Sort the slice with a comparator.
    ///
    /// A compact column holds at most `2^BITS <= 16` distinct values, so this
    /// is a counting sort: the raw lane values are counted in one word-level
    /// pass, the distinct values are ordered with `f`, and the slice is
    /// rewritten with word-level fills. Consequences (all documented
    /// contract):
    ///
    /// * `f` is invoked on distinct values only (at most 120 calls), never once
    ///   per element, so it must be a pure comparison.
    /// * Elements whose values compare `Equal` are grouped by their stored
    ///   value (the observable result of `slice::sort_unstable_by`), not
    ///   interleaved in their original order.
    ///
    /// Runs in `O(n / lanes_per_word)` word operations with no allocation.
    pub fn sort_by<F>(&mut self, mut f: F)
    where
        F: FnMut(Compact<T>, Compact<T>) -> core::cmp::Ordering,
    {
        let len = self.len;
        if len <= 1 {
            return;
        }
        let nvals = 1usize << T::BITS;
        debug_assert!(nvals <= 16);
        // Count every raw lane value. The last bucket comes from the length,
        // so a 1-bit column costs a single `count_ones` pass.
        let mut counts = [0usize; 16];
        {
            // SAFETY: the slice's lanes are within the live store.
            let pa = unsafe { &*self.packed };
            let mut seen = 0;
            for (v, slot) in counts.iter_mut().enumerate().take(nvals - 1) {
                let c = pa.count_in(self.start, len, v);
                *slot = c;
                seen += c;
            }
            counts[nvals - 1] = len - seen;
        }
        // Order the value table with `f` (stable insertion sort of at most 16
        // entries, so equal-comparing values keep ascending raw order).
        let mut order = [0usize; 16];
        for (v, slot) in order.iter_mut().enumerate().take(nvals) {
            *slot = v;
        }
        for i in 1..nvals {
            let mut j = i;
            while j > 0
                && f(
                    Compact(T::decode(order[j - 1])),
                    Compact(T::decode(order[j])),
                ) == core::cmp::Ordering::Greater
            {
                order.swap(j - 1, j);
                j -= 1;
            }
        }
        // Rewrite the slice as runs of equal values, word-level.
        // SAFETY: the slice's lanes are within the live store; the runs sum
        // to exactly `len`.
        let pa = unsafe { &mut *self.packed };
        let mut at = self.start;
        for &v in order.iter().take(nvals) {
            let c = counts[v];
            if c > 0 {
                pa.fill_range(at, c, v);
                at += c;
            }
        }
    }

    /// Sort the slice by a key function.
    ///
    /// Counting sort, like [`sort_by`](Self::sort_by): the key function is
    /// invoked on distinct values only (at most 240 calls), and elements
    /// whose keys compare equal are grouped by their stored value.
    pub fn sort_by_key<F, K>(&mut self, mut f: F)
    where
        F: FnMut(Compact<T>) -> K,
        K: Ord,
    {
        self.sort_by(|a, b| f(a).cmp(&f(b)));
    }

    /// Sort the slice by `T`'s ordering (counting sort, see
    /// [`sort_by`](Self::sort_by)).
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
        if self.len != other.len {
            return false;
        }
        if self.len == 0 {
            return true;
        }
        // SAFETY: `len > 0` implies both `packed` point at live storage; `eq`
        // only reads, so shared refs are fine even though the pointers are
        // `*mut`. Word-aligned starts compare word-at-a-time.
        let a = unsafe { &*self.packed };
        let b = unsafe { &*other.packed };
        a.range_eq(self.start, b, other.start, self.len)
    }
}

impl<'a, T: CompactRepr + Eq> Eq for CompactSliceMut<'a, T> {}

impl<'a, T: CompactRepr + core::hash::Hash> core::hash::Hash
    for CompactSliceMut<'a, T>
{
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.len.hash(state);
        // SAFETY: `packed` is a valid `Store<T>` for the slice's lifetime, and
        // `start + i < start + len`.
        let a = unsafe { &*self.packed };
        for i in 0..self.len {
            a.get(self.start + i).hash(state);
        }
    }
}

// ---------------------------------------------------------------------------
// CompactPtr / CompactPtrMut. A pointer is either storage-backed (`packed`
// points to a column, `index` addresses the element) or direct (`packed`
// holds a pointer to a standalone `Compact<T>` value, e.g. the compact
// field of a `Ref`/`RefMut` or an owned value, and `index` is
// `DIRECT_INDEX`). Direct pointers keep element pointers derived from a
// `Ref`/`RefMut` non-null and usable instead of collapsing them to null.
// The sentinel keeps the layout at two words and leaves every
// storage-backed code path unchanged. `CompactPtr::as_mut_ptr` is the one
// exception: a direct CONST pointer borrows immutably, so there is no
// storage a write could legally target and it converts to null.
// ---------------------------------------------------------------------------

/// `index` sentinel marking a direct `CompactPtr`. A storage-backed index can
/// never reach `usize::MAX` (no allocation can hold that many elements), so
/// the two modes cannot collide.
const DIRECT_INDEX: usize = usize::MAX;

#[derive(Copy, Clone)]
pub struct CompactPtr<T: CompactRepr> {
    packed: *const Store<T>,
    index: usize,
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
            .finish()
    }
}

impl<T: CompactRepr> PartialEq for CompactPtr<T> {
    fn eq(&self, other: &Self) -> bool {
        self.packed == other.packed && self.index == other.index
    }
}

impl<T: CompactRepr> Eq for CompactPtr<T> {}

impl<T: CompactRepr> core::hash::Hash for CompactPtr<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.packed.hash(state);
        self.index.hash(state);
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
        self.packed.is_null()
    }

    /// Returns the element the pointer references, or `None` if it is null.
    ///
    /// # Safety
    ///
    /// If non-null, `self.packed` must point to valid `Store<T>` storage
    /// (or, for a direct pointer, to a live `T`) and `self.index` must
    /// address an initialized element within it. The caller must ensure the
    /// backing storage remains live while the returned value is used.
    pub unsafe fn as_ref(self) -> Option<Compact<T>> {
        if self.is_null() {
            None
        } else if self.index == DIRECT_INDEX {
            Some(Compact(*self.packed.cast::<T>()))
        } else {
            // SAFETY: the caller guarantees `index` addresses an initialized
            // element of the live storage.
            Some(Compact(T::decode((*self.packed).get_unchecked(self.index))))
        }
    }

    /// Direct pointers convert to a null mutable pointer: they borrow a
    /// standalone value immutably, so there is no storage a write could
    /// legally target.
    pub fn as_mut_ptr(&self) -> CompactPtrMut<T> {
        if self.index == DIRECT_INDEX {
            CompactPtrMut {
                packed: core::ptr::null_mut(),
                index: 0,
            }
        } else {
            CompactPtrMut {
                packed: self.packed as *mut Store<T>,
                index: self.index,
            }
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
        }
    }

    /// Reads the element the pointer references (analogous to
    /// [`pointer::read`](core::pointer::read)).
    ///
    /// # Safety
    ///
    /// `self.packed` must point to valid `Store<T>` storage (or, for a
    /// direct pointer, to a live `T`) and `self.index` must address an
    /// initialized element within it.
    pub unsafe fn read(self) -> Compact<T> {
        if self.index == DIRECT_INDEX {
            Compact(*self.packed.cast::<T>())
        } else {
            // SAFETY: the caller guarantees `index` addresses an initialized
            // element of the live storage.
            Compact(T::decode((*self.packed).get_unchecked(self.index)))
        }
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
        } else if self.index == DIRECT_INDEX {
            // SAFETY: a direct pointer disguises a live `*mut T`.
            Some(Compact(*self.packed.cast::<T>()))
        } else {
            // SAFETY: the caller guarantees `index` addresses an initialized
            // element of the live storage.
            Some(Compact(T::decode((*self.packed).get_unchecked(self.index))))
        }
    }

    /// Returns a mutable reference to the element the pointer references, or
    /// `None` if it is null.
    ///
    /// # Safety
    ///
    /// If non-null, `self.packed` must point to valid `Store<T>` storage (or,
    /// for a direct pointer, to a live `T`), `self.index` must address an
    /// initialized element within it, and the caller must ensure no other
    /// references to the same element exist (no aliasing).
    pub unsafe fn as_mut<'a>(self) -> Option<CompactRefMut<'a, T>> {
        if self.is_null() {
            None
        } else {
            // Identical representation: the direct sentinel carries over.
            Some(CompactRefMut::from_packed_ptr(self.packed, self.index))
        }
    }

    pub fn as_ptr(&self) -> CompactPtr<T> {
        CompactPtr {
            packed: self.packed,
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
    pub unsafe fn offset(self, count: isize) -> CompactPtrMut<T> {
        CompactPtrMut {
            packed: self.packed,
            index: (self.index as isize + count) as usize,
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

    /// Reads the element the pointer references (analogous to
    /// [`pointer::read`](core::pointer::read)).
    ///
    /// # Safety
    ///
    /// `self.packed` must point to valid `Store<T>` storage and `self.index`
    /// must address an initialized element within it.
    pub unsafe fn read(self) -> Compact<T> {
        if self.index == DIRECT_INDEX {
            // SAFETY: a direct pointer disguises a live `*mut T`.
            Compact(*self.packed.cast::<T>())
        } else {
            // SAFETY: the caller guarantees `index` addresses an initialized
            // element of the live storage.
            Compact(T::decode((*self.packed).get_unchecked(self.index)))
        }
    }

    /// Overwrites the element the pointer references (analogous to
    /// [`pointer::write`](core::pointer::write)).
    ///
    /// # Safety
    ///
    /// `self.packed` must point to valid `Store<T>` storage (or, for a direct
    /// pointer, to a live, writable `T`), `self.index` must address a
    /// writable element within it, and the caller must ensure no other
    /// references to the same element exist (no aliasing).
    #[allow(clippy::forget_non_drop)]
    pub unsafe fn write(self, val: Compact<T>) {
        if self.index == DIRECT_INDEX {
            // SAFETY: a direct pointer disguises a live, writable `*mut T`.
            *self.packed.cast::<T>() = val.0;
        } else {
            // SAFETY: the caller guarantees `index` addresses a writable
            // element of the live storage.
            (*self.packed).set_unchecked(self.index, T::encode(val.0));
        }
    }
}

// ---------------------------------------------------------------------------
// CompactIter / CompactIterMut
// ---------------------------------------------------------------------------

pub struct CompactIter<'a, T: CompactRepr> {
    packed: *const Store<T>,
    pos: usize,
    end: usize,
    // Forward read cache: the current word pre-shifted so the lane at `pos`
    // sits in the low `BITS` bits, with `avail` lanes left in it (0 forces a
    // reload). Each forward step is then one mask and one constant shift;
    // memory is touched once per word. Sound because the iterator holds a
    // shared borrow, so the storage cannot change. Back reads go straight to
    // memory and leave the cache alone.
    cur_word: usize,
    avail: usize,
    _marker: PhantomData<&'a Store<T>>,
}

impl<'a, T: CompactRepr> CompactIter<'a, T> {
    #[inline]
    fn new(packed: *const Store<T>, pos: usize, end: usize) -> Self {
        Self {
            packed,
            pos,
            end,
            cur_word: 0,
            avail: 0,
            _marker: PhantomData,
        }
    }

    /// Read the lane at `pos` and advance, reloading the shifted word cache
    /// at word boundaries.
    ///
    /// # Safety
    ///
    /// `pos < end` must hold (the lane is within the live storage).
    #[inline(always)]
    unsafe fn read_front(&mut self) -> Compact<T> {
        let per = (usize::BITS / T::BITS) as usize;
        if self.avail == 0 {
            let off = self.pos % per;
            // SAFETY: `pos < end <= len`, so the word is live.
            self.cur_word = unsafe { (*self.packed).word(self.pos / per) }
                >> (off * T::BITS as usize);
            self.avail = per - off;
        }
        let raw = self.cur_word & ((1usize << T::BITS) - 1);
        self.cur_word >>= T::BITS;
        self.avail -= 1;
        self.pos += 1;
        Compact(T::decode(raw))
    }

    /// Step the back end down and read that lane directly (uncached).
    ///
    /// # Safety
    ///
    /// `pos < end` must hold.
    #[inline]
    unsafe fn read_back(&mut self) -> Compact<T> {
        self.end -= 1;
        let per = (usize::BITS / T::BITS) as usize;
        let off = (self.end % per) * T::BITS as usize;
        // SAFETY: `end` was within the live storage.
        let word = unsafe { (*self.packed).word(self.end / per) };
        Compact(T::decode((word >> off) & ((1usize << T::BITS) - 1)))
    }
}

impl<'a, T: CompactRepr> Iterator for CompactIter<'a, T> {
    type Item = Compact<T>;
    #[inline]
    fn next(&mut self) -> Option<Compact<T>> {
        if self.pos < self.end {
            // SAFETY: `pos < end` just checked.
            Some(unsafe { self.read_front() })
        } else {
            None
        }
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let r = self.end - self.pos;
        (r, Some(r))
    }
    #[inline]
    fn count(self) -> usize {
        self.end - self.pos
    }
}

impl<'a, T: CompactRepr> DoubleEndedIterator for CompactIter<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Compact<T>> {
        if self.pos < self.end {
            // SAFETY: `pos < end` just checked.
            Some(unsafe { self.read_back() })
        } else {
            None
        }
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactIter<'_, T> {}

impl<'a, T: CompactRepr> crate::SoACursor for CompactIter<'a, T> {
    type Item = Compact<T>;
    #[inline(always)]
    unsafe fn cursor_next(&mut self) -> Compact<T> {
        // SAFETY: the caller's length contract replaces the `pos < end`
        // check.
        unsafe { self.read_front() }
    }
    #[inline(always)]
    unsafe fn cursor_next_back(&mut self) -> Compact<T> {
        // SAFETY: as in `cursor_next`.
        unsafe { self.read_back() }
    }
}

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

impl<'a, T: CompactRepr> crate::SoACursor for CompactIterMut<'a, T> {
    type Item = CompactRefMut<'a, T>;
    #[inline(always)]
    unsafe fn cursor_next(&mut self) -> CompactRefMut<'a, T> {
        let i = self.pos;
        self.pos += 1;
        // SAFETY: the caller's length contract keeps `i` within the
        // iterator's range of the live storage.
        unsafe { CompactRefMut::from_packed_ptr(self.packed, i) }
    }
    #[inline(always)]
    unsafe fn cursor_next_back(&mut self) -> CompactRefMut<'a, T> {
        self.end -= 1;
        // SAFETY: as in `cursor_next`.
        unsafe { CompactRefMut::from_packed_ptr(self.packed, self.end) }
    }
}

// ---------------------------------------------------------------------------
// CompactDrain
// ---------------------------------------------------------------------------

pub struct CompactDrain<'a, T: CompactRepr> {
    // The store's logical length was lowered to `drain_start` when the drain
    // was created (leak safety), but every lane `< old_len` stays alive in
    // the backing words until `Drop` shifts the tail and truncates.
    packed: &'a mut Store<T>,
    // Original drain window; used by `Drop` to shift the tail regardless of
    // how many elements were yielded by the iterator.
    drain_start: usize,
    drain_end: usize,
    // Pre-drain length of the store.
    old_len: usize,
    // Live iteration cursors.
    pos: usize,
    back: usize,
}

impl<T: CompactRepr> Iterator for CompactDrain<'_, T> {
    type Item = Compact<T>;
    #[inline]
    fn next(&mut self) -> Option<Compact<T>> {
        if self.pos < self.back {
            // SAFETY: `pos < back <= drain_end <= old_len`; the lane is
            // initialized and its word stays alive for the drain's lifetime
            // even though it sits beyond the store's lowered length.
            let v = Compact(T::decode(unsafe {
                self.packed.get_unchecked(self.pos)
            }));
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
            // SAFETY: as in `next`.
            Some(Compact(T::decode(unsafe {
                self.packed.get_unchecked(self.back)
            })))
        } else {
            None
        }
    }
}

impl<T: CompactRepr> ExactSizeIterator for CompactDrain<'_, T> {}

impl<T: CompactRepr> Drop for CompactDrain<'_, T> {
    fn drop(&mut self) {
        // Restore the pre-drain length, shift the tail [drain_end, old_len)
        // down to [drain_start, ...), then truncate to the final length.
        // Uses the ORIGINAL window so this runs even after the iterator was
        // fully (or partially) consumed; `truncate` also re-tightens the
        // backing words.
        let drain_len = self.drain_end - self.drain_start;
        let tail = self.old_len - self.drain_end;
        // SAFETY: every lane `< old_len` is initialized and its word stayed
        // alive while the length was lowered (`set_len` never trims words).
        unsafe {
            self.packed.set_len(self.old_len);
        }
        if drain_len > 0 {
            // Word-level tail shift over the drained gap.
            self.packed
                .copy_lanes(self.drain_end, self.drain_start, tail);
        }
        self.packed.truncate(self.old_len - drain_len);
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
        // Non-overflowing ceiling division (`r + chunk_size` can wrap for a
        // huge chunk size).
        let r = self.slice.len.saturating_sub(self.pos);
        let c = r / self.chunk_size + usize::from(r % self.chunk_size != 0);
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
        // Non-overflowing ceiling division (see `CompactChunks`).
        let r = self.len.saturating_sub(self.pos);
        let c = r / self.chunk_size + usize::from(r % self.chunk_size != 0);
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
            crate::SoAIndex::get(0..self.end.saturating_add(1), s)
        }
    }
    #[inline]
    unsafe fn get_unchecked(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        crate::SoAIndex::get_unchecked(0..self.end.saturating_add(1), s)
    }
    #[inline]
    fn index(self, s: CompactSlice<'a, T>) -> Self::RefOutput {
        crate::SoAIndex::index(0..self.end.saturating_add(1), s)
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
            crate::SoAIndexMut::get_mut(0..self.end.saturating_add(1), s)
        }
    }
    #[inline]
    unsafe fn get_unchecked_mut(
        self,
        s: CompactSliceMut<'a, T>,
    ) -> Self::MutOutput {
        crate::SoAIndexMut::get_unchecked_mut(0..self.end.saturating_add(1), s)
    }
    #[inline]
    fn index_mut(self, s: CompactSliceMut<'a, T>) -> Self::MutOutput {
        crate::SoAIndexMut::index_mut(0..self.end.saturating_add(1), s)
    }
}
