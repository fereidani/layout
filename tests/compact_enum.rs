use layout::{Compact, CompactRepr, SOA};

/// 3 variants (max discriminant 2) -> 2-bit storage.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, CompactRepr)]
enum Kind {
    Red,
    Green,
    Blue,
}

/// Custom (non-contiguous) discriminants within 4 bits (max 15) -> 4-bit
/// storage.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, CompactRepr)]
enum Spaced {
    Low = 0,
    High = 15,
}

#[derive(SOA)]
struct Item {
    name: String,
    kind: Compact<Kind>,
    spaced: Compact<Spaced>,
}

#[test]
fn compact_enum_roundtrip() {
    let mut items = ItemVec::new();
    items.push(Item {
        name: "a".into(),
        kind: Compact(Kind::Red),
        spaced: Compact(Spaced::Low),
    });
    items.push(Item {
        name: "b".into(),
        kind: Compact(Kind::Green),
        spaced: Compact(Spaced::High),
    });
    items.push(Item {
        name: "c".into(),
        kind: Compact(Kind::Blue),
        spaced: Compact(Spaced::Low),
    });

    assert_eq!(items.len(), 3);
    assert_eq!(items.get(0).unwrap().kind.get(), Kind::Red);
    assert_eq!(items.get(1).unwrap().kind.get(), Kind::Green);
    assert_eq!(items.get(2).unwrap().kind.get(), Kind::Blue);

    // custom-discriminant column round-trips the real values.
    assert_eq!(items.get(0).unwrap().spaced.get(), Spaced::Low);
    assert_eq!(items.get(1).unwrap().spaced.get(), Spaced::High);
    assert_eq!(items.get(2).unwrap().spaced.get(), Spaced::Low);
}

#[test]
fn compact_enum_get_set() {
    let mut items = ItemVec::new();
    items.push(Item {
        name: "a".into(),
        kind: Compact(Kind::Red),
        spaced: Compact(Spaced::Low),
    });

    let mut r = items.get_mut(0).unwrap();
    assert_eq!(r.kind.get(), Kind::Red);
    r.kind.set(Kind::Blue);
    assert_eq!(r.kind.get(), Kind::Blue);
    r.spaced.set(Spaced::High);
    assert_eq!(r.spaced.get(), Spaced::High);
}

#[test]
fn compact_enum_iter() {
    let mut items = ItemVec::new();
    items.push(Item {
        name: "a".into(),
        kind: Compact(Kind::Red),
        spaced: Compact(Spaced::Low),
    });
    items.push(Item {
        name: "b".into(),
        kind: Compact(Kind::Green),
        spaced: Compact(Spaced::High),
    });

    let kinds: Vec<Kind> = items.iter().map(|r| r.kind.get()).collect();
    assert_eq!(kinds, vec![Kind::Red, Kind::Green]);
    let spaced: Vec<Spaced> = items.iter().map(|r| r.spaced.get()).collect();
    assert_eq!(spaced, vec![Spaced::Low, Spaced::High]);
}

#[test]
fn compact_enum_pop_remove_insert() {
    let mut items = ItemVec::new();
    items.push(Item {
        name: "a".into(),
        kind: Compact(Kind::Red),
        spaced: Compact(Spaced::Low),
    });
    items.push(Item {
        name: "b".into(),
        kind: Compact(Kind::Green),
        spaced: Compact(Spaced::High),
    });
    items.push(Item {
        name: "c".into(),
        kind: Compact(Kind::Blue),
        spaced: Compact(Spaced::Low),
    });

    let last = items.pop().unwrap();
    assert_eq!(last.kind.get(), Kind::Blue);
    assert_eq!(last.spaced.get(), Spaced::Low);

    items.remove(0);
    assert_eq!(items.get(0).unwrap().kind.get(), Kind::Green);

    items.insert(
        0,
        Item {
            name: "z".into(),
            kind: Compact(Kind::Red),
            spaced: Compact(Spaced::High),
        },
    );
    assert_eq!(items.get(0).unwrap().kind.get(), Kind::Red);
    assert_eq!(items.get(0).unwrap().spaced.get(), Spaced::High);
}

#[test]
fn compact_enum_storage_width() {
    // Sanity: the encoded widths are as designed (Kind=2 bits, Spaced=4 bits).
    // Enums needing >4 bits are rejected by `#[derive(CompactRepr)]` (8-bit
    // compact is byte-for-byte a plain `Vec<Enum>` and thus redundant).
    assert_eq!(<Kind as CompactRepr>::BITS, 2);
    assert_eq!(<Spaced as CompactRepr>::BITS, 4);
}

// `decode` is safe and public: invalid discriminants (only reachable via
// unsafe construction or bit corruption) map to the first variant instead of
// panicking. Debug builds still fail fast via `debug_assert!`.
#[cfg(not(debug_assertions))]
#[test]
fn decode_invalid_falls_back_to_first_variant() {
    assert_eq!(Kind::decode(3), Kind::Red);
    assert_eq!(Kind::decode(99), Kind::Red);
    assert_eq!(Spaced::decode(5), Spaced::Low);
    // Valid discriminants are unaffected.
    assert_eq!(Kind::decode(0), Kind::Red);
    assert_eq!(Kind::decode(1), Kind::Green);
    assert_eq!(Kind::decode(2), Kind::Blue);
}
