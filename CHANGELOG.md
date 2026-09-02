# Changelog

## Unreleased

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
