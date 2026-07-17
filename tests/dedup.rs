extern crate alloc;

mod particles;
use self::particles::{Particle, ParticleVec};

// `dedup_by` / `dedup_by_key` return the new length; a slice cannot resize, so
// callers must truncate the owning Vec to drop the stale tail.
#[test]
fn dedup_by_name() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Cl"), 35.5));
    v.push(Particle::new(String::from("Cl"), 35.5));
    v.push(Particle::new(String::from("Br"), 80.0));
    let new_len = v.as_mut_slice().dedup_by(|a, b| a.name == b.name);
    assert_eq!(new_len, 3);
    v.truncate(new_len);
    assert_eq!(v.len(), 3);
    assert_eq!(v.index(0).name, "Na");
    assert_eq!(v.index(1).name, "Cl");
    assert_eq!(v.index(2).name, "Br");
}

#[test]
fn dedup_by_all_same() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Na"), 23.0));
    let new_len = v.as_mut_slice().dedup_by(|a, b| a.name == b.name);
    assert_eq!(new_len, 1);
    v.truncate(new_len);
    assert_eq!(v.len(), 1);
    assert_eq!(v.index(0).name, "Na");
}

#[test]
fn dedup_by_no_duplicates() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Cl"), 35.5));
    v.push(Particle::new(String::from("Br"), 80.0));
    let new_len = v.as_mut_slice().dedup_by(|a, b| a.name == b.name);
    assert_eq!(new_len, 3);
    assert_eq!(v.index(0).name, "Na");
    assert_eq!(v.index(1).name, "Cl");
    assert_eq!(v.index(2).name, "Br");
}

#[test]
fn dedup_by_single_element() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    let new_len = v.as_mut_slice().dedup_by(|a, b| a.name == b.name);
    assert_eq!(new_len, 1);
    assert_eq!(v.index(0).name, "Na");
}

#[test]
fn dedup_by_empty() {
    let mut v = ParticleVec::new();
    let new_len = v.as_mut_slice().dedup_by(|a, b| a.name == b.name);
    assert_eq!(new_len, 0);
    assert!(v.is_empty());
}

#[test]
fn dedup_by_mass() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Na2"), 23.0));
    v.push(Particle::new(String::from("Cl"), 35.5));
    v.push(Particle::new(String::from("Cl2"), 35.5));
    let new_len = v.as_mut_slice().dedup_by(|a, b| *a.mass == *b.mass);
    assert_eq!(new_len, 2);
    v.truncate(new_len);
    assert_eq!(v.len(), 2);
    assert_eq!(v.index(0).name, "Na");
    assert_eq!(v.index(1).name, "Cl");
}

#[test]
fn dedup_by_key_name() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Cl"), 35.5));
    let new_len = v.as_mut_slice().dedup_by_key(|p| p.name.clone());
    assert_eq!(new_len, 2);
    v.truncate(new_len);
    assert_eq!(v.len(), 2);
    assert_eq!(v.index(0).name, "Na");
    assert_eq!(v.index(1).name, "Cl");
}

#[test]
fn dedup_by_key_mass() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Na2"), 23.0));
    v.push(Particle::new(String::from("Cl"), 35.5));
    v.push(Particle::new(String::from("Cl2"), 35.5));
    let new_len = v.as_mut_slice().dedup_by_key(|p| *p.mass);
    assert_eq!(new_len, 2);
    v.truncate(new_len);
    assert_eq!(v.len(), 2);
    assert_eq!(v.index(0).name, "Na");
    assert_eq!(v.index(1).name, "Cl");
}

#[test]
fn dedup_alternating_no_consecutive() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Cl"), 35.5));
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Cl"), 35.5));
    // No *consecutive* duplicates -> length unchanged.
    let new_len = v.as_mut_slice().dedup_by(|a, b| a.name == b.name);
    assert_eq!(new_len, 4);
    assert_eq!(v.index(0).name, "Na");
    assert_eq!(v.index(1).name, "Cl");
    assert_eq!(v.index(2).name, "Na");
    assert_eq!(v.index(3).name, "Cl");
}

// Without truncation the stale tail stays behind: dedup compacts the prefix
// but the slice length is unchanged, so old duplicates linger past new_len.
// std's `dedup_by` contract: the predicate receives the two elements in
// opposite order from their order in the slice — `a` is the later
// (removal candidate) element, `b` the earlier retained one.
#[test]
fn dedup_by_argument_order_matches_std() {
    let mut plain = vec![1.0f64, 2.0, 3.0];
    let mut std_order = Vec::new();
    plain.dedup_by(|a, b| {
        std_order.push((*a, *b));
        false
    });
    assert_eq!(std_order, [(2.0, 1.0), (3.0, 2.0)]);

    let mut v = ParticleVec::new();
    for mass in [1.0, 2.0, 3.0] {
        v.push(Particle::new(String::from("p"), mass));
    }
    let mut soa_order = Vec::new();
    let new_len = v.as_mut_slice().dedup_by(|a, b| {
        soa_order.push((*a.mass, *b.mass));
        false
    });
    assert_eq!(new_len, 3);
    assert_eq!(soa_order, std_order);
}

// With an asymmetric predicate the argument order decides which element
// survives, so the result must match what std::vec::Vec::dedup_by keeps.
#[test]
fn dedup_by_asymmetric_predicate_matches_std() {
    let masses = [1.0f64, 3.0, 2.0];

    let mut plain = masses.to_vec();
    plain.dedup_by(|a, b| *a > *b);
    assert_eq!(plain, [1.0]);

    let mut v = ParticleVec::new();
    for mass in masses {
        v.push(Particle::new(String::from("p"), mass));
    }
    let new_len = v.as_mut_slice().dedup_by(|a, b| *a.mass > *b.mass);
    v.truncate(new_len);
    let kept: Vec<f64> = v.iter().map(|p| *p.mass).collect();
    assert_eq!(kept, plain);
}

#[test]
fn dedup_tail_is_stale_without_truncate() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Cl"), 35.5));
    let new_len = v.as_mut_slice().dedup_by(|a, b| a.name == b.name);
    assert_eq!(new_len, 2);
    assert_eq!(v.len(), 3); // not resized
    assert_eq!(v.index(0).name, "Na");
    assert_eq!(v.index(1).name, "Cl");
    assert_eq!(v.index(2).name, "Na"); // stale duplicate still present
    v.truncate(new_len);
    assert_eq!(v.len(), 2);
}
