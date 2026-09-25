# Changelog

## 0.2.3 - 2026-09-25

### Fixed

- Safe code could make row access read out of bounds by leaving the
  columns of a generated vector or slice with different lengths: the
  columns are public, so a column can be swapped or replaced, a nested or
  compact column can `push`/`pop`/`clear` on its own, a slice can be built
  from mismatched fields, and `Deserialize` accepts mismatched columns.
  The length is now the shortest column's, so every row access stays in
  bounds; debug builds still assert that the lengths match.
- A panicking `Clone` in `resize` / `extend_from_slice`, or a panicking
  `Drop` in `truncate` / `clear`, left the columns with different lengths.
  The columns are now cut back to whole rows while unwinding.
- `drain` asked its `RangeBounds` for the range once per column, so a range
  that answers differently on each call drained the columns unevenly. It
  now resolves the range once.
- Hashing an empty default `CompactSlice` or `CompactSliceMut` dereferenced
  a dangling pointer.
- Sorting a compact column of an enum whose variant count is not a power
  of two panicked in debug builds: the counting sort decoded raw values
  that are not discriminants. The comparator now only sees values present
  in the slice.
- Indexing with an exhausted `RangeInclusive` returned one row instead of
  an empty slice.
- `#[layout(Clone)]` on a struct that implements `Drop` failed to compile.
- The derive relied on names in scope at the call site: a `#[nested_soa]`
  field needed `SOA` imported, and a local module named `layout` broke the
  generated paths.
- `#[soa_impl]` on `impl path::Type` took the first path segment as the
  type name.
- `CompactVec` deserialization preallocated whatever length the input
  claimed; the preallocation is now capped, as serde does for `Vec`.

### Performance

- Generated iterators implement `nth`, `nth_back` and `last` by moving
  every column cursor at once. With a bit-packed column, `skip` and
  `step_by` stepped every skipped row; `step_by(8)` is now about 4x faster.
- A bit-packed column in a generated iterator reads each lane straight from
  its word. A loop that never reads the compact field now drops it and can
  vectorize (a sum over a struct with an unread `Compact<bool>` is 6x to 8x
  faster), and loops that do read it are about 2.5x faster.

### Changed

- `BitPack` gained `word_unchecked`, with a default that forwards to
  `word`.
- The generated `drain` no longer requires `R: Clone`.
- Removed the hidden in-place permutation helpers that no generated code
  calls since sorts gather instead (`layout_internal` is pinned to the
  exact matching version).

## 0.2.2 - 2026-09-02

### Performance

- `retain` / `retain_mut` on generated vectors now use the write-index
  compaction of `Vec::retain`: rows before the first rejection are never
  written, kept rows move down with one copy per column, and every column
  length is set once at the end. Previously each kept row was swapped
  through an out-of-line call.
- `sort_by`, `sort_by_key`, `sort` and `apply_index` gather each column into
  a fresh buffer in sorted order instead of walking permutation cycles in
  place, which chased one dependent cache miss per element. Sorting a
  four-column struct of 100k rows by key dropped from 10.1 ms to 3.9 ms; the
  permutation apply itself is 5x to 20x faster depending on size.
- `CompactVec::count` counts whole words only; the two boundary words are
  masked instead of extracted lane by lane. Counting 100k bits dropped from
  1.8 us to 0.3 us.
- Bit-packed columns copy unaligned ranges a word at a time: `split_off`,
  `extend_from_slice`, `append` and `drain` at a non-word-aligned index no
  longer fall back to one push per lane (`split_off` of 100k bits at the
  midpoint dropped from 70 us to 0.4 us), and the tail shift of `insert`,
  `remove` and `splice` is about 3x faster.
- `CompactVec::extend` and `collect` pack lanes into a register and store
  each completed word once.
- `pop` on a generated vector no longer re-checks emptiness per column.

### Changed

- `BitPack` gained `extend_lanes` and `copy_from_packed`. Only affects
  out-of-crate implementors; `PackedArray` supplies both.

### Fixed

- `#[derive(SOA)]` failed to compile for a struct with a field named `pos`,
  `end` or `chunk_size`, which collided with the generated chunk iterators'
  own fields.

## 0.2.0 - 2026-08-20

### Breaking

- `BitPack` now requires `const BITS: u32`, the store's lane width. Only
  affects out-of-crate implementors; `PackedArray` supplies it.

### Fixed

- `CompactVec::drain` and `splice` silently did nothing for an inclusive end
  of `usize::MAX`, instead of panicking like `Vec`.
- A `CompactRepr` impl whose `BITS` disagrees with its `Storage` lane width is
  now rejected at compile time instead of addressing the wrong bits.
