# Changelog

## 0.2.0 - 2026-08-20

### Breaking

- `BitPack` now requires `const BITS: u32`, the store's lane width. Only
  affects out-of-crate implementors; `PackedArray` supplies it.

### Fixed

- `CompactVec::drain` and `splice` silently did nothing for an inclusive end
  of `usize::MAX`, instead of panicking like `Vec`.
- A `CompactRepr` impl whose `BITS` disagrees with its `Storage` lane width is
  now rejected at compile time instead of addressing the wrong bits.
