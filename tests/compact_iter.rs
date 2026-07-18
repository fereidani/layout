//! `CompactIter` internal-iteration paths (`fold`, `nth`, `last`, `count`)
//! against a per-element oracle, including offset subslices and mixed
//! front/back consumption.

use layout::{Compact, CompactVec};

fn build(n: usize) -> (CompactVec<bool>, Vec<bool>) {
    let oracle: Vec<bool> = (0..n).map(|i| i % 3 == 0 || i % 7 == 0).collect();
    let cv = oracle.iter().map(|&b| Compact::new(b)).collect();
    (cv, oracle)
}

#[test]
fn fold_matches_oracle() {
    for n in [0usize, 1, 63, 64, 65, 200] {
        let (cv, oracle) = build(n);
        let got: Vec<bool> = cv.iter().map(|c| c.get()).collect();
        assert_eq!(got, oracle, "n={n}");
        // `filter(..).count()` routes through `fold`.
        let ones = cv.iter().filter(|c| c.get()).count();
        assert_eq!(ones, oracle.iter().filter(|&&b| b).count(), "n={n}");
    }
}

#[test]
fn fold_on_offset_subslice() {
    let (cv, oracle) = build(300);
    let s = cv.slice(37..263);
    let got: Vec<bool> = s.iter().map(|c| c.get()).collect();
    assert_eq!(got, oracle[37..263].to_vec());
    let sum: usize = s.iter().map(|c| usize::from(c.get())).sum();
    let want: usize = oracle[37..263].iter().map(|&b| usize::from(b)).sum();
    assert_eq!(sum, want);
}

#[test]
fn nth_and_last() {
    let (cv, oracle) = build(200);
    let mut it = cv.iter();
    assert_eq!(it.nth(5).unwrap().get(), oracle[5]);
    // After nth, the cursor continues correctly across the invalidated
    // cache.
    assert_eq!(it.next().unwrap().get(), oracle[6]);
    assert_eq!(it.nth(70).unwrap().get(), oracle[77]);
    assert_eq!(it.next().unwrap().get(), oracle[78]);
    // Overshoot exhausts.
    assert!(it.nth(1000).is_none());
    assert!(it.next().is_none());

    let (cv, oracle) = build(130);
    assert_eq!(cv.iter().last().unwrap().get(), oracle[129]);
    let empty = CompactVec::<bool>::new();
    assert!(empty.iter().last().is_none());
}

#[test]
fn mixed_front_back_then_fold() {
    let (cv, oracle) = build(150);
    let mut it = cv.iter();
    assert_eq!(it.next().unwrap().get(), oracle[0]);
    assert_eq!(it.next_back().unwrap().get(), oracle[149]);
    assert_eq!(it.next().unwrap().get(), oracle[1]);
    // Remaining range is [2, 149); fold must cover exactly that.
    let rest: Vec<bool> = it.map(|c| c.get()).collect();
    assert_eq!(rest, oracle[2..149].to_vec());
}
