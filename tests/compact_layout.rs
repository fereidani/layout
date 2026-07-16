//! `#[layout(...)]` derive-passthrough must work for structs containing
//! `Compact<T>` columns. `#[layout(Clone)]` generates `resize` and
//! `extend_from_slice` that dispatch to each column; compact columns need
//! matching methods on `CompactVec`. `#[layout(Debug/PartialEq/Eq/Hash)]`
//! derive those traits on the generated Slice/SliceMut/RefMut/Ptr/PtrMut,
//! whose compact fields need matching trait impls.
use layout::{soa_zip, Compact, CompactBool, SOA, SoAAppendVec};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[derive(Clone, PartialEq, SOA)]
#[layout(Clone)]
struct Tag {
    id: u32,
    active: CompactBool,
}

#[test]
fn layout_clone_resize_grows_and_shrinks() {
    let mut v = TagVec::new();
    v.push(Tag { id: 1, active: Compact(true) });
    v.push(Tag { id: 2, active: Compact(false) });

    v.resize(4, Tag { id: 9, active: Compact(true) });
    assert_eq!(v.len(), 4);
    assert_eq!(v.active.get(2), Some(Compact(true)));
    assert_eq!(v.id[3], 9);

    v.resize(1, Tag { id: 0, active: Compact(false) });
    assert_eq!(v.len(), 1);
}

#[test]
fn layout_clone_extend_from_slice() {
    let mut v = TagVec::new();
    v.push(Tag { id: 1, active: Compact(true) });

    let mut other = TagVec::new();
    other.push(Tag { id: 7, active: Compact(true) });
    other.push(Tag { id: 8, active: Compact(false) });

    v.extend_from_slice(other.as_slice());
    assert_eq!(v.len(), 3);
    assert_eq!(v.id[1], 7);
    assert_eq!(v.id[2], 8);
    assert_eq!(v.active.get(2), Some(Compact(false)));
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, SOA)]
#[layout(Debug, PartialEq, Eq, Hash)]
struct Marker {
    id: u32,
    flag: CompactBool,
}

#[test]
fn layout_traits_eq_hash_debug() {
    let mut a = MarkerVec::new();
    a.push(Marker { id: 1, flag: Compact(true) });
    a.push(Marker { id: 2, flag: Compact(false) });
    let mut b = MarkerVec::new();
    b.push(Marker { id: 1, flag: Compact(true) });
    b.push(Marker { id: 2, flag: Compact(false) });

    // PartialEq / Eq on the Vec (and, via the derive, on Slice/RefMut/Ptr).
    assert_eq!(a, b);
    assert_eq!(hash_of(&a), hash_of(&b));
    assert_eq!(hash_of(&a.as_slice()), hash_of(&b.as_slice()));
    assert_eq!(a.as_slice(), b.as_slice());

    // Debug value-lists the decoded compact column.
    let s = format!("{:?}", a);
    assert!(s.contains("Compact(true)"));

    // Mutation breaks equality (exercises CompactSliceMut path too).
    a.index_mut(0).flag.set(false);
    assert_ne!(a, b);
}

#[derive(SOA)]
struct Item {
    id: u32,
    flag: CompactBool,
}

#[test]
fn compactvec_iter_iter_mut_and_zip() {
    let mut v = ItemVec::new();
    v.push(Item { id: 1, flag: Compact(true) });
    v.push(Item { id: 2, flag: Compact(false) });

    // for x in &compact_vec
    let mut trues = 0;
    for f in &v.flag {
        if f.get() {
            trues += 1;
        }
    }
    assert_eq!(trues, 1);

    // iter_mut writes back immediately
    for mut f in v.flag.iter_mut() {
        f.set(true);
    }
    assert_eq!(v.flag.get(1), Some(Compact(true)));

    // soa_zip! over a compact column (immutable and mutable)
    let mut active = 0;
    for (_id, flag) in soa_zip!(&v, [id, flag]) {
        if flag.get() {
            active += 1;
        }
    }
    assert_eq!(active, 2);

    for mut flag in soa_zip!(&mut v, [mut flag]) {
        flag.set(false);
    }
    assert_eq!(v.flag.get(0), Some(Compact(false)));
}
