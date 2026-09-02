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
    // trailing words holding stale lanes. The bulk appends re-tighten the
    // store before copying, so a slack store stays fully usable.
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
        self.extend_from_packed(other, 0, other_len);
        other.clear();
    }

    /// Append `other`'s lanes `[start, start + len)` to the end.
    ///
    /// When the destination tail and `start` are both word-aligned the packed
    /// words are copied wholesale (a `Vec<usize>` memcpy); any other
    /// alignment is copied a word at a time (see `copy_bits_between`).
    /// `other` must not alias `self`.
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
        let new_len = match self.len.checked_add(len) {
            Some(new_len) => new_len,
            None => capacity_overflow(),
        };
        // Drop any stale words a leaked drain left behind, so the tail word
        // is the last word and the copies below land right after it.
        self.words.truncate(Self::words_for(self.len));
        if self.len % per == 0 && start % per == 0 {
            // Both stores pack lanes at identical bit offsets within each
            // word, so the source words copy in verbatim. Stale bits past
            // `start + len` in the last word land beyond the new length, so
            // they stay invisible.
            let first = start / per;
            self.words.extend_from_slice(
                &other.words[first..first + Self::words_for(len)],
            );
        } else {
            let b = BITS as usize;
            self.words.resize(Self::words_for(new_len), 0);
            copy_bits_between(
                &other.words,
                start * b,
                &mut self.words,
                self.len * b,
                len * b,
            );
        }
        self.len = new_len;
    }

    /// Append every lane yielded by `lanes` (each truncated to `BITS` bits).
    ///
    /// Lanes are packed into a register and each completed word is stored
    /// once, so a lane costs a shift and an or instead of the read-modify-
    /// write of the tail word that [`push`](Self::push) performs.
    pub fn extend_lanes<I: IntoIterator<Item = usize>>(&mut self, lanes: I) {
        let mut lanes = lanes.into_iter();
        let per = Self::items_per_word();
        let mask = Self::mask();
        self.reserve(lanes.size_hint().0);
        // Complete the partial tail word lane by lane.
        while self.len % per != 0 {
            match lanes.next() {
                Some(v) => self.push(v),
                None => return,
            }
        }
        // The tail is word-aligned: drop any stale words a leaked drain left
        // behind, so every word below is appended at `words.len()`.
        self.words.truncate(Self::words_for(self.len));
        let mut word = 0usize;
        let mut filled = 0usize;
        for v in lanes {
            word |= (v & mask) << (filled * BITS as usize);
            filled += 1;
            if filled == per {
                self.words.push(word);
                self.len += per;
                word = 0;
                filled = 0;
            }
        }
        if filled != 0 {
            self.words.push(word);
            self.len += filled;
        }
    }

    /// Append `count` lanes all equal to `value` (truncated to `BITS` bits).
    pub fn extend_fill(&mut self, value: usize, count: usize) {
        if count == 0 {
            return;
        }
        let start = self.len;
        let new_len = match start.checked_add(count) {
            Some(new_len) => new_len,
            None => capacity_overflow(),
        };
        // `resize` drops any stale words a leaked drain left behind and
        // zero-fills the new ones; the lanes are then written at word speed.
        self.words.resize(Self::words_for(new_len), 0);
        self.len = new_len;
        self.fill_range(start, count, value);
    }

    /// Overwrite lanes `[start, start + count)` with `other`'s lanes
    /// `[other_start, other_start + count)`, a word at a time. `other` must
    /// not alias `self`.
    pub fn copy_from_packed(
        &mut self,
        other: &Self,
        other_start: usize,
        start: usize,
        count: usize,
    ) {
        assert!(
            other_start <= other.len
                && count <= other.len - other_start
                && start <= self.len
                && count <= self.len - start,
            "copy range out of bounds: the source len is {} and the range is \
             {other_start}..{other_start}+{count}, the destination len is {} \
             and the range is {start}..{start}+{count}",
            other.len,
            self.len
        );
        let b = BITS as usize;
        copy_bits_between(
            &other.words,
            other_start * b,
            &mut self.words,
            start * b,
            count * b,
        );
    }

    /// Overwrite lanes `[start, start + len)` with `value` (truncated to
    /// `BITS` bits).
    ///
    /// Interior words are stored wholesale with the lane-replicated value;
    /// the two boundary words (at most) are masked read-modify-writes, so
    /// this runs at memset speed for long ranges.
    pub fn fill_range(&mut self, start: usize, len: usize, value: usize) {
        assert!(
            start <= self.len && len <= self.len - start,
            "fill range out of bounds: the len is {} but the range is \
             {start}..{start}+{len}",
            self.len
        );
        if len == 0 {
            return;
        }
        let mask = Self::mask();
        // Lane-replicated fill word (mask divides usize::MAX exactly for
        // BITS in {1,2,4}).
        let rep = (value & mask).wrapping_mul(usize::MAX / mask);
        let end = start + len;
        let mut wi = Self::word_of(start);
        let last = Self::word_of(end - 1);
        let head_lo = Self::offset_of(start);
        let tail_hi = Self::offset_of(end - 1) + BITS as usize;
        let word_bits = usize::BITS as usize;
        if wi == last {
            // The whole range sits in one word.
            let m = bit_span_mask(head_lo, tail_hi);
            let w = &mut self.words[wi];
            *w = (*w & !m) | (rep & m);
            return;
        }
        if head_lo != 0 {
            // Partial head word.
            let m = bit_span_mask(head_lo, word_bits);
            let w = &mut self.words[wi];
            *w = (*w & !m) | (rep & m);
            wi += 1;
        }
        // Fully covered interior words (including the last word when the
        // range ends exactly on its boundary).
        let interior_end = if tail_hi == word_bits { last + 1 } else { last };
        for w in &mut self.words[wi..interior_end] {
            *w = rep;
        }
        if tail_hi != word_bits {
            // Partial tail word.
            let m = bit_span_mask(0, tail_hi);
            let w = &mut self.words[last];
            *w = (*w & !m) | (rep & m);
        }
    }

    /// Move `count` lanes from `src` to `dst` within the store. The ranges
    /// may overlap (memmove semantics): the copy direction follows the
    /// offsets so no lane is read after being overwritten.
    ///
    /// Runs a destination word at a time (see `copy_bits_within`), so a
    /// shift of a long tail costs `count / lanes_per_word` funnel shifts
    /// instead of `count` lane read-modify-writes.
    pub fn copy_lanes(&mut self, src: usize, dst: usize, count: usize) {
        assert!(
            src <= self.len
                && count <= self.len - src
                && dst <= self.len
                && count <= self.len - dst,
            "copy range out of bounds: the len is {} but the ranges are \
             {src}..{src}+{count} and {dst}..{dst}+{count}",
            self.len
        );
        let b = BITS as usize;
        copy_bits_within(&mut self.words, src * b, dst * b, count * b);
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
            // Unaligned: compare up to a word's worth of bits per step,
            // reading each side at its own offset.
            let b = BITS as usize;
            let word_bits = usize::BITS as usize;
            let (sbit, obit) = (start * b, other_start * b);
            let total = len * b;
            let mut done = 0;
            while done < total {
                let n = word_bits.min(total - done);
                if read_bits(&self.words, sbit + done, n)
                    != read_bits(&other.words, obit + done, n)
                {
                    return false;
                }
                done += n;
            }
            return true;
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
    /// Every word is counted whole: for `BITS == 1` with a plain
    /// `count_ones`/`count_zeros`, for wider lanes with the SWAR match in
    /// [`count_word_in`]. The lanes of the two boundary words that fall
    /// outside the range are first replaced by a value that cannot match, so
    /// no lane is ever extracted on its own.
    pub fn count_in(&self, start: usize, len: usize, value: usize) -> usize {
        assert!(
            start <= self.len && len <= self.len - start,
            "count range out of bounds: the len is {} but the range is \
             {start}..{start}+{len}",
            self.len
        );
        if len == 0 {
            return 0;
        }
        let mask = Self::mask();
        let value = value & mask;
        // `value ^ mask` differs from `value` in every lane; replicated into
        // the out-of-range lanes it makes them count as misses. (`mask`
        // divides `usize::MAX` exactly for BITS in {1, 2, 4}.)
        let filler = (value ^ mask).wrapping_mul(usize::MAX / mask);
        let end = start + len;
        let first = Self::word_of(start);
        let last = Self::word_of(end - 1);
        let head_lo = Self::offset_of(start);
        let tail_hi = Self::offset_of(end - 1) + BITS as usize;
        let word_bits = usize::BITS as usize;
        if first == last {
            let keep = bit_span_mask(head_lo, tail_hi);
            let w = (self.words[first] & keep) | (filler & !keep);
            return count_word_in::<BITS>(w, value);
        }
        let keep = bit_span_mask(head_lo, word_bits);
        let head = (self.words[first] & keep) | (filler & !keep);
        let keep = bit_span_mask(0, tail_hi);
        let tail = (self.words[last] & keep) | (filler & !keep);
        let mut total = count_word_in::<BITS>(head, value)
            + count_word_in::<BITS>(tail, value);
        for &w in &self.words[first + 1..last] {
            total += count_word_in::<BITS>(w, value);
        }
        total
    }
}

/// Report a length that does not fit in `usize`, like `alloc` does.
#[cold]
#[inline(never)]
fn capacity_overflow() -> ! {
    panic!("capacity overflow")
}

/// Bit mask covering word bits `[lo, hi)`, `0 <= lo < hi <= usize::BITS`.
#[inline(always)]
fn bit_span_mask(lo: usize, hi: usize) -> usize {
    let width = hi - lo;
    if width == usize::BITS as usize {
        usize::MAX
    } else {
        ((1usize << width) - 1) << lo
    }
}

/// Read `n <= usize::BITS` bits starting at absolute bit position `bit`,
/// straddling at most two words.
#[inline(always)]
fn read_bits(words: &[usize], bit: usize, n: usize) -> usize {
    let word_bits = usize::BITS as usize;
    let w = bit / word_bits;
    let r = bit % word_bits;
    let val = if r == 0 {
        words[w]
    } else if r + n <= word_bits {
        words[w] >> r
    } else {
        (words[w] >> r) | (words[w + 1] << (word_bits - r))
    };
    if n == word_bits {
        val
    } else {
        val & ((1usize << n) - 1)
    }
}

/// Replace bits `[lo, hi)` of `word` with the low `hi - lo` bits of `val`.
#[inline(always)]
fn merge_bits(word: usize, val: usize, lo: usize, hi: usize) -> usize {
    let m = bit_span_mask(lo, hi);
    (word & !m) | ((val << lo) & m)
}

/// Move `nbits` bits from bit position `src` to bit position `dst` inside
/// `words`. The ranges may overlap (memmove semantics); both must lie within
/// the slice.
///
/// Works a destination word at a time: the first and last words are masked
/// read-modify-writes, every word in between is one funnel shift of two
/// source words stored whole. Destination words are visited in ascending
/// order when moving down and descending order when moving up, so every
/// write trails the source bits still to be read.
fn copy_bits_within(words: &mut [usize], src: usize, dst: usize, nbits: usize) {
    if nbits == 0 || src == dst {
        return;
    }
    let wb = usize::BITS as usize;
    let first = dst / wb;
    let last = (dst + nbits - 1) / wb;
    let lo = dst % wb;
    let hi = (dst + nbits - 1) % wb + 1;
    if first == last {
        let v = read_bits(words, src, nbits);
        words[first] = merge_bits(words[first], v, lo, hi);
        return;
    }
    // Source bit position of the first bit of destination word `first + 1`;
    // every later word's source is one word further on, so the in-word
    // offset `r` is shared by the whole interior.
    let s0 = src + (first + 1) * wb - dst;
    let (j0, r) = (s0 / wb, s0 % wb);
    let head = |words: &mut [usize]| {
        let v = read_bits(words, src, wb - lo);
        words[first] = merge_bits(words[first], v, lo, wb);
    };
    let tail = |words: &mut [usize]| {
        let v = read_bits(words, src + nbits - hi, hi);
        words[last] = merge_bits(words[last], v, 0, hi);
    };
    let middle = |words: &mut [usize], w: usize| {
        let j = j0 + (w - first - 1);
        words[w] = if r == 0 {
            words[j]
        } else {
            (words[j] >> r) | (words[j + 1] << (wb - r))
        };
    };
    if dst < src {
        head(words);
        for w in first + 1..last {
            middle(words, w);
        }
        tail(words);
    } else {
        tail(words);
        for w in (first + 1..last).rev() {
            middle(words, w);
        }
        head(words);
    }
}

/// Copy `nbits` bits from bit position `src_bit` of `src` to bit position
/// `dst_bit` of `dst`, a destination word at a time as in
/// [`copy_bits_within`]. Both ranges must lie within their slices.
fn copy_bits_between(
    src: &[usize],
    src_bit: usize,
    dst: &mut [usize],
    dst_bit: usize,
    nbits: usize,
) {
    if nbits == 0 {
        return;
    }
    let wb = usize::BITS as usize;
    let first = dst_bit / wb;
    let last = (dst_bit + nbits - 1) / wb;
    let lo = dst_bit % wb;
    let hi = (dst_bit + nbits - 1) % wb + 1;
    if first == last {
        let v = read_bits(src, src_bit, nbits);
        dst[first] = merge_bits(dst[first], v, lo, hi);
        return;
    }
    let v = read_bits(src, src_bit, wb - lo);
    dst[first] = merge_bits(dst[first], v, lo, wb);
    let s0 = src_bit + (first + 1) * wb - dst_bit;
    let (j0, r) = (s0 / wb, s0 % wb);
    if r == 0 {
        dst[first + 1..last].copy_from_slice(&src[j0..j0 + (last - first - 1)]);
    } else {
        for (i, w) in dst[first + 1..last].iter_mut().enumerate() {
            let j = j0 + i;
            *w = (src[j] >> r) | (src[j + 1] << (wb - r));
        }
    }
    let v = read_bits(src, src_bit + nbits - hi, hi);
    dst[last] = merge_bits(dst[last], v, 0, hi);
}

/// Count the lanes of a fully-valid `word` equal to `value`.
///
/// Public (hidden) so the SWAR correctness gate in `tests/bitpack.rs` can
/// drive it directly against an independent oracle.
#[doc(hidden)]
#[inline]
pub fn count_word_in<const BITS: u32>(word: usize, value: usize) -> usize {
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
        // `count_word_in` gate in `tests/bitpack.rs`.
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
    /// Bits per lane in this store. A compact column derives its own lane
    /// arithmetic from [`CompactRepr::BITS`](crate::CompactRepr::BITS), so the
    /// two must agree; the column asserts it at compile time.
    const BITS: u32;

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
    /// Append every lane yielded by `lanes`.
    fn extend_lanes<I: IntoIterator<Item = usize>>(&mut self, lanes: I);
    /// Append `count` lanes all equal to `value`.
    fn extend_fill(&mut self, value: usize, count: usize);
    /// Overwrite lanes `[start, start + count)` with `other`'s lanes at
    /// `other_start`.
    fn copy_from_packed(
        &mut self,
        other: &Self,
        other_start: usize,
        start: usize,
        count: usize,
    );
    /// Overwrite lanes `[start, start + len)` with `value`.
    fn fill_range(&mut self, start: usize, len: usize, value: usize);
    /// Move `count` lanes from `src` to `dst` (overlap allowed).
    fn copy_lanes(&mut self, src: usize, dst: usize, count: usize);
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
    const BITS: u32 = BITS;

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
    fn extend_lanes<I: IntoIterator<Item = usize>>(&mut self, lanes: I) {
        PackedArray::extend_lanes(self, lanes);
    }
    #[inline]
    fn extend_fill(&mut self, value: usize, count: usize) {
        PackedArray::extend_fill(self, value, count);
    }
    #[inline]
    fn copy_from_packed(
        &mut self,
        other: &Self,
        other_start: usize,
        start: usize,
        count: usize,
    ) {
        PackedArray::copy_from_packed(self, other, other_start, start, count);
    }
    #[inline]
    fn fill_range(&mut self, start: usize, len: usize, value: usize) {
        PackedArray::fill_range(self, start, len, value);
    }
    #[inline]
    fn copy_lanes(&mut self, src: usize, dst: usize, count: usize) {
        PackedArray::copy_lanes(self, src, dst, count);
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
