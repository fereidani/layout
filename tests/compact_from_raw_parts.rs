use layout::{Compact, CompactBool, SOA};

#[derive(Debug, Clone, PartialEq, SOA)]
pub struct C {
    pub id: u32,
    pub flag: CompactBool,
}

#[test]
fn compact_round_trip() {
    let mut v = CVec::new();
    v.push(C {
        id: 1,
        flag: Compact(true),
    });
    v.push(C {
        id: 2,
        flag: Compact(false),
    });
    v.push(C {
        id: 3,
        flag: Compact(true),
    });
    let len = v.len();
    let cap = v.id.capacity();
    let ptr = v.as_mut_ptr();
    core::mem::forget(v);
    let v2 = unsafe { CVec::from_raw_parts(ptr, len, cap) };
    assert_eq!(v2.len(), 3);
    assert_eq!(v2.id[0], 1);
    assert_eq!(v2.id[2], 3);
    assert!(v2.get(0).unwrap().flag.get());
    assert!(!v2.get(1).unwrap().flag.get());
    // dropping v2 must not abort
    drop(v2);
}
