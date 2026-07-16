extern crate alloc;

mod particles;
use self::particles::{Particle, ParticleVec};

#[test]
fn binary_search_by_found() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Ag"), 107.9));
    v.push(Particle::new(String::from("Au"), 197.0));
    v.push(Particle::new(String::from("Fe"), 55.8));
    v.push(Particle::new(String::from("Zn"), 65.4));
    let slice = v.as_slice();
    assert_eq!(slice.binary_search_by(|p| p.name.as_str().cmp("Fe")), Ok(2));
}

#[test]
fn binary_search_by_not_found() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Ag"), 107.9));
    v.push(Particle::new(String::from("Au"), 197.0));
    v.push(Particle::new(String::from("Fe"), 55.8));
    v.push(Particle::new(String::from("Zn"), 65.4));
    let slice = v.as_slice();
    assert_eq!(
        slice.binary_search_by(|p| p.name.as_str().cmp("Cu")),
        Err(2)
    );
}

#[test]
fn binary_search_by_empty() {
    let v = ParticleVec::new();
    let slice = v.as_slice();
    assert_eq!(
        slice.binary_search_by(|p| p.name.as_str().cmp("Fe")),
        Err(0)
    );
}

#[test]
fn binary_search_by_single_found() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Fe"), 55.8));
    let slice = v.as_slice();
    assert_eq!(slice.binary_search_by(|p| p.name.as_str().cmp("Fe")), Ok(0));
}

#[test]
fn binary_search_by_first_and_last() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Ag"), 107.9));
    v.push(Particle::new(String::from("Au"), 197.0));
    v.push(Particle::new(String::from("Fe"), 55.8));
    let slice = v.as_slice();
    assert_eq!(slice.binary_search_by(|p| p.name.as_str().cmp("Ag")), Ok(0));
    assert_eq!(slice.binary_search_by(|p| p.name.as_str().cmp("Fe")), Ok(2));
}

#[test]
fn binary_search_by_mass() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Fe"), 55.8));
    v.push(Particle::new(String::from("Zn"), 65.4));
    v.push(Particle::new(String::from("Ag"), 107.9));
    v.push(Particle::new(String::from("Au"), 197.0));
    let slice = v.as_slice();
    assert_eq!(
        slice.binary_search_by(|p| p.mass.partial_cmp(&107.9).unwrap()),
        Ok(2)
    );
}

#[test]
fn binary_search_by_key_name() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Ag"), 107.9));
    v.push(Particle::new(String::from("Au"), 197.0));
    v.push(Particle::new(String::from("Fe"), 55.8));
    v.push(Particle::new(String::from("Zn"), 65.4));
    let slice = v.as_slice();
    assert_eq!(
        slice.binary_search_by_key(&String::from("Au"), |p| p.name.clone()),
        Ok(1)
    );
}

#[test]
fn binary_search_by_key_not_found() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Ag"), 107.9));
    v.push(Particle::new(String::from("Au"), 197.0));
    v.push(Particle::new(String::from("Fe"), 55.8));
    let slice = v.as_slice();
    assert_eq!(
        slice.binary_search_by_key(&String::from("Cu"), |p| p.name.clone()),
        Err(2)
    );
}

#[test]
fn binary_search_slice_mut_by() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Ag"), 107.9));
    v.push(Particle::new(String::from("Au"), 197.0));
    v.push(Particle::new(String::from("Fe"), 55.8));
    let slice_mut = v.as_mut_slice();
    assert_eq!(
        slice_mut.binary_search_by(|p| p.name.as_str().cmp("Au")),
        Ok(1)
    );
}

#[test]
fn binary_search_slice_mut_by_key() {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Ag"), 107.9));
    v.push(Particle::new(String::from("Au"), 197.0));
    v.push(Particle::new(String::from("Fe"), 55.8));
    let slice_mut = v.as_mut_slice();
    assert_eq!(
        slice_mut.binary_search_by_key(&String::from("Ag"), |p| p.name.clone()),
        Ok(0)
    );
}
