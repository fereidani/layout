//! Compact bit-packed array storage.
//!
//! [`PackedArray`] stores a sequence of small unsigned integers (`0..2^BITS`)
//! packed into a sequence of `usize` "words", so that each element occupies
//! exactly `BITS` bits. It is the backing store used by [`crate::Compact`]
//! (1 bit per element for `Compact<bool>`) and by compact enum columns
//! (2 / 4 bits per element).
//!
//! Only `1`, `2` and `4` are supported. Each divides `usize::BITS` evenly,
//! so no element ever straddles a word boundary and reading or writing a
//! single element touches exactly one word. Wider widths are rejected: at 8
//! bits and above a packed column is byte-identical to a plain column but
//! adds encode/decode overhead, so compaction buys nothing (the
//! [`CompactRepr`](crate::CompactRepr) derive caps at 4 bits for the same
//! reason).

use alloc::vec::Vec;

/// A growable array packing `BITS`-wide unsigned values into `usize` words.
///
/// Valid widths are `1`, `2` and `4` (each divides `usize::BITS` evenly).
/// Values are truncated to `BITS` bits on insertion.
///
/// Any other width is rejected at build time:
///
/// ```compile_fail
/// // `BITS = 0` would divide by zero on the first access.
/// let a = layout::bitpack::PackedArray::<0>::new();
/// ```
///
/// ```compile_fail
/// // `BITS = 64` would overflow the value mask and truncate every
/// // element to zero.
/// let a = layout::bitpack::PackedArray::<64>::new();
/// ```
///
/// This type is `no_std` compatible (it only relies on `alloc::vec::Vec`).
#[derive(Debug)]
pub struct PackedArray<const BITS: u32> {
    // Invariant: `words.len() >= words_for(len)`, and every lane `< len` holds
    // a previously written value. The two lengths are normally tight
    // (`words.len() == words_for(len)`); a lowered [`set_len`] (used by the
    // drain machinery, and left behind by a leaked drain) may leave extra
    // trailing words holding stale lanes. Bulk word-copy fast paths check for
    // tightness and fall back to lane-at-a-time copies, so a slack store stays
    // fully usable.
    /// [`set_len`]: PackedArray::set_len
    words: Vec<usize>,
    len: usize,
}

impl<const BITS: u32> PackedArray<BITS> {
    /// Number of elements stored in a single `usize` word.
    #[inline(always)]
    fn items_per_word() -> usize {
        (usize::BITS / BITS) as usize
    }

    /// Bit mask covering the low `BITS` bits of a word.
    #[inline(always)]
    fn mask() -> usize {
        // `WIDTH_OK` guarantees BITS is one of {1, 2, 4}, all strictly less
        // than usize::BITS, so `1 << BITS` never overflows.
        (1usize << BITS) - 1
    }

    /// Index of the word holding element `index`.
    #[inline(always)]
    fn word_of(index: usize) -> usize {
        index / Self::items_per_word()
    }

    /// Bit offset of element `index` within its word.
    #[inline(always)]
    fn offset_of(index: usize) -> usize {
        (index % Self::items_per_word()) * BITS as usize
    }

    /// Number of words required to hold `len` elements.
    #[inline(always)]
    fn words_for(len: usize) -> usize {
        if len == 0 {
            0
        } else {
            Self::word_of(len - 1) + 1
        }
    }

    /// Evaluated from every constructor, so an unsupported width fails to
    /// build instead of dividing by zero (`BITS = 0`) or overflowing the
    /// mask shift and silently truncating every value (`BITS = usize::BITS`)
    /// in release.
    const WIDTH_OK: () = assert!(
        BITS == 1 || BITS == 2 || BITS == 4,
        "PackedArray BITS must be 1, 2 or 4"
    );

    /// Create an empty array.
    #[inline]
    pub fn new() -> Self {
        let () = Self::WIDTH_OK;
        Self {
            words: Vec::new(),
            len: 0,
        }
    }

    /// Create an empty array with capacity for at least `capacity` elements.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        let () = Self::WIDTH_OK;
        Self {
            words: Vec::with_capacity(Self::words_for(capacity)),
            len: 0,
        }
    }

    /// Number of elements stored.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the array holds no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Total number of elements that can be stored without reallocating.
    #[inline]
    pub fn capacity(&self) -> usize {
        // Saturate rather than wrap: at absurd word capacities the true
        // element capacity is unreachable anyway, and wrapping could report a
        // misleadingly small number.
        self.words.capacity().saturating_mul(Self::items_per_word())
    }

    /// Reserve capacity for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        let target_words = Self::words_for(self.len.saturating_add(additional));
        if target_words > self.words.capacity() {
            self.words.reserve(target_words - self.words.len());
        }
    }

    /// Reserve exactly the minimal capacity for `additional` more elements.
    pub fn reserve_exact(&mut self, additional: usize) {
        let target_words = Self::words_for(self.len.saturating_add(additional));
        if target_words > self.words.capacity() {
            self.words.reserve_exact(target_words - self.words.len());
        }
    }

    /// Shrink the allocated capacity to fit the current length.
    pub fn shrink_to_fit(&mut self) {
        // Drop any stale words a leaked drain left behind, then release the
        // spare allocation.
        self.words.truncate(Self::words_for(self.len));
        self.words.shrink_to_fit();
    }

    /// Set the logical length without touching the backing words.
    ///
    /// Used by the drain machinery to mirror `Vec::drain`'s leak safety: the
    /// length drops to the drain start up front while the drained lanes stay
    /// alive in the words, so a leaked drain leaves a short but fully
    /// consistent store.
    ///
    /// # Safety
    ///
    /// `words_for(new_len)` must not exceed `words.len()`, and every lane
    /// `< new_len` must hold a previously written value.
    #[inline]
    pub unsafe fn set_len(&mut self, new_len: usize) {
        debug_assert!(Self::words_for(new_len) <= self.words.len());
        self.len = new_len;
    }

    /// Read the element at `index` as a `usize`.
    ///
    /// # Panics
    ///
    /// Panics if `index >= self.len()`.
    #[inline]
    pub fn get(&self, index: usize) -> usize {
        assert!(
            index < self.len,
            "index out of bounds: the len is {} but the index is {}",
            self.len,
            index
        );
        // SAFETY: `index < self.len` implies the lane's word is in bounds.
        unsafe { self.get_unchecked(index) }
    }

    /// Write `value` (truncated to `BITS` bits) to the element at `index`.
    ///
    /// # Panics
    ///
    /// Panics if `index >= self.len()`.
    #[inline]
    pub fn set(&mut self, index: usize, value: usize) {
        assert!(
            index < self.len,
            "index out of bounds: the len is {} but the index is {}",
            self.len,
            index
        );
        // SAFETY: `index < self.len` implies the lane's word is in bounds.
        unsafe { self.set_unchecked(index, value) }
    }

    /// Read the element at `index` without any bounds checking.
    ///
    /// # Safety
    ///
    /// The lane's word must be allocated (`index / items_per_word <
    /// words.len()`) and the lane must hold a previously written value.
    /// Callers normally guarantee both via `index < self.len()`; the drain
    /// machinery also reads initialized lanes beyond the current length while
    /// the backing words are kept alive.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: usize) -> usize {
        debug_assert!(
            Self::word_of(index) < self.words.len(),
            "lane {index} has no backing word"
        );
        // SAFETY: the caller guarantees the word exists.
        let word = unsafe { *self.words.get_unchecked(Self::word_of(index)) };
        (word >> Self::offset_of(index)) & Self::mask()
    }

    /// Write `value` (truncated to `BITS` bits) to the element at `index`
    /// without any bounds checking.
    ///
    /// # Safety
    ///
    /// The lane's word must be allocated (`index / items_per_word <
    /// words.len()`). Callers normally guarantee this via
    /// `index < self.len()`.
    #[inline(always)]
    pub unsafe fn set_unchecked(&mut self, index: usize, value: usize) {
        debug_assert!(
            Self::word_of(index) < self.words.len(),
            "lane {index} has no backing word"
        );
        let off = Self::offset_of(index);
        // SAFETY: the caller guarantees the word exists.
        let slot =
            unsafe { self.words.get_unchecked_mut(Self::word_of(index)) };
        *slot &= !(Self::mask() << off);
        *slot |= (value & Self::mask()) << off;
    }

    /// Append `value` (truncated to `BITS` bits) to the end.
    #[inline]
    pub fn push(&mut self, value: usize) {
        let off = Self::offset_of(self.len);
        let bits = (value & Self::mask()) << off;
        if let Some(w) = self.words.get_mut(Self::word_of(self.len)) {
            // The target word exists: a partial tail word, or a stale word a
            // leaked drain left behind. Clear the slot (it may hold stale
            // bits) and set it, touching the word once.
            *w = (*w & !(Self::mask() << off)) | bits;
        } else {
            // Fresh word. The target word can only be missing when the store
            // is tight and the length is word-aligned, so `off == 0` and the
            // value lands in the low lane directly.
            debug_assert!(off == 0);
            self.words.push(bits);
        }
        self.len += 1;
    }

    /// Remove and return the last element, if any.
    #[inline]
    pub fn pop(&mut self) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        // SAFETY: `self.len - 1 < self.len`.
        let value = unsafe { self.get_unchecked(self.len - 1) };
        self.len -= 1;
        // Symmetric with push: drop the now-empty tail word (and any stale
        // words a leaked drain left behind).
        self.words.truncate(Self::words_for(self.len));
        Some(value)
    }

    /// Shorten the array to `new_len`, discarding trailing elements.
    pub fn truncate(&mut self, new_len: usize) {
        if new_len >= self.len {
            return;
        }
        self.len = new_len;
        self.words.truncate(Self::words_for(new_len));
    }

    /// Remove all elements.
    pub fn clear(&mut self) {
        self.words.clear();
        self.len = 0;
    }

    /// Move all elements from `other` to the end of `self`, leaving `other`
    /// empty.
    pub fn append(&mut self, other: &mut Self) {
        let other_len = other.len;
        if other_len == 0 {
            return;
        }
        let per = Self::items_per_word();
        self.reserve(other_len);
        if self.len % per == 0 && self.words.len() == Self::words_for(self.len)
        {
            // Word-aligned tight tail: both stores pack lanes at identical
            // bit offsets within each word, so `other`'s packed words copy in
            // verbatim. Only the words holding `other`'s valid lanes are
            // copied (a leaked drain may have left `other` with extra stale
            // words), and any stale bits beyond `other_len` in the last word
            // land beyond `self`'s new length, so they stay invisible. No
            // per-element `push`, so advance the length explicitly.
            self.words
                .extend_from_slice(&other.words[..Self::words_for(other_len)]);
            self.len += other_len;
        } else {
            // Unaligned or slack tail (stale words after a leaked drain):
            // merge element by element; `push` advances the length itself.
            for i in 0..other_len {
                // SAFETY: `i < other_len == other.len`.
                self.push(unsafe { other.get_unchecked(i) });
            }
        }
        other.clear();
    }

    /// Append `other`'s lanes `[start, start + len)` to the end.
    ///
    /// When the destination tail and `start` are both word-aligned the packed
    /// words are copied wholesale (a `Vec<usize>` memcpy); otherwise lanes are
    /// copied one at a time. `other` must not alias `self`.
    pub fn extend_from_packed(
        &mut self,
        other: &Self,
        start: usize,
        len: usize,
    ) {
        assert!(
            start <= other.len && len <= other.len - start,
            "source range out of bounds: the len is {} but the range is \
             {start}..{start}+{len}",
            other.len
        );
        if len == 0 {
            return;
        }
        let per = Self::items_per_word();
        self.reserve(len);
        if self.len % per == 0
            && start % per == 0
            && self.words.len() == Self::words_for(self.len)
        {
            // Aligned tight tail: copy whole words. The source words exist
            // even if `other` carries stale trailing words, and stale bits
            // past `start + len` in the last word land beyond `self`'s new
            // length, so stay invisible.
            let first = start / per;
            let nwords = Self::words_for(len);
            self.words
                .extend_from_slice(&other.words[first..first + nwords]);
            self.len += len;
        } else {
            for i in 0..len {
                // SAFETY: `start + i < start + len <= other.len` (asserted
                // above).
                self.push(unsafe { other.get_unchecked(start + i) });
            }
        }
    }

    /// Append `count` lanes all equal to `value` (truncated to `BITS` bits).
    pub fn extend_fill(&mut self, value: usize, count: usize) {
        if count == 0 {
            return;
        }
        let per = Self::items_per_word();
        let mask = Self::mask();
        let v = value & mask;
        self.reserve(count);
        let mut filled = 0;
        // Finish the current partial word one lane at a time.
        while filled < count && self.len % per != 0 {
            self.push(v);
            filled += 1;
        }
        // Bulk full words: `v` replicated into every lane (mask divides
        // usize::MAX exactly for BITS in {1,2,4}). Requires a tight store;
        // a slack one (stale words after a leaked drain) falls through to the
        // per-element loop below.
        let full_words = (count - filled) / per;
        if full_words > 0 && self.words.len() == Self::words_for(self.len) {
            let rep = v.wrapping_mul(usize::MAX / mask);
            self.words.resize(self.words.len() + full_words, rep);
            self.len += full_words * per;
            filled += full_words * per;
        }
        while filled < count {
            self.push(v);
            filled += 1;
        }
    }

    /// Whether `self`'s lanes `[start, start + len)` equal `other`'s lanes
    /// `[other_start, other_start + len)`.
    ///
    /// Word-batched (one `usize` compare per `per` lanes) when both starts are
    /// word-aligned; otherwise compared lane by lane. Full words hold only
    /// valid lanes, so they compare verbatim; the tail word is masked to its
    /// valid lanes so stale bits past the range do not affect the result.
    pub fn range_eq(
        &self,
        start: usize,
        other: &Self,
        other_start: usize,
        len: usize,
    ) -> bool {
        assert!(
            start <= self.len
                && len <= self.len - start
                && other_start <= other.len
                && len <= other.len - other_start,
            "compare range out of bounds"
        );
        if len == 0 {
            return true;
        }
        let per = Self::items_per_word();
        if start % per != 0 || other_start % per != 0 {
            return (0..len)
                .all(|i| self.get(start + i) == other.get(other_start + i));
        }
        let (sw, ow) = (start / per, other_start / per);
        let full = len / per;
        if self.words[sw..sw + full] != other.words[ow..ow + full] {
            return false;
        }
        let tail = len % per;
        if tail == 0 {
            return true;
        }
        let mask = (1usize << (tail * BITS as usize)) - 1;
        (self.words[sw + full] & mask) == (other.words[ow + full] & mask)
    }

    /// Count elements whose stored value equals `value`, over
    /// `[start, start+len)`.
    ///
    /// Bulk words are counted without per-element extraction; for `BITS == 1`
    /// this is a direct `count_ones`/`count_zeros` over the packed words
    /// (auto-vectorizable). Boundary words (at most two) fall back to
    /// per-element extraction. Multi-bit widths count lanes within each word.
    pub fn count_in(&self, start: usize, len: usize, value: usize) -> usize {
        assert!(
            start <= self.len && len <= self.len - start,
            "count range out of bounds: the len is {} but the range is \
             {start}..{start}+{len}",
            self.len
        );
        let per = Self::items_per_word();
        let mut total = 0usize;
        let mut idx = start;
        let end = start + len;
        while idx < end {
            let wstart = (idx / per) * per;
            let wend = wstart + per;
            if idx == wstart && wend <= end {
                // Full word: all lanes are valid.
                total += count_word_in::<BITS>(self.words[wstart / per], value);
                idx = wend;
            } else {
                // Partial boundary word: extract element by element.
                let stop = wend.min(end);
                for i in idx..stop {
                    // SAFETY: `i < end <= self.len` (checked on entry).
                    if unsafe { self.get_unchecked(i) } == value {
                        total += 1;
                    }
                }
                idx = stop;
            }
        }
        total
    }
}

/// Count the lanes of a fully-valid `word` equal to `value`.
#[inline]
fn count_word_in<const BITS: u32>(word: usize, value: usize) -> usize {
    if BITS == 1 {
        // 1-bit lanes: a lane is a single bit. `value` is 0 or 1.
        if value != 0 {
            word.count_ones() as usize
        } else {
            word.count_zeros() as usize
        }
    } else {
        // SWAR lane-equality count (auto-vectorizes in `count_in`).
        // Borrow-based zero detectors (`(v - 0x01..) & !v & 0x80..`)
        // are unsafe here: a borrow out of a zero lane corrupts a
        // small-valued neighbour's indicator. Instead: XOR with the
        // replicated target, collapse each lane to one indicator bit,
        // move it to the lane's high bit, invert and mask to one bit
        // per matching lane, then `count_ones`. `rep1` = usize::MAX
        // / mask gives a 1 in bit 0 of every lane. Covered by the
        // `count_word_in_*` tests below.
        let mask = (1usize << BITS) - 1;
        let rep1 = usize::MAX / mask;
        // A 1 in the high bit (BITS-1) of every lane.
        let highrep = rep1 << (BITS - 1);
        let target = value & mask;
        // Replicate target into every lane. target < 2^BITS, REP1 has 1s spaced
        // BITS apart, so this multiply produces no inter-lane carry.
        let rep_target = target.wrapping_mul(rep1);
        let u = word ^ rep_target;

        // Collapse each lane to bit 0 = "lane has any set bit" (lane nonzero).
        let mut collapsed = u & rep1;
        let mut j: u32 = 1;
        while j < BITS {
            collapsed |= (u >> j) & rep1;
            j += 1;
        }
        // Move the lane-nonzero indicator to the high bit, invert, keep only
        // the per-lane high-bit positions: one bit per matching lane.
        let nz_high = collapsed << (BITS - 1);
        let matches = !nz_high & highrep;
        matches.count_ones() as usize
    }
}

impl<const BITS: u32> Default for PackedArray<BITS> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<const BITS: u32> Clone for PackedArray<BITS> {
    fn clone(&self) -> Self {
        Self {
            words: self.words.clone(),
            len: self.len,
        }
    }
}

// No custom `Drop`: `usize` words have no destructor, so `Vec`'s automatic
// drop frees the allocation with no extra work.

// ---------------------------------------------------------------------------
// BitPack - the method surface a generic compact column drives its storage
// through. Routing storage to a compact column via this associated trait
// (rather than `PackedArray<{ T::BITS }>`) keeps the design on stable Rust,
// which forbids generic associated consts as const generic arguments.
// ---------------------------------------------------------------------------

/// The `Vec`-shaped method surface a compact column needs from its backing
/// bit-packed storage.
///
/// Implemented for every valid [`PackedArray<BITS>`]. The generic compact
/// column types hold a `<T as crate::CompactRepr>::Storage` and operate on it
/// purely through this trait, so the column code is independent of the exact
/// bit width.
pub trait BitPack: Clone + Default + core::fmt::Debug + Sized {
    /// Create an empty store.
    fn new() -> Self;
    /// Create an empty store with capacity for at least `capacity` elements.
    fn with_capacity(capacity: usize) -> Self;
    /// Number of elements stored.
    fn len(&self) -> usize;
    /// Whether the store holds no elements.
    fn is_empty(&self) -> bool;
    /// Total number of elements that can be stored without reallocating.
    fn capacity(&self) -> usize;
    /// Reserve capacity for at least `additional` more elements.
    fn reserve(&mut self, additional: usize);
    /// Reserve exactly the minimal capacity for `additional` more elements.
    fn reserve_exact(&mut self, additional: usize);
    /// Shrink the allocated capacity to fit the current length.
    fn shrink_to_fit(&mut self);
    /// Shorten the store to `new_len`, discarding trailing elements.
    fn truncate(&mut self, new_len: usize);
    /// Set the logical length without touching the backing words.
    ///
    /// # Safety
    /// Every lane `< new_len` must be backed by an allocated word and hold a
    /// previously written value.
    unsafe fn set_len(&mut self, new_len: usize);
    /// Read the element at `index` as a `usize`.
    fn get(&self, index: usize) -> usize;
    /// Read the element at `index` without bounds checking.
    ///
    /// # Safety
    /// The lane's backing word must be allocated and hold a previously
    /// written value; callers normally guarantee both via `index < len()`.
    unsafe fn get_unchecked(&self, index: usize) -> usize;
    /// Raw packed word at word-index `index` (for word-at-a-time reads).
    fn word(&self, index: usize) -> usize;
    /// Write `value` to the element at `index`.
    fn set(&mut self, index: usize, value: usize);
    /// Write `value` to the element at `index` without bounds checking.
    ///
    /// # Safety
    /// The lane's backing word must be allocated; callers normally guarantee
    /// this via `index < len()`.
    unsafe fn set_unchecked(&mut self, index: usize, value: usize);
    /// Append `value` to the end.
    fn push(&mut self, value: usize);
    /// Remove and return the last element, if any.
    fn pop(&mut self) -> Option<usize>;
    /// Remove all elements.
    fn clear(&mut self);
    /// Move all elements from `other` to the end of `self`, leaving `other`
    /// empty.
    fn append(&mut self, other: &mut Self);
    /// Append `other`'s lanes `[start, start + len)` to the end.
    fn extend_from_packed(&mut self, other: &Self, start: usize, len: usize);
    /// Append `count` lanes all equal to `value`.
    fn extend_fill(&mut self, value: usize, count: usize);
    /// Whether `self`'s lanes `[start, start + len)` equal `other`'s lanes at
    /// `other_start`.
    fn range_eq(
        &self,
        start: usize,
        other: &Self,
        other_start: usize,
        len: usize,
    ) -> bool;
    /// Count elements equal to `value` in `[start, start+len)`.
    fn count_in(&self, start: usize, len: usize, value: usize) -> usize;
}

impl<const BITS: u32> BitPack for PackedArray<BITS> {
    #[inline]
    fn new() -> Self {
        PackedArray::new()
    }
    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        PackedArray::with_capacity(capacity)
    }
    #[inline]
    fn len(&self) -> usize {
        PackedArray::len(self)
    }
    #[inline]
    fn is_empty(&self) -> bool {
        PackedArray::is_empty(self)
    }
    #[inline]
    fn capacity(&self) -> usize {
        PackedArray::capacity(self)
    }
    #[inline]
    fn reserve(&mut self, additional: usize) {
        PackedArray::reserve(self, additional);
    }
    #[inline]
    fn reserve_exact(&mut self, additional: usize) {
        PackedArray::reserve_exact(self, additional);
    }
    #[inline]
    fn shrink_to_fit(&mut self) {
        PackedArray::shrink_to_fit(self);
    }
    #[inline]
    fn truncate(&mut self, new_len: usize) {
        PackedArray::truncate(self, new_len);
    }
    #[inline]
    unsafe fn set_len(&mut self, new_len: usize) {
        // SAFETY: forwarded contract.
        unsafe { PackedArray::set_len(self, new_len) };
    }
    #[inline]
    fn get(&self, index: usize) -> usize {
        PackedArray::get(self, index)
    }
    #[inline(always)]
    unsafe fn get_unchecked(&self, index: usize) -> usize {
        // SAFETY: forwarded contract.
        unsafe { PackedArray::get_unchecked(self, index) }
    }
    #[inline]
    fn word(&self, index: usize) -> usize {
        debug_assert!(index < self.words.len());
        self.words[index]
    }
    #[inline]
    fn set(&mut self, index: usize, value: usize) {
        PackedArray::set(self, index, value);
    }
    #[inline(always)]
    unsafe fn set_unchecked(&mut self, index: usize, value: usize) {
        // SAFETY: forwarded contract.
        unsafe { PackedArray::set_unchecked(self, index, value) };
    }
    #[inline]
    fn push(&mut self, value: usize) {
        PackedArray::push(self, value);
    }
    #[inline]
    fn pop(&mut self) -> Option<usize> {
        PackedArray::pop(self)
    }
    #[inline]
    fn clear(&mut self) {
        PackedArray::clear(self);
    }
    #[inline]
    fn append(&mut self, other: &mut Self) {
        PackedArray::append(self, other);
    }
    #[inline]
    fn extend_from_packed(&mut self, other: &Self, start: usize, len: usize) {
        PackedArray::extend_from_packed(self, other, start, len);
    }
    #[inline]
    fn extend_fill(&mut self, value: usize, count: usize) {
        PackedArray::extend_fill(self, value, count);
    }
    #[inline]
    fn range_eq(
        &self,
        start: usize,
        other: &Self,
        other_start: usize,
        len: usize,
    ) -> bool {
        PackedArray::range_eq(self, start, other, other_start, len)
    }
    #[inline]
    fn count_in(&self, start: usize, len: usize, value: usize) -> usize {
        PackedArray::count_in(self, start, len, value)
    }
}

#[cfg(test)]
mod tests {
    use super::{count_word_in, PackedArray};

    #[test]
    fn one_bit_roundtrip() {
        let mut a = PackedArray::<1>::new();
        let pattern = [0, 1, 1, 0, 1, 0, 0, 1, 1, 1];
        for &v in pattern.iter() {
            a.push(v);
        }
        assert_eq!(a.len(), pattern.len());
        for (i, &v) in pattern.iter().enumerate() {
            assert_eq!(a.get(i), v, "mismatch at {i}");
        }
        assert_eq!(a.pop(), Some(1));
        assert_eq!(a.pop(), Some(1));
        assert_eq!(a.len(), pattern.len() - 2);
    }

    #[test]
    fn four_bit_roundtrip() {
        let mut a = PackedArray::<4>::with_capacity(8);
        let pattern = [0, 1, 5, 15, 7, 9, 3, 14, 2];
        for &v in pattern.iter() {
            a.push(v);
        }
        for (i, &v) in pattern.iter().enumerate() {
            assert_eq!(a.get(i), v, "mismatch at {i}");
        }
    }

    #[test]
    fn truncate_then_push_clears_stale_bits() {
        // Regression: truncate must not leave stale bits that a subsequent
        // push(0) into a partially-filled word would OR against.
        let mut a = PackedArray::<4>::new(); // 16 items/word on 64-bit
        for v in [1u32, 2, 3, 4, 5] {
            a.push(v as usize); // indices 0..4, values 1..5
        }
        assert_eq!(a.len(), 5);
        a.truncate(2); // len now 2; tail word still holds stale bits for idx 2..15
        assert_eq!(a.get(0), 1);
        assert_eq!(a.get(1), 2);
        a.push(0); // idx 2 — must store 0, not the stale 3
        a.push(0); // idx 3
        a.push(0); // idx 4
        assert_eq!(a.get(2), 0, "stale bit leaked through truncate+push");
        assert_eq!(a.get(3), 0);
        assert_eq!(a.get(4), 0);
    }

    #[test]
    fn set_updates_in_place() {
        let mut a = PackedArray::<4>::new();
        for _ in 0..6 {
            a.push(0);
        }
        a.set(0, 15);
        a.set(5, 7);
        assert_eq!(a.get(0), 15);
        assert_eq!(a.get(5), 7);
        assert_eq!(a.get(3), 0);
    }

    #[test]
    fn truncate_and_append() {
        let mut a = PackedArray::<1>::new();
        for i in 0..130 {
            a.push(i % 2);
        }
        assert_eq!(a.len(), 130);
        a.truncate(64);
        assert_eq!(a.len(), 64);
        for i in 0..64 {
            assert_eq!(a.get(i), i % 2);
        }

        let mut b = PackedArray::<1>::new();
        b.push(1);
        b.push(0);
        a.append(&mut b);
        assert_eq!(a.len(), 66);
        assert_eq!(a.get(64), 1);
        assert_eq!(a.get(65), 0);
        assert!(b.is_empty());
    }

    #[test]
    fn capacity_grows() {
        let mut a = PackedArray::<4>::with_capacity(4);
        assert!(a.capacity() >= 4);
        for i in 0..200 {
            a.push(i & 0xF);
        }
        assert!(a.capacity() >= 200);
        a.shrink_to_fit();
        assert!(a.capacity() >= a.len());
    }

    // `extend_from_packed` / `extend_fill` against a per-element oracle across
    // widths and every alignment of destination tail, source start, and length.
    fn check_bulk<const B: u32>() {
        let per = (usize::BITS / B) as usize;
        let mask = (1usize << B) - 1;
        let dest_lens = [0, 1, per - 1, per, per + 1, 2 * per];

        let starts = [0, 1, per, per + 2];
        let lens = [0, 1, per - 1, per, per + 3, 2 * per + 1];
        for &dl in &dest_lens {
            for &st in &starts {
                for &ln in &lens {
                    let mut src = PackedArray::<B>::new();
                    for i in 0..(st + ln + 2) {
                        src.push(i.wrapping_mul(7).wrapping_add(1) & mask);
                    }
                    let mut dest = PackedArray::<B>::new();
                    for i in 0..dl {
                        dest.push(i.wrapping_mul(3).wrapping_add(2) & mask);
                    }
                    let mut want: Vec<usize> =
                        (0..dl).map(|i| dest.get(i)).collect();
                    for i in 0..ln {
                        want.push(src.get(st + i));
                    }
                    dest.extend_from_packed(&src, st, ln);
                    assert_eq!(dest.len(), dl + ln);
                    for (i, &w) in want.iter().enumerate() {
                        assert_eq!(
                            dest.get(i),
                            w,
                            "packed B={B} dl={dl} st={st} ln={ln} at {i}"
                        );
                    }
                }
            }
        }

        let counts = [0, 1, per - 1, per, per + 5, 2 * per];
        for &dl in &dest_lens {
            for &cnt in &counts {
                for &v in &[0usize, 1, mask, mask / 2] {
                    let mut dest = PackedArray::<B>::new();
                    for i in 0..dl {
                        dest.push(i.wrapping_mul(5).wrapping_add(1) & mask);
                    }
                    let mut want: Vec<usize> =
                        (0..dl).map(|i| dest.get(i)).collect();
                    for _ in 0..cnt {
                        want.push(v);
                    }
                    dest.extend_fill(v, cnt);
                    assert_eq!(dest.len(), dl + cnt);
                    for (i, &w) in want.iter().enumerate() {
                        assert_eq!(
                            dest.get(i),
                            w,
                            "fill B={B} dl={dl} cnt={cnt} v={v} at {i}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bulk_primitives_match_oracle() {
        check_bulk::<1>();
        check_bulk::<2>();
        check_bulk::<4>();
    }

    fn check_range_eq<const B: u32>() {
        let per = (usize::BITS / B) as usize;
        let mask = (1usize << B) - 1;
        for n in [0, 1, per - 1, per, per + 1, 2 * per + 3] {
            let mut a = PackedArray::<B>::new();
            for i in 0..n {
                a.push(i.wrapping_mul(11).wrapping_add(1) & mask);
            }
            // Same n lanes, but with junk left in the tail word past n.
            let mut b = PackedArray::<B>::new();
            for i in 0..n {
                b.push(i.wrapping_mul(11).wrapping_add(1) & mask);
            }
            for i in 0..per {
                b.push((i ^ 0x2a) & mask);
            }
            b.truncate(n);

            assert!(a.range_eq(0, &b, 0, n), "aligned B={B} n={n}");
            if n >= 2 {
                // Unaligned start takes the per-lane path.
                assert!(a.range_eq(1, &b, 1, n - 1), "unaligned B={B} n={n}");
            }
            if n > 0 {
                let mut c = a.clone();
                let last = c.get(n - 1);
                c.set(n - 1, (last + 1) & mask);
                assert!(!a.range_eq(0, &c, 0, n), "differ B={B} n={n}");
            }
        }
    }

    #[test]
    fn range_eq_ignores_stale_tail_bits() {
        check_range_eq::<1>();
        check_range_eq::<2>();
        check_range_eq::<4>();
    }

    // -----------------------------------------------------------------
    // `count_word_in` correctness gate: the multi-bit SWAR path is checked
    // bit-exact against an independent scalar oracle (fuzz + structured edge
    // cases). Gated under cfg(not(miri)).
    // -----------------------------------------------------------------

    /// Independent scalar oracle: extract each BITS-wide lane and compare.
    #[cfg(not(miri))]
    fn oracle<const BITS: u32>(word: usize, value: usize) -> usize {
        let per = (usize::BITS / BITS) as usize;
        let mask = (1usize << BITS) - 1;
        let target = value & mask;
        let mut w = word;
        let mut count = 0usize;
        for _ in 0..per {
            if (w & mask) == target {
                count += 1;
            }
            w >>= BITS;
        }
        count
    }

    /// LCG (numerical recipes "MMIX" constants) — deterministic, no `rand`.
    #[cfg(not(miri))]
    fn lcg_next(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *state
    }

    /// Build a word by filling its lanes from `lanes` (low lane first); any
    /// lane beyond `lanes.len()` is set to `fill`.
    #[cfg(not(miri))]
    fn build_word(b: u32, lanes: &[u64], fill: u64) -> usize {
        let per = (usize::BITS / b) as usize;
        let lane_mask = (1u64 << b) - 1;
        let mut w = 0u64;
        for i in 0..per {
            let v = if i < lanes.len() {
                lanes[i] & lane_mask
            } else {
                fill & lane_mask
            };
            w |= v << (i * b as usize);
        }
        w as usize
    }

    /// Target values to probe per width, including 0 and the lane mask.
    #[cfg(not(miri))]
    fn target_values(b: u32) -> Vec<usize> {
        let nvals = 1usize << b;
        let mut v: Vec<usize> = (0..nvals).collect();
        // Ensure the prompt's required targets are present (they already are
        // for the exhaustive small widths, but keep them explicit for b=8/16).
        for t in [0usize, 1, nvals / 2, nvals - 1] {
            if !v.contains(&t) {
                v.push(t);
            }
        }
        v
    }

    /// Run the full gate for one width: exhaustive low-lane enumeration (where
    /// affordable), a large LCG fuzz, and structured edge cases that stress
    /// borrow-based detectors (adjacent zero-then-small lanes).
    #[cfg(not(miri))]
    fn gate_width(b: u32) {
        let nvals = 1u64 << b;
        let lane_mask = (nvals - 1) as usize;
        let per = (usize::BITS / b) as usize;
        let targets = target_values(b);
        let mut tested = 0u64;

        // --- Exhaustive enumeration of the first 4 lanes (rest 0 and rest
        // all-ones),     for every target. Only affordable for b in {2,
        // 4}. ---
        if b == 2 || b == 4 {
            let span = nvals;
            // Enumerate first 4 lanes fully.
            for l0 in 0..span {
                for l1 in 0..span {
                    for l2 in 0..span {
                        for l3 in 0..span {
                            for &fill in &[0u64, nvals - 1] {
                                let word =
                                    build_word(b, &[l0, l1, l2, l3], fill);
                                for &t in &targets {
                                    let got = count_word_in_word(b, word, t);
                                    let exp = oracle_dyn(b, word, t);
                                    assert_eq!(
                                        got, exp,
                                        "exhaust b={} t={} word={:#x}",
                                        b, t, word
                                    );
                                    tested += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // --- LCG fuzz: >= 300_000 pseudo-deterministic words per width.
        //     For widths with many target values (b=8, b=16) the fuzz uses the
        //     required subset {0, 1, mask/2, mask}; full target coverage comes
        //     from the exhaustive / structured sections. b=2 and b=4 are cheap
        //     enough to fuzz against every target. ---
        let n = 300_005;
        let fuzz_targets: Vec<usize> = if b <= 4 {
            targets.clone()
        } else {
            vec![0, 1, lane_mask, (nvals / 2) as usize]
        };
        let mut state = 0x9E37_79B9_7F4A_7C15u64
            .wrapping_add((b as u64).wrapping_mul(2654435761));
        for _ in 0..n {
            let word = lcg_next(&mut state) as usize;
            for &t in &fuzz_targets {
                let got = count_word_in_word(b, word, t);
                let exp = oracle_dyn(b, word, t);
                assert_eq!(got, exp, "fuzz b={} t={} word={:#x}", b, t, word);
                tested += 1;
            }
        }

        // --- Structured edge cases. ---
        // all-zero and all-ones words.
        for &word in &[0usize, usize::MAX] {
            for &t in &targets {
                let got = count_word_in_word(b, word, t);
                let exp = oracle_dyn(b, word, t);
                assert_eq!(got, exp, "const b={} t={} word={:#x}", b, t, word);
                tested += 1;
            }
        }
        // all-lanes-equal-target (stresses any packing overflow).
        for &t in &targets {
            let word = build_word(b, &[], t as u64);
            let got = count_word_in_word(b, word, t);
            let exp = oracle_dyn(b, word, t);
            assert_eq!(got, exp, "allmatch b={} t={}", b, t);
            tested += 1;
        }
        // single matching lane at each position; alternating match/no-match;
        // adjacent (zero-then-small) lane patterns that stress borrow
        // detectors.
        for pos in 0..per {
            // single lane == target (target 0), rest = (mask/2)
            let fill = nvals / 2;
            let mut lanes = vec![fill; per];
            lanes[pos] = 0;
            let word = build_word(b, &lanes, fill);
            for &t in &[0usize, 1, lane_mask, (nvals / 2) as usize] {
                let got = count_word_in_word(b, word, t);
                let exp = oracle_dyn(b, word, t);
                assert_eq!(
                    got, exp,
                    "single b={} pos={} t={} word={:#x}",
                    b, pos, t, word
                );
                tested += 1;
            }
            // adjacent zero-then-small (and small-then-zero) at (pos, pos+1)
            if pos + 1 < per {
                for &small in &[0u64, 1, 2, nvals / 2, nvals - 1] {
                    let mut lanes_a = vec![fill; per];
                    lanes_a[pos] = 0;
                    lanes_a[pos + 1] = small;
                    let mut lanes_b = vec![fill; per];
                    lanes_b[pos] = small;
                    lanes_b[pos + 1] = 0;
                    for &word in &[
                        build_word(b, &lanes_a, fill),
                        build_word(b, &lanes_b, fill),
                    ] {
                        for &t in &[0usize, 1, small as usize, lane_mask] {
                            let got = count_word_in_word(b, word, t);
                            let exp = oracle_dyn(b, word, t);
                            assert_eq!(
                                got, exp,
                                "adjacent b={} pos={} small={} t={} word={:#x}",
                                b, pos, small, t, word
                            );
                            tested += 1;
                        }
                    }
                }
            }
        }
        // alternating match/no-match
        {
            let mut lanes = Vec::with_capacity(per);
            for i in 0..per {
                lanes.push(if i % 2 == 0 { 0 } else { nvals - 1 });
            }
            let word = build_word(b, &lanes, 0);
            for &t in &[0usize, 1, lane_mask] {
                let got = count_word_in_word(b, word, t);
                let exp = oracle_dyn(b, word, t);
                assert_eq!(got, exp, "alternating b={} t={}", b, t);
                tested += 1;
            }
        }

        // Sanity: ensure we actually exercised a meaningful number of cases.
        assert!(tested >= 300_000, "b={} only tested={}", b, tested);
    }

    // Dynamic (runtime-b) wrapper around the const-generic `count_word_in`.
    // Each width dispatches to its monomorphized instance.
    #[cfg(not(miri))]
    fn count_word_in_word(b: u32, word: usize, value: usize) -> usize {
        match b {
            2 => count_word_in::<2>(word, value),
            4 => count_word_in::<4>(word, value),
            _ => unreachable!("unsupported width {}", b),
        }
    }
    #[cfg(not(miri))]
    fn oracle_dyn(b: u32, word: usize, value: usize) -> usize {
        match b {
            2 => oracle::<2>(word, value),
            4 => oracle::<4>(word, value),
            _ => unreachable!("unsupported width {}", b),
        }
    }

    #[cfg(not(miri))]
    #[test]
    fn count_word_in_gate_2() {
        gate_width(2);
    }
    #[cfg(not(miri))]
    #[test]
    fn count_word_in_gate_4() {
        gate_width(4);
    }
}
