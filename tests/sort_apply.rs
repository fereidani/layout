//! Composite sorts and `apply_index` reorder every column kind (plain,
//! compact, nested) consistently, checked against `Vec` sorts on the same
//! keys.

use layout::{Compact, SoASliceMut, SOA};

#[derive(SOA, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[layout(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Pos {
    x: i64,
    y: i64,
}

#[derive(SOA, Clone, Debug, PartialEq)]
#[layout(Clone, Debug, PartialEq)]
struct Item {
    key: u32,
    seq: u32,
    name: String,
    flag: Compact<bool>,
    #[nested_soa]
    pos: Pos,
}

fn lcg(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

fn item(seq: u32, key: u32) -> Item {
    Item {
        key,
        seq,
        name: format!("item{seq}"),
        flag: Compact::new(seq % 3 == 0),
        pos: Pos {
            x: i64::from(seq) * 7,
            y: -i64::from(seq),
        },
    }
}

fn build(n: u32, distinct_keys: u64, seed: &mut u64) -> (ItemVec, Vec<Item>) {
    let mut v = ItemVec::new();
    let mut plain = Vec::new();
    for seq in 0..n {
        let it = item(seq, (lcg(seed) % distinct_keys) as u32);
        plain.push(it.clone());
        v.push(it);
    }
    (v, plain)
}

fn owned(v: &ItemVec) -> Vec<Item> {
    v.iter().map(|r| r.to_owned()).collect()
}

#[test]
fn sort_by_key_matches_stable_vec_sort() {
    let mut seed = 1u64;
    let sizes: &[u32] = if cfg!(miri) {
        &[0, 1, 2, 3, 64, 65, 130]
    } else {
        &[0, 1, 2, 3, 64, 65, 200, 2000]
    };
    for &n in sizes {
        for keys in [1u64, 2, 7, 1000] {
            let (mut v, mut plain) = build(n, keys, &mut seed);
            v.as_mut_slice().sort_by_key(|r| *r.key);
            plain.sort_by_key(|it| it.key);
            assert_eq!(owned(&v), plain, "n={n} keys={keys}");
        }
    }
}

#[test]
fn sort_by_matches_stable_vec_sort() {
    let mut seed = 2u64;
    for n in [0u32, 1, 2, 63, 64, 65, 500] {
        let (mut v, mut plain) = build(n, 5, &mut seed);
        v.as_mut_slice().sort_by(|a, b| b.key.cmp(a.key));
        plain.sort_by_key(|it| core::cmp::Reverse(it.key));
        assert_eq!(owned(&v), plain, "n={n}");
    }
}

#[test]
fn sort_matches_vec_sort() {
    let mut seed = 3u64;
    for n in [0u32, 1, 2, 64, 65, 300] {
        let mut v = PosVec::new();
        let mut plain = Vec::new();
        for _ in 0..n {
            let p = Pos {
                x: (lcg(&mut seed) % 9) as i64,
                y: (lcg(&mut seed) % 5) as i64,
            };
            plain.push(p.clone());
            v.push(p);
        }
        v.as_mut_slice().sort();
        plain.sort();
        let got: Vec<Pos> = v.iter().map(|r| r.to_owned()).collect();
        assert_eq!(got, plain, "n={n}");
    }
}

#[test]
fn apply_index_gathers_from_argsort() {
    let mut seed = 4u64;
    let sizes: &[u32] = if cfg!(miri) {
        &[0, 1, 2, 64, 65, 129]
    } else {
        &[0, 1, 2, 64, 65, 129, 1000]
    };
    for &n in sizes {
        let (mut v, plain) = build(n, 1000, &mut seed);
        // Random permutation `indices[pos] = src`.
        let mut indices: Vec<usize> = (0..n as usize).collect();
        for i in (1..indices.len()).rev() {
            let j = (lcg(&mut seed) as usize) % (i + 1);
            indices.swap(i, j);
        }
        v.as_mut_slice().apply_index(&indices);
        let want: Vec<Item> =
            indices.iter().map(|&src| plain[src].clone()).collect();
        assert_eq!(owned(&v), want, "n={n}");
    }
}

#[test]
fn sort_of_sub_slice_leaves_the_rest_alone() {
    let mut seed = 5u64;
    let (mut v, plain) = build(200, 10, &mut seed);
    v.slice_mut(50..150).sort_by_key(|r| *r.key);
    let got = owned(&v);
    assert_eq!(&got[..50], &plain[..50]);
    assert_eq!(&got[150..], &plain[150..]);
    let mut middle = plain[50..150].to_vec();
    middle.sort_by_key(|it| it.key);
    assert_eq!(&got[50..150], &middle[..]);
}
