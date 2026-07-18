//! Counting-sort semantics of compact-column sorts, and the word-level
//! `fill_range` primitive they rewrite the slice with.

use layout::{bitpack::PackedArray, Compact, CompactVec};

#[repr(u8)]
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, layout::CompactRepr,
)]
enum Kind {
    A,
    B,
    C,
    D,
}

fn bools(xs: &[bool]) -> CompactVec<bool> {
    xs.iter().map(|&b| Compact::new(b)).collect()
}

fn kinds(xs: &[Kind]) -> CompactVec<Kind> {
    xs.iter().map(|&k| Compact::new(k)).collect()
}

#[test]
fn sort_bools_large() {
    // Spans several words with an irregular pattern.
    let n = 1000;
    let mut v: CompactVec<bool> = (0..n)
        .map(|i| Compact::new(i % 7 == 0 || i % 3 == 0))
        .collect();
    let ones = v.count(true);
    v.as_mut_slice().sort();
    for i in 0..n {
        assert_eq!(v.get(i).unwrap().get(), i >= n - ones, "at {i}");
    }
}

#[test]
fn sort_by_reverse_order() {
    let mut v = kinds(&[Kind::C, Kind::A, Kind::D, Kind::A, Kind::B]);
    v.as_mut_slice().sort_by(|a, b| b.get().cmp(&a.get()));
    let got: Vec<Kind> = v.iter().map(|c| c.get()).collect();
    assert_eq!(got, [Kind::D, Kind::C, Kind::B, Kind::A, Kind::A]);
}

#[test]
fn sort_by_key_custom() {
    // Key collapses B and C into one bucket; equal-keyed elements are
    // grouped by stored value (documented counting-sort contract), with the
    // smaller raw value first.
    let mut v = kinds(&[Kind::C, Kind::B, Kind::D, Kind::C, Kind::B, Kind::A]);
    v.as_mut_slice().sort_by_key(|c| match c.get() {
        Kind::A => 0u8,
        Kind::B | Kind::C => 1,
        Kind::D => 2,
    });
    let got: Vec<Kind> = v.iter().map(|c| c.get()).collect();
    assert_eq!(got, [Kind::A, Kind::B, Kind::B, Kind::C, Kind::C, Kind::D]);
}

#[test]
fn sort_subslice_leaves_neighbors_alone() {
    // Sort a middle window at an unaligned offset; the outside lanes must
    // not move.
    let n = 200;
    let mut v: CompactVec<bool> =
        (0..n).map(|i| Compact::new(i % 2 == 0)).collect();
    let ones_inside = (37..163).filter(|i| i % 2 == 0).count();
    v.slice_mut(37..163).sort();
    for i in 0..37 {
        assert_eq!(v.get(i).unwrap().get(), i % 2 == 0, "prefix at {i}");
    }
    for i in 163..n {
        assert_eq!(v.get(i).unwrap().get(), i % 2 == 0, "suffix at {i}");
    }
    for i in 37..163 {
        assert_eq!(
            v.get(i).unwrap().get(),
            i >= 163 - ones_inside,
            "window at {i}"
        );
    }
}

#[test]
fn sort_all_equal_and_empty_and_single() {
    let mut v = bools(&[]);
    v.as_mut_slice().sort();
    assert_eq!(v.len(), 0);

    let mut v = bools(&[true]);
    v.as_mut_slice().sort();
    assert!(v.get(0).unwrap().get());

    let mut v = bools(&[true; 130]);
    v.as_mut_slice().sort();
    assert_eq!(v.count(true), 130);
}

#[test]
fn fill_range_matches_oracle() {
    // Every (start alignment, length, width) combination against a per-lane
    // oracle, including single-word ranges and exact word boundaries.
    fn check<const B: u32>() {
        let per = (usize::BITS / B) as usize;
        let mask = (1usize << B) - 1;
        let n = 3 * per + 5;
        let starts = [
            0,
            1,
            per - 1,
            per,
            per + 1,
            2 * per - 1,
            2 * per,
            2 * per + 3,
        ];
        let lens = [0, 1, 2, per - 1, per, per + 1, 2 * per, 2 * per + 4];
        for &st in &starts {
            for &ln in &lens {
                if st + ln > n {
                    continue;
                }
                for &v in &[0usize, 1, mask] {
                    let mut a = PackedArray::<B>::new();
                    for i in 0..n {
                        a.push(i.wrapping_mul(7).wrapping_add(3) & mask);
                    }
                    let mut want: Vec<usize> =
                        (0..n).map(|i| a.get(i)).collect();
                    for slot in want.iter_mut().skip(st).take(ln) {
                        *slot = v;
                    }
                    a.fill_range(st, ln, v);
                    for (i, &w) in want.iter().enumerate() {
                        assert_eq!(
                            a.get(i),
                            w,
                            "B={B} st={st} ln={ln} v={v} at {i}"
                        );
                    }
                }
            }
        }
    }
    check::<1>();
    check::<2>();
    check::<4>();
}
