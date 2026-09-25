//! Generated code must not depend on names the deriving module happens to
//! have in scope.

// A module of the user's own named `layout` (a UI layout module, say) must
// not capture the generated paths into this crate.
#[allow(dead_code)]
mod layout {}

#[allow(dead_code)]
mod shapes {
    // Derived by path, with nothing from the crate imported.
    #[derive(::layout::SOA)]
    pub struct Point {
        pub x: f32,
        pub y: f32,
    }

    #[derive(::layout::SOA)]
    pub struct Body {
        #[nested_soa]
        pub center: Point,
        pub mass: f32,
    }
}

#[test]
fn derive_by_path_without_imports() {
    let mut bodies = shapes::BodyVec::with_capacity(2);
    bodies.push(shapes::Body {
        center: shapes::Point { x: 3.0, y: 4.0 },
        mass: 1.0,
    });
    assert_eq!(bodies.len(), 1);
}
