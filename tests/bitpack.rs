//! Unit tests for the bit-packed backing store, including the SWAR
//! `count_word_in` correctness gate (exhaustive small-width enumeration,
//! LCG fuzz, and structured borrow-stress edge cases checked against an
//! independent scalar oracle).

use layout::bitpack::{count_word_in, PackedArray};

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
                            let word = build_word(b, &[l0, l1, l2, l3], fill);
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
