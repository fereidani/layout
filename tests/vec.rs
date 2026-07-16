#![allow(clippy::float_cmp)]

extern crate alloc;

use alloc::rc::Rc;
use core::cell::Cell;

mod particles;
use layout::SOA;

use self::particles::{Particle, ParticleVec};

#[test]
fn ty() {
    let _: <Particle as SOA>::Type = ParticleVec::new();
}

#[test]
fn push() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Na"), 56.0));

    assert_eq!(particles.name[0], "Na");
    assert_eq!(particles.mass[0], 56.0);
}

#[test]
fn len() {
    let mut particles = ParticleVec::new();
    assert_eq!(particles.len(), 0);
    assert!(particles.is_empty());

    particles.push(Particle::new(String::from("Na"), 56.0));
    particles.push(Particle::new(String::from("Na"), 56.0));
    particles.push(Particle::new(String::from("Na"), 56.0));
    assert_eq!(particles.len(), 3);

    particles.clear();
    assert_eq!(particles.len(), 0);
}

#[test]
fn capacity() {
    let mut particles = ParticleVec::with_capacity(9);
    assert_eq!(particles.len(), 0);
    assert_eq!(particles.capacity(), 9);

    particles.reserve(42);
    assert!(particles.capacity() >= 42);

    particles.reserve_exact(100);
    assert!(particles.capacity() == 100);

    particles.push(Particle::new(String::from("Na"), 56.0));
    particles.push(Particle::new(String::from("Na"), 56.0));
    assert_eq!(particles.len(), 2);
    assert!(particles.capacity() == 100);

    particles.shrink_to_fit();
    assert_eq!(particles.len(), 2);
    assert_eq!(particles.capacity(), 2);
}

#[test]
fn remove() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 0.0));
    particles.push(Particle::new(String::from("Na"), 0.0));
    particles.push(Particle::new(String::from("Br"), 0.0));
    particles.push(Particle::new(String::from("Zn"), 0.0));

    let particle = particles.remove(1);
    assert_eq!(particle.name, "Na");
    assert_eq!(particles.name[0], "Cl");
    assert_eq!(particles.name[1], "Br");
    assert_eq!(particles.name[2], "Zn");
}

#[test]
fn swap_remove() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 0.0));
    particles.push(Particle::new(String::from("Na"), 0.0));
    particles.push(Particle::new(String::from("Br"), 0.0));
    particles.push(Particle::new(String::from("Zn"), 0.0));

    let particle = particles.swap_remove(1);
    assert_eq!(particle.name, "Na");
    assert_eq!(particles.index(0).name, "Cl");
    assert_eq!(particles.index(1).name, "Zn");
    assert_eq!(particles.index(2).name, "Br");
}

#[test]
fn insert() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 0.0));
    particles.push(Particle::new(String::from("Na"), 0.0));

    particles.insert(1, Particle::new(String::from("Zn"), 0.0));
    assert_eq!(particles.index(0).name, "Cl");
    assert_eq!(particles.index(1).name, "Zn");
    assert_eq!(particles.index(2).name, "Na");
}

// Regression: `index == len` is valid append semantics for `Vec::insert`. The
// old guard `index >= self.len()` rejected it (and made inserting into an empty
// vec at index 0 impossible). Fixed to `index > self.len()`.
#[test]
fn insert_at_len_is_append() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 0.0));
    particles.push(Particle::new(String::from("Na"), 0.0));

    particles.insert(2, Particle::new(String::from("Zn"), 0.0));
    assert_eq!(particles.len(), 3);
    assert_eq!(particles.index(0).name, "Cl");
    assert_eq!(particles.index(1).name, "Na");
    assert_eq!(particles.index(2).name, "Zn");
}

#[test]
fn insert_into_empty_at_zero() {
    let mut particles = ParticleVec::new();
    particles.insert(0, Particle::new(String::from("Cl"), 0.0));
    assert_eq!(particles.len(), 1);
    assert_eq!(particles.index(0).name, "Cl");
}

#[test]
#[should_panic(expected = "should be <= len")]
fn insert_out_of_bounds_panics() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 0.0));
    // index > len is still out of bounds.
    particles.insert(5, Particle::new(String::from("Zn"), 0.0));
}

#[test]
fn pop() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 0.0));
    particles.push(Particle::new(String::from("Na"), 0.0));

    let particle = particles.pop();
    assert_eq!(particle, Some(Particle::new(String::from("Na"), 0.0)));

    let particle = particles.pop();
    assert_eq!(particle, Some(Particle::new(String::from("Cl"), 0.0)));

    let particle = particles.pop();
    assert_eq!(particle, None)
}

#[test]
fn append() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 0.0));
    particles.push(Particle::new(String::from("Na"), 0.0));

    let mut others = ParticleVec::new();
    others.push(Particle::new(String::from("Zn"), 0.0));
    others.push(Particle::new(String::from("Mg"), 0.0));

    particles.append(&mut others);
    assert_eq!(particles.index(0).name, "Cl");
    assert_eq!(particles.index(1).name, "Na");
    assert_eq!(particles.index(2).name, "Zn");
    assert_eq!(particles.index(3).name, "Mg");
}

#[test]
fn split_off() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 0.0));
    particles.push(Particle::new(String::from("Na"), 0.0));
    particles.push(Particle::new(String::from("Zn"), 0.0));
    particles.push(Particle::new(String::from("Mg"), 0.0));

    let other = particles.split_off(2);
    assert_eq!(particles.len(), 2);
    assert_eq!(other.len(), 2);

    assert_eq!(particles.index(0).name, "Cl");
    assert_eq!(particles.index(1).name, "Na");
    assert_eq!(other.index(0).name, "Zn");
    assert_eq!(other.index(1).name, "Mg");
}

#[test]
fn retain() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 0.0));
    particles.push(Particle::new(String::from("Na"), 0.0));
    particles.push(Particle::new(String::from("Zn"), 0.0));
    particles.push(Particle::new(String::from("C"), 0.0));

    particles.retain(|particle| particle.name.starts_with('C'));
    assert_eq!(particles.len(), 2);
    assert_eq!(particles.index(0).name, "Cl");
    assert_eq!(particles.index(1).name, "C");
}

#[test]
fn retain_mut() {
    let mut particles = ParticleVec::new();
    particles.push(Particle::new(String::from("Cl"), 1.0));
    particles.push(Particle::new(String::from("Na"), 1.0));
    particles.push(Particle::new(String::from("Zn"), 0.0));
    particles.push(Particle::new(String::from("C"), 1.0));

    particles.retain_mut(|particle| {
        particle.name.make_ascii_uppercase();
        *particle.mass > 0.5
    });
    assert_eq!(particles.len(), 3);
    assert!(["CL", "NA", "C"].iter().copied().eq(particles.name.iter()));
}

#[derive(SOA)]
struct IncrOnDrop {
    cell: Rc<Cell<usize>>,
}

impl Drop for IncrOnDrop {
    fn drop(&mut self) {
        self.cell.set(self.cell.get() + 1);
    }
}

#[test]
fn drop_vec() {
    let counter = Rc::new(Cell::default());
    let mut vec = IncrOnDropVec::new();
    for _ in 0..5 {
        vec.push(IncrOnDrop {
            cell: counter.clone(),
        });
    }

    assert_eq!(counter.get(), 0);
    drop(vec);
    assert_eq!(counter.get(), 5);
}

// Regression for the panic-safety (double-drop on unwind) fix: `push`, `insert`,
// and `replace` consume their argument by bitwise-copying each field out via
// `ptr::read` and then wrapping the value in `ManuallyDrop` instead of calling
// `mem::forget`. This test verifies the HAPPY path still drops each user value
// exactly once (no double-drop, no leak) across push/insert/replace/pop/drop.
// (The unwind path itself is not testable here since Vec push only fails on
// OOM, which aborts.)
#[test]
fn happy_path_drops_each_value_exactly_once() {
    let counter = Rc::new(Cell::default());

    // Helper to spawn a value tied to the shared drop counter.
    let mk = || IncrOnDrop {
        cell: counter.clone(),
    };

    let mut vec = IncrOnDropVec::new();

    // 3 outstanding values after push.
    vec.push(mk());
    vec.push(mk());
    vec.push(mk());
    assert_eq!(counter.get(), 0, "push must not drop the consumed value");

    // insert adds a 4th outstanding value (no drop).
    vec.insert(1, mk());
    assert_eq!(counter.get(), 0, "insert must not drop the consumed value");

    // replace returns the OLD value (which we drop) and consumes the NEW one
    // (kept in the vec) -> exactly one drop here for the displaced old value.
    let old = vec.replace(0, mk());
    assert_eq!(counter.get(), 0, "replace must not drop the consumed new value");
    drop(old);
    assert_eq!(counter.get(), 1, "replace returns the old value, dropping it once");

    // 4 values still live in the vec. pop one out and drop it -> one more drop.
    let popped = vec.pop();
    assert!(popped.is_some());
    assert_eq!(
        counter.get(),
        1,
        "pop must move the value out without dropping it"
    );
    drop(popped);
    assert_eq!(counter.get(), 2, "popped value drops exactly once");

    // 3 values remain in the vec; dropping it must drop each exactly once.
    drop(vec);
    assert_eq!(counter.get(), 5, "dropping the vec drops each remaining value once");

    // Final check: no value was double-dropped or leaked. The total number of
    // drops (5) equals the total number of values created (5: 3 push + 1 insert
    // + 1 replace-new), and every created value was accounted for.
    assert_eq!(counter.get(), 5);
}
