//! Oracle tests for the word-level `copy_lanes` primitive (overlapping
//! memmove over packed lanes) and its integration into `CompactVec`
//! insert/remove.

use layout::{bitpack::PackedArray, Compact, CompactVec};

#[test]
fn copy_lanes_matches_oracle() {
    fn check<const B: u32>() {
        let per = (usize::BITS / B) as usize;
        let mask = (1usize << B) - 1;
        let n = 3 * per + 7;
        let positions = [
            0,
            1,
            2,
            per - 1,
            per,
            per + 1,
            2 * per - 1,
            2 * per + 3,
            n - 1,
        ];
        let counts = [0, 1, 2, per - 1, per, per + 1, 2 * per, 2 * per + 5];
        for &src in &positions {
            for &dst in &positions {
                for &cnt in &counts {
                    if src + cnt > n || dst + cnt > n {
                        continue;
                    }
                    let mut a = PackedArray::<B>::new();
                    for i in 0..n {
                        a.push(i.wrapping_mul(11).wrapping_add(5) & mask);
                    }
                    let mut want: Vec<usize> =
                        (0..n).map(|i| a.get(i)).collect();
                    // Oracle: memmove semantics.
                    let moved: Vec<usize> = want[src..src + cnt].to_vec();
                    want[dst..dst + cnt].copy_from_slice(&moved);
                    a.copy_lanes(src, dst, cnt);
                    for (i, &w) in want.iter().enumerate() {
                        assert_eq!(
                            a.get(i),
                            w,
                            "B={B} src={src} dst={dst} cnt={cnt} at {i}"
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

#[test]
fn insert_remove_word_shift_roundtrip() {
    // Insert/remove across word boundaries; verify against a Vec<bool>
    // oracle.
    let n = 300;
    let mut cv: CompactVec<bool> = CompactVec::new();
    let mut oracle: Vec<bool> = Vec::new();
    for i in 0..n {
        cv.push(Compact::new(i % 3 == 0));
        oracle.push(i % 3 == 0);
    }
    for &at in &[0usize, 1, 63, 64, 65, 150, 299] {
        cv.insert(at, Compact::new(true));
        oracle.insert(at, true);
    }
    for &at in &[0usize, 70, 200, 300, 299] {
        assert_eq!(cv.remove(at).get(), oracle.remove(at));
    }
    assert_eq!(cv.len(), oracle.len());
    for (i, &b) in oracle.iter().enumerate() {
        assert_eq!(cv.get(i).unwrap().get(), b, "at {i}");
    }
}
