//! Word-level copies, counts and comparisons of the bit-packed store,
//! checked lane by lane against a plain `Vec<usize>` oracle at every
//! alignment.

use layout::bitpack::PackedArray;

fn lcg(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

fn random_lanes(n: usize, mask: usize, seed: &mut u64) -> Vec<usize> {
    (0..n).map(|_| (lcg(seed) as usize) & mask).collect()
}

fn packed<const B: u32>(lanes: &[usize]) -> PackedArray<B> {
    let mut a = PackedArray::<B>::new();
    for &v in lanes {
        a.push(v);
    }
    a
}

fn lanes<const B: u32>(a: &PackedArray<B>) -> Vec<usize> {
    (0..a.len()).map(|i| a.get(i)).collect()
}

fn check_copy_lanes<const B: u32>() {
    let mask = (1usize << B) - 1;
    let per = (usize::BITS / B) as usize;
    let mut seed = 11u64;
    let n = 3 * per + 5;
    let base = random_lanes(n, mask, &mut seed);
    let offsets = [0usize, 1, per - 1, per, per + 1, 2 * per + 3];
    let counts = [0usize, 1, per - 1, per, per + 1, 2 * per, n - 2 * per - 3];
    for &src in &offsets {
        for &dst in &offsets {
            for &count in &counts {
                if src + count > n || dst + count > n {
                    continue;
                }
                let mut a = packed::<B>(&base);
                a.copy_lanes(src, dst, count);
                let mut want = base.clone();
                want.copy_within(src..src + count, dst);
                assert_eq!(
                    lanes(&a),
                    want,
                    "B={B} src={src} dst={dst} count={count}"
                );
            }
        }
    }
}

#[test]
fn copy_lanes_matches_copy_within() {
    check_copy_lanes::<1>();
    check_copy_lanes::<2>();
    check_copy_lanes::<4>();
}

fn check_copy_from_packed<const B: u32>() {
    let mask = (1usize << B) - 1;
    let per = (usize::BITS / B) as usize;
    let mut seed = 12u64;
    let n = 3 * per + 7;
    let src_lanes = random_lanes(n, mask, &mut seed);
    let dst_lanes = random_lanes(n, mask, &mut seed);
    let src = packed::<B>(&src_lanes);
    let offsets = [0usize, 1, per - 1, per, per + 2, 2 * per + 5];
    let counts = [0usize, 1, per - 1, per, per + 1, 2 * per + 1];
    for &from in &offsets {
        for &to in &offsets {
            for &count in &counts {
                if from + count > n || to + count > n {
                    continue;
                }
                let mut dst = packed::<B>(&dst_lanes);
                dst.copy_from_packed(&src, from, to, count);
                let mut want = dst_lanes.clone();
                want[to..to + count]
                    .copy_from_slice(&src_lanes[from..from + count]);
                assert_eq!(
                    lanes(&dst),
                    want,
                    "B={B} from={from} to={to} count={count}"
                );
            }
        }
    }
}

#[test]
fn copy_from_packed_matches_slice_copy() {
    check_copy_from_packed::<1>();
    check_copy_from_packed::<2>();
    check_copy_from_packed::<4>();
}

fn check_count_in<const B: u32>() {
    let mask = (1usize << B) - 1;
    let per = (usize::BITS / B) as usize;
    let mut seed = 13u64;
    let n = 2 * per + 9;
    let base = random_lanes(n, mask, &mut seed);
    let a = packed::<B>(&base);
    // Under Miri sample the alignments; natively sweep them all.
    let step = if cfg!(miri) { 11 } else { 1 };
    for start in (0..n).step_by(step) {
        for len in (0..=(n - start)).step_by(step) {
            for value in 0..=mask {
                let want = base[start..start + len]
                    .iter()
                    .filter(|&&v| v == value)
                    .count();
                assert_eq!(
                    a.count_in(start, len, value),
                    want,
                    "B={B} start={start} len={len} value={value}"
                );
            }
        }
    }
}

#[test]
fn count_in_matches_oracle_at_every_alignment() {
    check_count_in::<1>();
    check_count_in::<2>();
    check_count_in::<4>();
}

fn check_range_eq<const B: u32>() {
    let mask = (1usize << B) - 1;
    let per = (usize::BITS / B) as usize;
    let mut seed = 14u64;
    let n = 3 * per + 3;
    let base = random_lanes(n, mask, &mut seed);
    let a = packed::<B>(&base);
    // `b` holds the same lanes shifted by one, so every unaligned pairing
    // compares equal lanes at different offsets.
    let mut shifted = vec![mask ^ base[0]];
    shifted.extend_from_slice(&base);
    let b = packed::<B>(&shifted);
    for start in [0usize, 1, per - 1, per, per + 1] {
        for len in [0usize, 1, per - 1, per, per + 1, 2 * per, n - per - 1] {
            assert!(
                a.range_eq(start, &b, start + 1, len),
                "B={B} start={start} len={len}"
            );
            if len > 0 {
                let mut c = packed::<B>(&shifted);
                let last = start + 1 + len - 1;
                c.set(last, c.get(last) ^ 1);
                assert!(
                    !a.range_eq(start, &c, start + 1, len),
                    "B={B} start={start} len={len}"
                );
            }
        }
    }
}

#[test]
fn range_eq_unaligned_matches_lanes() {
    check_range_eq::<1>();
    check_range_eq::<2>();
    check_range_eq::<4>();
}

fn check_extend_lanes<const B: u32>() {
    let mask = (1usize << B) - 1;
    let per = (usize::BITS / B) as usize;
    let mut seed = 15u64;
    for head in [0usize, 1, per - 1, per, per + 3] {
        for count in [0usize, 1, per - 1, per, per + 1, 5 * per + 2] {
            let first = random_lanes(head, mask, &mut seed);
            let more = random_lanes(count, mask, &mut seed);
            let mut a = packed::<B>(&first);
            a.extend_lanes(more.iter().copied());
            let mut want = first.clone();
            want.extend_from_slice(&more);
            assert_eq!(lanes(&a), want, "B={B} head={head} count={count}");
        }
    }
}

#[test]
fn extend_lanes_matches_push() {
    check_extend_lanes::<1>();
    check_extend_lanes::<2>();
    check_extend_lanes::<4>();
}

// A store whose length was lowered with `set_len` keeps stale words past
// its length (as a leaked drain leaves it). Every bulk append must ignore
// them.
fn check_slack_store<const B: u32>() {
    let mask = (1usize << B) - 1;
    let per = (usize::BITS / B) as usize;
    let mut seed = 16u64;
    let base = random_lanes(3 * per, mask, &mut seed);
    let more = random_lanes(per + 3, mask, &mut seed);
    let src = packed::<B>(&more);
    for keep in [0usize, 1, per - 1, per, per + 1] {
        let slack = || {
            let mut a = packed::<B>(&base);
            // SAFETY: `keep <= len`, so every kept lane stays initialized.
            unsafe { a.set_len(keep) };
            a
        };
        let want = |tail: &[usize]| {
            let mut w = base[..keep].to_vec();
            w.extend_from_slice(tail);
            w
        };

        let mut a = slack();
        a.extend_from_packed(&src, 1, per + 1);
        assert_eq!(
            lanes(&a),
            want(&more[1..per + 2]),
            "B={B} keep={keep} packed"
        );

        let mut a = slack();
        a.extend_lanes(more.iter().copied());
        assert_eq!(lanes(&a), want(&more), "B={B} keep={keep} lanes");

        let mut a = slack();
        a.extend_fill(mask, per + 2);
        assert_eq!(
            lanes(&a),
            want(&vec![mask; per + 2]),
            "B={B} keep={keep} fill"
        );

        let mut a = slack();
        let mut b = packed::<B>(&more);
        a.append(&mut b);
        assert_eq!(lanes(&a), want(&more), "B={B} keep={keep} append");
        assert!(b.is_empty());

        let mut a = slack();
        a.push(mask);
        assert_eq!(lanes(&a), want(&[mask]), "B={B} keep={keep} push");
    }
}

#[test]
fn bulk_appends_ignore_stale_words() {
    check_slack_store::<1>();
    check_slack_store::<2>();
    check_slack_store::<4>();
}
