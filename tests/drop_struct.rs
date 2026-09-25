//! A struct that implements `Drop` can still derive the `Clone`-only
//! methods of its generated vector.

use layout::SOA;

/// A `Drop` struct can still use the `Clone`-only `resize`.
#[derive(Clone, SOA)]
#[layout(Clone)]
pub struct Tracked {
    pub id: u64,
    pub name: String,
}

impl Drop for Tracked {
    fn drop(&mut self) {}
}

#[test]
fn resize_accepts_a_drop_struct() {
    let mut v = TrackedVec::new();
    v.resize(
        3,
        Tracked {
            id: 1,
            name: String::from("a"),
        },
    );
    assert_eq!(v.len(), 3);
    assert_eq!(v.name[2], "a");
    v.resize(
        1,
        Tracked {
            id: 2,
            name: String::from("b"),
        },
    );
    assert_eq!(v.len(), 1);
}
