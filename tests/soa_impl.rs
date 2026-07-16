// The `#[soa_impl]` regression methods below deliberately use the exact code
// shapes that used to break the generated Ref/RefMut methods (e.g.
// `let x = self.field; x`, `if self.flag { true } else { false }`, equality
// guards in `match`). Those shapes trip a few clippy style lints; the same
// shapes are cloned into the generated impls, so silence the lints file-wide.
#![allow(
    clippy::let_and_return,
    clippy::needless_bool,
    clippy::redundant_guards
)]
use layout::{soa_impl, Compact, CompactBool, SOA};

#[derive(Debug, Clone, PartialEq, SOA)]
pub struct Particle {
    pub name: String,
    pub mass: f64,
}

impl Particle {
    pub fn new(name: String, mass: f64) -> Self {
        Particle { name, mass }
    }
}

// ---------------------------------------------------------------------------
// Test 1: Basic &self method with Copy field arithmetic → Ref
// ---------------------------------------------------------------------------
#[soa_impl]
impl Particle {
    pub fn kinetic_energy(&self, velocity: f64) -> f64 {
        0.5 * self.mass * velocity * velocity
    }
}

#[test]
fn ref_kinetic_energy() {
    let mut vec = ParticleVec::new();
    vec.push(Particle::new("Na".into(), 23.0));

    let r = vec.index(0);
    let energy = r.kinetic_energy(2.0);
    let expected = 0.5 * 23.0 * 2.0 * 2.0;
    assert!((energy - expected).abs() < f64::EPSILON);
}

// ---------------------------------------------------------------------------
// Test 2: &self method with method call on non-Copy field → Ref (no transform)
// ---------------------------------------------------------------------------
#[soa_impl]
impl Particle {
    pub fn name_len(&self) -> usize {
        self.name.len()
    }

    pub fn summary(&self) -> String {
        format!("{}: {}", self.name, self.mass)
    }
}

#[test]
fn ref_method_call_on_string() {
    let mut vec = ParticleVec::new();
    vec.push(Particle::new("Sodium".into(), 23.0));

    let r = vec.index(0);
    assert_eq!(r.name_len(), 6);
    assert_eq!(r.summary(), "Sodium: 23");
}

// ---------------------------------------------------------------------------
// Test 3: &mut self compound assignment → RefMut
// ---------------------------------------------------------------------------
#[soa_impl]
impl Particle {
    pub fn scale_mass(&mut self, factor: f64) {
        self.mass *= factor;
    }
}

#[test]
fn ref_mut_compound_assign() {
    let mut vec = ParticleVec::new();
    vec.push(Particle::new("Na".into(), 23.0));

    vec.index_mut(0).scale_mass(2.0);
    assert_eq!(vec.mass[0], 46.0);
}

// ---------------------------------------------------------------------------
// Test 4: &mut self direct assignment → RefMut
// ---------------------------------------------------------------------------
#[soa_impl]
impl Particle {
    pub fn set_mass(&mut self, mass: f64) {
        self.mass = mass;
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
}

#[test]
fn ref_mut_direct_assign() {
    let mut vec = ParticleVec::new();
    vec.push(Particle::new("Na".into(), 23.0));

    vec.index_mut(0).set_mass(100.0);
    assert_eq!(vec.mass[0], 100.0);

    vec.index_mut(0).set_name("Helium".into());
    assert_eq!(vec.name[0], "Helium");
}

// ---------------------------------------------------------------------------
// Test 5: Associated function → NOT generated for Ref/RefMut
//         (verified by not having compilation errors; the function only exists
// on Particle)
// ---------------------------------------------------------------------------
#[soa_impl]
impl Particle {
    pub fn create_default() -> Self {
        Particle {
            name: "unknown".into(),
            mass: 0.0,
        }
    }
}

#[test]
fn associated_function_only_on_original() {
    let p = Particle::create_default();
    assert_eq!(p.name, "unknown");
    assert_eq!(p.mass, 0.0);
}

// ---------------------------------------------------------------------------
// Test 6: Method returning Self → NOT generated for Ref/RefMut
// ---------------------------------------------------------------------------
#[soa_impl]
impl Particle {
    pub fn with_mass(&self, mass: f64) -> Self {
        Particle {
            name: self.name.clone(),
            mass,
        }
    }
}

#[test]
fn self_return_skipped_for_ref() {
    let p = Particle::new("Na".into(), 23.0);
    let p2 = p.with_mass(50.0);
    assert_eq!(p2.name, "Na");
    assert_eq!(p2.mass, 50.0);
}

// ---------------------------------------------------------------------------
// Test 7: Comparison operators in &self method → Ref
// ---------------------------------------------------------------------------
#[soa_impl]
impl Particle {
    pub fn is_heavy(&self) -> bool {
        self.mass > 10.0
    }

    pub fn is_light(&self) -> bool {
        self.mass < 5.0
    }

    pub fn mass_equals(&self, value: f64) -> bool {
        self.mass == value
    }
}

#[test]
fn ref_comparison_operators() {
    let mut vec = ParticleVec::new();
    vec.push(Particle::new("Heavy".into(), 50.0));
    vec.push(Particle::new("Light".into(), 2.0));

    assert!(vec.index(0).is_heavy());
    assert!(!vec.index(1).is_heavy());
    assert!(vec.index(1).is_light());
    assert!(!vec.index(0).is_light());
    assert!(vec.index(0).mass_equals(50.0));
}

// ---------------------------------------------------------------------------
// Test 8: Multiple fields in one expression → Ref
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, SOA)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

#[soa_impl]
impl Vec2 {
    pub fn dot(&self, other: &Vec2) -> f64 {
        self.x * other.x + self.y * other.y
    }

    pub fn magnitude(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn sum(&self) -> f64 {
        self.x + self.y
    }
}

#[test]
fn ref_multiple_fields_expression() {
    let mut vec = Vec2Vec::new();
    vec.push(Vec2 { x: 3.0, y: 4.0 });

    let r = vec.index(0);
    let dot = r.dot(&Vec2 { x: 1.0, y: 0.0 });
    assert!((dot - 3.0).abs() < 1e-10);

    let mag = r.magnitude();
    assert!((mag - 5.0).abs() < 1e-10);

    let s = r.sum();
    assert!((s - 7.0).abs() < 1e-10);
}

// ---------------------------------------------------------------------------
// Test 9: Mixed impl block with all method types
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, SOA)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[soa_impl]
impl Point {
    // Associated function — should NOT appear on Ref/RefMut
    pub fn origin() -> Self {
        Point { x: 0.0, y: 0.0 }
    }

    // &self method — should appear on Ref
    pub fn distance_from_origin(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    // &mut self method — should appear on RefMut
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.x += dx;
        self.y += dy;
    }

    // Method returning Self — should NOT appear on Ref/RefMut
    pub fn doubled(&self) -> Self {
        Point {
            x: self.x * 2.0,
            y: self.y * 2.0,
        }
    }
}

#[test]
fn mixed_impl_block() {
    let mut vec = PointVec::new();
    vec.push(Point { x: 3.0, y: 4.0 });

    // Ref: &self method
    let dist = vec.index(0).distance_from_origin();
    assert!((dist - 5.0).abs() < 1e-5);

    // RefMut: &mut self method
    vec.index_mut(0).translate(1.0, 1.0);
    assert_eq!(vec.x[0], 4.0);
    assert_eq!(vec.y[0], 5.0);

    // Original: associated function
    let origin = Point::origin();
    assert_eq!(origin.x, 0.0);

    // Original: method returning Self
    let p = Point { x: 3.0, y: 4.0 };
    let doubled = p.doubled();
    assert_eq!(doubled.x, 6.0);
}

// ---------------------------------------------------------------------------
// Test 10: Unary negation on self.field
// ---------------------------------------------------------------------------
#[soa_impl]
impl Vec2 {
    pub fn negate_x(&self) -> f64 {
        -self.x
    }
}

#[test]
fn ref_unary_negation() {
    let mut vec = Vec2Vec::new();
    vec.push(Vec2 { x: 3.0, y: 4.0 });

    let r = vec.index(0);
    assert_eq!(r.negate_x(), -3.0);
}

// ---------------------------------------------------------------------------
// Test 11: Cast expression on self.field
// ---------------------------------------------------------------------------
#[soa_impl]
impl Vec2 {
    pub fn x_as_i32(&self) -> i32 {
        self.x as i32
    }
}

#[test]
fn ref_cast_expression() {
    let mut vec = Vec2Vec::new();
    vec.push(Vec2 { x: 3.7, y: 4.2 });

    let r = vec.index(0);
    assert_eq!(r.x_as_i32(), 3);
}

// ---------------------------------------------------------------------------
// Test 12: Using Ref methods through iteration
// ---------------------------------------------------------------------------
#[test]
fn ref_methods_through_iteration() {
    let mut vec = ParticleVec::new();
    vec.push(Particle::new("Na".into(), 23.0));
    vec.push(Particle::new("Cl".into(), 35.5));

    let mut total_energy = 0.0;
    for p in vec.iter() {
        total_energy += p.kinetic_energy(1.0);
    }
    let expected = 0.5 * 23.0 + 0.5 * 35.5;
    assert!((total_energy - expected).abs() < 1e-10);
}

// ---------------------------------------------------------------------------
// Test 13: Using RefMut methods through mutable iteration
// ---------------------------------------------------------------------------
#[test]
fn ref_mut_methods_through_iteration() {
    let mut vec = ParticleVec::new();
    vec.push(Particle::new("Na".into(), 23.0));
    vec.push(Particle::new("Cl".into(), 35.5));

    for mut p in vec.iter_mut() {
        p.scale_mass(2.0);
    }
    assert_eq!(vec.mass[0], 46.0);
    assert_eq!(vec.mass[1], 71.0);
}

// ---------------------------------------------------------------------------
// Test 14: Boolean field with negation
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, SOA)]
pub struct Toggle {
    pub active: bool,
    pub label: String,
}

#[soa_impl]
impl Toggle {
    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn toggle(&mut self) {
        self.active = !self.active;
    }

    pub fn label_len(&self) -> usize {
        self.label.len()
    }

    pub fn set_label(&mut self, label: String) {
        self.label = label;
    }
}

#[test]
fn bool_field_with_negation() {
    let mut vec = ToggleVec::new();
    vec.push(Toggle {
        active: true,
        label: "test".into(),
    });

    assert!(vec.index(0).is_active());
    assert_eq!(vec.index(0).label_len(), 4);

    vec.index_mut(0).toggle();
    assert!(!vec.index(0).is_active());

    vec.index_mut(0).set_label("longer label".into());
    assert_eq!(vec.index(0).label_len(), 12);
}

// ---------------------------------------------------------------------------
// Test 15: CompactBool field with soa_impl — method calls on Ref/RefMut
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, SOA)]
pub struct Task {
    pub name: String,
    pub done: CompactBool,
    pub priority: u32,
}

impl Task {
    pub fn new(name: &str, done: bool, priority: u32) -> Self {
        Task {
            name: name.into(),
            done: Compact(done),
            priority,
        }
    }
}

#[soa_impl]
impl Task {
    pub fn is_done(&self) -> bool {
        self.done.get()
    }

    pub fn status(&self) -> String {
        let state = if self.done.get() { "done" } else { "pending" };
        format!("{}: {} (p={})", self.name, state, self.priority)
    }

    pub fn complete(&mut self) {
        self.done.set(true);
    }

    pub fn reopen(&mut self) {
        self.done.set(false);
    }

    pub fn toggle_done(&mut self) {
        self.done.set(!self.done.get());
    }

    pub fn set_priority(&mut self, p: u32) {
        self.priority = p;
    }
}

#[test]
fn compact_bool_ref_methods() {
    let mut vec = TaskVec::new();
    vec.push(Task::new("write tests", false, 1));
    vec.push(Task::new("fix bug", true, 3));

    assert!(!vec.index(0).is_done());
    assert!(vec.index(1).is_done());

    let status = vec.index(0).status();
    assert_eq!(status, "write tests: pending (p=1)");
}

#[test]
fn compact_bool_ref_mut_set() {
    let mut vec = TaskVec::new();
    vec.push(Task::new("write tests", false, 1));

    vec.index_mut(0).complete();
    assert!(vec.index(0).is_done());

    vec.index_mut(0).reopen();
    assert!(!vec.index(0).is_done());
}

#[test]
fn compact_bool_ref_mut_flip() {
    let mut vec = TaskVec::new();
    vec.push(Task::new("task", false, 0));
    vec.push(Task::new("task", true, 0));

    vec.index_mut(0).toggle_done();
    assert!(vec.index(0).is_done());

    vec.index_mut(1).toggle_done();
    assert!(!vec.index(1).is_done());
}

#[test]
fn compact_bool_mixed_fields() {
    let mut vec = TaskVec::new();
    vec.push(Task::new("a", false, 1));
    vec.push(Task::new("b", true, 2));
    vec.push(Task::new("c", false, 3));

    // Use soa_impl methods alongside normal field access
    vec.index_mut(0).complete();
    vec.index_mut(0).set_priority(10);

    assert!(vec.index(0).is_done());
    assert_eq!(vec.priority[0], 10);

    // Flip all tasks through iteration
    for mut task in vec.iter_mut() {
        task.toggle_done();
    }
    assert!(!vec.index(0).is_done());
    assert!(!vec.index(1).is_done());
    assert!(vec.index(2).is_done());
}

// ---------------------------------------------------------------------------
// Regression: bare self.field reads in value positions previously left as &T.
//
// On Ref<'a> each field is &T (or &mut T on RefMut). A bare `self.field` read
// in a let-initializer / if-condition / assignment-RHS / match-scrutinee /
// closure-body used to compile on the owned struct but FAIL on the generated
// Ref/RefMut because the transformer only wrapped a closed set of parent kinds.
// These tests exercise all of those positions on both Ref and RefMut.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, SOA)]
pub struct Sample {
    pub a: f64,       // Copy field
    pub name: String, // non-Copy field
    pub flag: bool,   // bool field
}

impl Sample {
    pub fn new(a: f64, name: &str, flag: bool) -> Self {
        Sample {
            a,
            name: name.into(),
            flag,
        }
    }
}

#[soa_impl]
impl Sample {
    /// `let x = self.a; x` — let-initializer becomes &T without the fix.
    pub fn read_a_via_let(&self) -> f64 {
        let x = self.a;
        x
    }

    /// `if self.flag { ... }` — &bool is not bool without the fix.
    pub fn flag_via_if(&self) -> bool {
        if self.flag {
            true
        } else {
            false
        }
    }

    /// `*out = self.a;` — plain assignment RHS.
    pub fn write_a_into(&self, out: &mut f64) {
        *out = self.a;
    }

    /// `match self.a { ... }` — bare field as a match scrutinee.
    pub fn classify_a(&self) -> &'static str {
        match self.a {
            v if v < 0.0 => "negative",
            v if v == 0.0 => "zero",
            _ => "positive",
        }
    }

    /// A closure capturing nothing that reads a field in its body.
    pub fn closure_reading_a(&self) -> f64 {
        let f = || self.a;
        f()
    }

    /// Tail-expression bare field read (already worked, kept for coverage).
    pub fn tail_a(&self) -> f64 {
        self.a
    }
}

#[soa_impl]
impl Sample {
    /// RefMut: `let x = self.a; x` then mutably observed via return.
    pub fn read_a_via_let_mut(&mut self) -> f64 {
        let x = self.a;
        x
    }

    /// RefMut: assignment RHS into an out-parameter.
    pub fn write_a_into_mut(&mut self, out: &mut f64) {
        *out = self.a;
    }

    /// RefMut: if-condition on a bool field, also mutating the field.
    pub fn flip_if_flag(&mut self) -> bool {
        if self.flag {
            self.flag = !self.flag;
            true
        } else {
            false
        }
    }

    /// RefMut: match scrutinee on a Copy field.
    pub fn classify_a_mut(&mut self) -> &'static str {
        match self.a {
            v if v < 0.0 => "negative",
            v if v == 0.0 => "zero",
            _ => "positive",
        }
    }
}

#[test]
fn ref_bare_field_value_positions() {
    let mut vec = SampleVec::new();
    vec.push(Sample::new(-3.5, "neg", false));
    vec.push(Sample::new(0.0, "zero", true));
    vec.push(Sample::new(7.0, "pos", true));

    let r0 = vec.index(0);
    assert_eq!(r0.read_a_via_let(), -3.5);
    assert!(!r0.flag_via_if());

    let r1 = vec.index(1);
    assert!(r1.flag_via_if());

    let mut out = 0.0_f64;
    vec.index(2).write_a_into(&mut out);
    assert_eq!(out, 7.0);

    assert_eq!(vec.index(0).classify_a(), "negative");
    assert_eq!(vec.index(1).classify_a(), "zero");
    assert_eq!(vec.index(2).classify_a(), "positive");

    assert_eq!(vec.index(2).closure_reading_a(), 7.0);
    assert_eq!(vec.index(2).tail_a(), 7.0);
}

#[test]
fn ref_mut_bare_field_value_positions() {
    let mut vec = SampleVec::new();
    vec.push(Sample::new(-3.5, "neg", false));
    vec.push(Sample::new(0.0, "zero", true));

    assert_eq!(vec.index_mut(0).read_a_via_let_mut(), -3.5);

    let mut out = 0.0_f64;
    vec.index_mut(0).write_a_into_mut(&mut out);
    assert_eq!(out, -3.5);

    // flag starts false on row 0 -> no flip
    assert!(!vec.index_mut(0).flip_if_flag());
    // flag starts true on row 1 -> flip to false
    assert!(vec.index_mut(1).flip_if_flag());
    assert!(!vec.index(1).flag);

    assert_eq!(vec.index_mut(0).classify_a_mut(), "negative");
    assert_eq!(vec.index_mut(1).classify_a_mut(), "zero");
}

// ---------------------------------------------------------------------------
// Regression: `&self` methods must be reachable on `RefMut` too — a mutable
// borrow can do everything an immutable one can (mirrors `&mut T` calling
// `&self` methods). Covers both a Copy field and a non-Copy field.
// ---------------------------------------------------------------------------
#[test]
fn refmut_can_call_ref_methods() {
    let mut vec = ParticleVec::new();
    vec.push(Particle::new("Na".into(), 23.0));

    let m = vec.index_mut(0);
    // `&self` method reading a Copy field (mass).
    assert!((m.kinetic_energy(2.0) - 46.0).abs() < f64::EPSILON);
    // `&self` method calling through a non-Copy field (name: String).
    assert_eq!(m.name_len(), 2);

    // `&self` method whose body reads `self.field` inside a method-call
    // receiver — `(self.x * self.x + self.y * self.y).sqrt()`. The operand
    // reads must be wrapped so this compiles on RefMut too.
    let mut v2 = Vec2Vec::new();
    v2.push(Vec2 { x: 3.0, y: 4.0 });
    assert_eq!(v2.index_mut(0).magnitude(), 5.0);
}
