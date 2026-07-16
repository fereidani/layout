use layout::{Compact, CompactBool, SOA};

#[derive(SOA)]
struct Entity {
    name: String,
    active: CompactBool,
    value: f64,
}

#[test]
fn compact_bool_push_pop() {
    let mut entities = EntityVec::new();
    entities.push(Entity {
        name: "a".into(),
        active: Compact(true),
        value: 1.0,
    });
    entities.push(Entity {
        name: "b".into(),
        active: Compact(false),
        value: 2.0,
    });
    entities.push(Entity {
        name: "c".into(),
        active: Compact(true),
        value: 3.0,
    });

    assert_eq!(entities.len(), 3);

    let last = entities.pop().unwrap();
    assert_eq!(last.name, "c");
    assert_eq!(last.active, Compact(true));
    assert_eq!(last.value, 3.0);
}

#[test]
fn compact_bool_get_set() {
    let mut entities = EntityVec::new();
    entities.push(Entity {
        name: "a".into(),
        active: Compact(true),
        value: 1.0,
    });
    entities.push(Entity {
        name: "b".into(),
        active: Compact(false),
        value: 2.0,
    });

    // immutable: read via get()
    let r = entities.get(0).unwrap();
    assert!(r.active.get());

    // mutable: read via get(), write via set()
    let mut r = entities.get_mut(1).unwrap();
    assert!(!r.active.get());
    r.active.set(true);
    assert!(r.active.get());
}

#[test]
fn compact_bool_ref() {
    let mut entities = EntityVec::new();
    entities.push(Entity {
        name: "a".into(),
        active: Compact(true),
        value: 1.0,
    });

    let r = entities.get(0).unwrap();
    assert!(r.active.get());
}

#[test]
fn compact_bool_insert_remove() {
    let mut entities = EntityVec::new();
    entities.push(Entity {
        name: "a".into(),
        active: Compact(true),
        value: 1.0,
    });
    entities.push(Entity {
        name: "b".into(),
        active: Compact(false),
        value: 2.0,
    });
    entities.push(Entity {
        name: "c".into(),
        active: Compact(true),
        value: 3.0,
    });

    entities.remove(1);
    assert_eq!(entities.len(), 2);
    assert!(entities.get(0).unwrap().active.get());
    assert!(entities.get(1).unwrap().active.get());
}

#[test]
fn compact_bool_iter() {
    let mut entities = EntityVec::new();
    entities.push(Entity {
        name: "a".into(),
        active: Compact(true),
        value: 1.0,
    });
    entities.push(Entity {
        name: "b".into(),
        active: Compact(false),
        value: 2.0,
    });
    entities.push(Entity {
        name: "c".into(),
        active: Compact(true),
        value: 3.0,
    });

    let count: Vec<bool> = entities.iter().map(|r| r.active.get()).collect();
    assert_eq!(count, vec![true, false, true]);
}

#[test]
fn compact_bool_as_slice() {
    let mut entities = EntityVec::new();
    entities.push(Entity {
        name: "a".into(),
        active: Compact(true),
        value: 1.0,
    });
    entities.push(Entity {
        name: "b".into(),
        active: Compact(false),
        value: 2.0,
    });

    let slice = entities.as_slice();
    assert_eq!(slice.len(), 2);
    assert!(slice.get(0).unwrap().active.get());
    assert!(!slice.get(1).unwrap().active.get());
}

#[test]
fn compact_bool_mut_writeback() {
    let mut entities = EntityVec::new();
    entities.push(Entity {
        name: "a".into(),
        active: Compact(false),
        value: 1.0,
    });
    entities.push(Entity {
        name: "b".into(),
        active: Compact(true),
        value: 2.0,
    });

    // Mutate through the iterator via get()/set(); changes persist (immediate
    // write).
    for mut r in entities.iter_mut() {
        r.active.set(!r.active.get());
    }
    assert!(entities.get(0).unwrap().active.get());
    assert!(!entities.get(1).unwrap().active.get());
}

#[test]
fn compact_bool_owned_get_set() {
    // get()/set() on the owning CompactBool wrapper directly.
    let mut flag = Compact(false);
    assert!(!flag.get());
    flag.set(true);
    assert!(flag.get());
}
