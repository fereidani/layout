extern crate alloc;

mod particles;
use self::particles::{Particle, ParticleVec};

fn make_particles() -> ParticleVec {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Cl"), 35.5));
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Br"), 80.0));
    v.push(Particle::new(String::from("Zn"), 65.4));
    v.push(Particle::new(String::from("Fe"), 55.8));
    v
}

#[test]
fn drain_full() {
    let mut v = make_particles();
    let drained: alloc::vec::Vec<_> = v.drain(..).collect();
    assert_eq!(drained.len(), 5);
    assert_eq!(drained[0].name, "Cl");
    assert_eq!(drained[4].name, "Fe");
    assert!(v.is_empty());
}

#[test]
fn drain_range() {
    let mut v = make_particles();
    let drained: alloc::vec::Vec<_> = v.drain(1..3).collect();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].name, "Na");
    assert_eq!(drained[1].name, "Br");
}

#[test]
fn drain_from() {
    let mut v = make_particles();
    let drained: alloc::vec::Vec<_> = v.drain(3..).collect();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].name, "Zn");
    assert_eq!(drained[1].name, "Fe");
}

#[test]
fn drain_single() {
    let mut v = make_particles();
    let drained: alloc::vec::Vec<_> = v.drain(2..3).collect();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].name, "Br");
    drop(v);
}

#[test]
fn drain_empty_range() {
    let mut v = make_particles();
    let drained: alloc::vec::Vec<_> = v.drain(2..2).collect();
    assert!(drained.is_empty());
}

#[test]
fn drain_partial_consume() {
    let mut v = make_particles();
    let mut drain = v.drain(1..4);
    let first = drain.next().unwrap();
    assert_eq!(first.name, "Na");
}

#[test]
fn drain_next_back() {
    let mut v = make_particles();
    let mut drain = v.drain(1..4);
    let last = drain.next_back().unwrap();
    assert_eq!(last.name, "Zn");
    let first = drain.next().unwrap();
    assert_eq!(first.name, "Na");
    let last2 = drain.next_back().unwrap();
    assert_eq!(last2.name, "Br");
    assert!(drain.next().is_none());
}

#[test]
fn drain_len() {
    let mut v = make_particles();
    let mut drain = v.drain(1..3);
    assert_eq!(drain.len(), 2);
    drain.next();
    assert_eq!(drain.len(), 1);
    drain.next();
    assert_eq!(drain.len(), 0);
}

#[test]
fn drain_size_hint() {
    let mut v = make_particles();
    let drain = v.drain(1..3);
    assert_eq!(drain.size_hint(), (2, Some(2)));
}

#[test]
fn drain_then_push() {
    let mut v = make_particles();
    {
        let drained: alloc::vec::Vec<_> = v.drain(1..3).collect();
        assert_eq!(drained.len(), 2);
    }
    // After drain completes, the vec should be usable again
    v.push(Particle::new(String::from("Au"), 197.0));
    assert_eq!(v.len(), 4);
    assert_eq!(v.index(0).name, "Cl");
    assert_eq!(v.index(3).name, "Au");
}
