//! Compact bit-packed array storage.
//!
//! [`PackedArray`] stores a sequence of small unsigned integers (`0..2^BITS`)
//! packed into a sequence of `usize` "words", so that each element occupies
//! exactly `BITS` bits. It is the backing store used by [`crate::Compact`]
//! (1 bit per element for `Compact<bool>`) and by compact enum columns
//! (2 / 4 / 8 / 16 bits per element).
//!
//! Only widths that divide `usize::BITS` evenly are supported (`1`, `2`, `4`,
//! `8` and `16`). This guarantees that no element ever straddles a word
//! boundary, so reading or writing a single element touches exactly one word.

use alloc::vec::Vec;

/// A growable array packing `BITS`-wide unsigned values into `usize` words.
///
/// Valid widths are `1`, `2`, `4`, `8` and `16` (each divides `usize::BITS`
/// evenly). Values are truncated to `BITS` bits on insertion.
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
        // `WIDTH_OK` guarantees BITS is one of {1, 2, 4, 8, 16}, all strictly
        // less than usize::BITS, so `1 << BITS` never overflows.
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
        BITS == 1 || BITS == 2 || BITS == 4 || BITS == 8 || BITS == 16,
        "PackedArray BITS must be 1, 2, 4, 8 or 16"
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
        self.words.shrink_to_fit();
    }

    /// Read the element at `index` as a `usize`.
    #[inline]
    pub fn get(&self, index: usize) -> usize {
        debug_assert!(index < self.len, "index out of bounds");
        let word = self.words[Self::word_of(index)];
        (word >> Self::offset_of(index)) & Self::mask()
    }

    /// Write `value` (truncated to `BITS` bits) to the element at `index`.
    #[inline]
    pub fn set(&mut self, index: usize, value: usize) {
        debug_assert!(index < self.len, "index out of bounds");
        let off = Self::offset_of(index);
        let slot = &mut self.words[Self::word_of(index)];
        *slot &= !(Self::mask() << off);
        *slot |= (value & Self::mask()) << off;
    }

    /// Append `value` (truncated to `BITS` bits) to the end.
    #[inline]
    pub fn push(&mut self, value: usize) {
        let per = Self::items_per_word();
        let off = Self::offset_of(self.len);
        let word_idx = Self::word_of(self.len);
        if self.len % per == 0 {
            // Starting a fresh word: it is zeroed, so OR is sufficient.
            self.words.push(0);
        } else {
            // Pushing into a partially-filled word: clear the slot first,
            // because `truncate`/`pop` may have left stale bits here.
            self.words[word_idx] &= !(Self::mask() << off);
        }
        self.words[word_idx] |= (value & Self::mask()) << off;
        self.len += 1;
    }

    /// Remove and return the last element, if any.
    #[inline]
    pub fn pop(&mut self) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let value = self.get(self.len - 1);
        self.len -= 1;
        // Symmetric with push: drop the now-empty tail word.
        if self.len % Self::items_per_word() == 0 {
            self.words.pop();
        }
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
        self.reserve(other_len);
        for i in 0..other_len {
            self.push(other.get(i));
        }
        other.clear();
    }

    /// Count elements whose stored value equals `value`, over
    /// `[start, start+len)`.
    ///
    /// Bulk words are counted without per-element extraction; for `BITS == 1`
    /// this is a direct `count_ones`/`count_zeros` over the packed words
    /// (auto-vectorizable). Boundary words (at most two) fall back to
    /// per-element extraction. Multi-bit widths count lanes within each word.
    pub fn count_in(&self, start: usize, len: usize, value: usize) -> usize {
        debug_assert!(start.saturating_add(len) <= self.len);
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
                    if self.get(i) == value {
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
        // SWAR lane-equality count (auto-vectorizes in `count_in`). Borrow-based
        // zero detectors (`(v - 0x01..) & !v & 0x80..`) are unsafe here: a
        // borrow out of a zero lane corrupts a small-valued neighbour's
        // indicator. Instead: XOR with the replicated target, collapse each lane
        // to one indicator bit, move it to the lane's high bit, invert and mask
        // to one bit per matching lane, then `count_ones`. `rep1` = usize::MAX
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
    /// Read the element at `index` as a `usize`.
    fn get(&self, index: usize) -> usize;
    /// Write `value` to the element at `index`.
    fn set(&mut self, index: usize, value: usize);
    /// Append `value` to the end.
    fn push(&mut self, value: usize);
    /// Remove and return the last element, if any.
    fn pop(&mut self) -> Option<usize>;
    /// Remove all elements.
    fn clear(&mut self);
    /// Move all elements from `other` to the end of `self`, leaving `other`
    /// empty.
    fn append(&mut self, other: &mut Self);
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
    fn get(&self, index: usize) -> usize {
        PackedArray::get(self, index)
    }
    #[inline]
    fn set(&mut self, index: usize, value: usize) {
        PackedArray::set(self, index, value);
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
    fn eight_and_sixteen_bit() {
        let mut a8 = PackedArray::<8>::new();
        for v in 0u16..300 {
            a8.push((v & 0xFF) as usize);
        }
        for i in 0..a8.len() {
            assert_eq!(a8.get(i), i & 0xFF);
        }

        let mut a16 = PackedArray::<16>::new();
        for v in 0u32..70_000 {
            a16.push((v & 0xFFFF) as usize);
        }
        for i in 0..a16.len() {
            assert_eq!(a16.get(i), i & 0xFFFF);
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
        let mut a = PackedArray::<8>::with_capacity(4);
        assert!(a.capacity() >= 4);
        for i in 0..200 {
            a.push(i & 0xFF);
        }
        assert!(a.capacity() >= 200);
        a.shrink_to_fit();
        assert!(a.capacity() >= a.len());
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

        // --- Exhaustive enumeration of the first 4 lanes (rest 0 and rest all-ones),
        //     for every target. Only affordable for b in {2, 4}. ---
        if b == 2 || b == 4 {
            let span = nvals;
            // Enumerate first 4 lanes fully.
            for l0 in 0..span {
                for l1 in 0..span {
                    for l2 in 0..span {
                        for l3 in 0..span {
                            for &fill in &[0u64, nvals - 1] {
                                let word = build_word(b, &[l0, l1, l2, l3], fill);
                                for &t in &targets {
                                    let got = count_word_in_word(b, word, t);
                                    let exp = oracle_dyn(b, word, t);
                                    assert_eq!(got, exp, "exhaust b={} t={} word={:#x}", b, t, word);
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
        let mut state = 0x9E37_79B9_7F4A_7C15u64.wrapping_add((b as u64).wrapping_mul(2654435761));
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
        // adjacent (zero-then-small) lane patterns that stress borrow detectors.
        for pos in 0..per {
            // single lane == target (target 0), rest = (mask/2)
            let fill = nvals / 2;
            let mut lanes = vec![fill; per];
            lanes[pos] = 0;
            let word = build_word(b, &lanes, fill);
            for &t in &[0usize, 1, lane_mask, (nvals / 2) as usize] {
                let got = count_word_in_word(b, word, t);
                let exp = oracle_dyn(b, word, t);
                assert_eq!(got, exp, "single b={} pos={} t={} word={:#x}", b, pos, t, word);
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
                    for &word in &[build_word(b, &lanes_a, fill), build_word(b, &lanes_b, fill)] {
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
            8 => count_word_in::<8>(word, value),
            16 => count_word_in::<16>(word, value),
            _ => unreachable!("unsupported width {}", b),
        }
    }
    #[cfg(not(miri))]
    fn oracle_dyn(b: u32, word: usize, value: usize) -> usize {
        match b {
            2 => oracle::<2>(word, value),
            4 => oracle::<4>(word, value),
            8 => oracle::<8>(word, value),
            16 => oracle::<16>(word, value),
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
    #[cfg(not(miri))]
    #[test]
    fn count_word_in_gate_8() {
        gate_width(8);
    }
    #[cfg(not(miri))]
    #[test]
    fn count_word_in_gate_16() {
        gate_width(16);
    }
}
