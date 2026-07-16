extern crate alloc;

mod particles;
use self::particles::{Particle, ParticleVec};

fn make_particles() -> ParticleVec {
    let mut v = ParticleVec::new();
    v.push(Particle::new(String::from("Na"), 23.0));
    v.push(Particle::new(String::from("Cl"), 35.5));
    v.push(Particle::new(String::from("Br"), 80.0));
    v.push(Particle::new(String::from("Zn"), 65.4));
    v.push(Particle::new(String::from("Fe"), 55.8));
    v.push(Particle::new(String::from("Au"), 197.0));
    v.push(Particle::new(String::from("Ag"), 107.9));
    v
}

// --- Immutable chunks ---

#[test]
fn chunks_exact_division() {
    let v = make_particles();
    let slice = v.as_slice();
    let chunks: alloc::vec::Vec<_> = slice.chunks(2).collect();
    assert_eq!(chunks.len(), 4);
    assert_eq!(chunks[0].len(), 2);
    assert_eq!(chunks[0].index(0).name, "Na");
    assert_eq!(chunks[0].index(1).name, "Cl");
}

#[test]
fn chunks_with_remainder() {
    let v = make_particles();
    let slice = v.as_slice();
    let chunks: alloc::vec::Vec<_> = slice.chunks(3).collect();
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].len(), 3);
    assert_eq!(chunks[1].len(), 3);
    assert_eq!(chunks[2].len(), 1);
    assert_eq!(chunks[2].index(0).name, "Ag");
}

#[test]
fn chunks_size_one() {
    let v = make_particles();
    let slice = v.as_slice();
    let chunks: alloc::vec::Vec<_> = slice.chunks(1).collect();
    assert_eq!(chunks.len(), 7);
    for chunk in &chunks {
        assert_eq!(chunk.len(), 1);
    }
}

#[test]
fn chunks_equals_len() {
    let v = make_particles();
    let slice = v.as_slice();
    let chunks: alloc::vec::Vec<_> = slice.chunks(7).collect();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), 7);
}

#[test]
fn chunks_larger_than_len() {
    let v = make_particles();
    let slice = v.as_slice();
    let chunks: alloc::vec::Vec<_> = slice.chunks(10).collect();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].len(), 7);
}

#[test]
fn chunks_size_hint() {
    let v = make_particles();
    let slice = v.as_slice();
    let mut iter = slice.chunks(3);
    assert_eq!(iter.size_hint(), (3, Some(3)));
    iter.next();
    assert_eq!(iter.size_hint(), (2, Some(2)));
}

#[test]
fn chunks_count() {
    let v = make_particles();
    let slice = v.as_slice();
    assert_eq!(slice.chunks(2).count(), 4);
    assert_eq!(slice.chunks(3).count(), 3);
    assert_eq!(slice.chunks(7).count(), 1);
}

// --- Immutable chunks_exact ---

#[test]
fn chunks_exact_basic() {
    let v = make_particles();
    let slice = v.as_slice();
    let mut iter = slice.chunks_exact(3);
    let c1 = iter.next().unwrap();
    assert_eq!(c1.len(), 3);
    assert_eq!(c1.index(0).name, "Na");
    assert_eq!(c1.index(2).name, "Br");

    let c2 = iter.next().unwrap();
    assert_eq!(c2.len(), 3);
    assert_eq!(c2.index(0).name, "Zn");

    assert!(iter.next().is_none());
}

#[test]
fn chunks_exact_remainder() {
    let v = make_particles();
    let slice = v.as_slice();
    let mut iter = slice.chunks_exact(3);
    iter.next();
    iter.next();
    let rem = iter.remainder();
    assert_eq!(rem.len(), 1);
    assert_eq!(rem.index(0).name, "Ag");
}

#[test]
fn chunks_exact_no_remainder() {
    let mut v = ParticleVec::new();
    for name in ["Na", "Cl", "Br", "Zn"] {
        v.push(Particle::new(String::from(name), 0.0));
    }
    let slice = v.as_slice();
    let mut iter = slice.chunks_exact(2);
    iter.next();
    iter.next();
    let rem = iter.remainder();
    assert_eq!(rem.len(), 0);
}

#[test]
fn chunks_exact_count() {
    let v = make_particles();
    let slice = v.as_slice();
    assert_eq!(slice.chunks_exact(3).count(), 2);
    assert_eq!(slice.chunks_exact(7).count(), 1);
}

// --- Mutable chunks_mut ---

#[test]
fn chunks_mut_basic() {
    let mut v = make_particles();
    {
        let mut slice_mut = v.as_mut_slice();
        let chunks: alloc::vec::Vec<_> = slice_mut.chunks_mut(3).collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 3);
        assert_eq!(chunks[1].len(), 3);
        assert_eq!(chunks[2].len(), 1);
    }
}

#[test]
fn chunks_mut_modify() {
    let mut v = make_particles();
    {
        let mut slice_mut = v.as_mut_slice();
        for mut chunk in slice_mut.chunks_mut(3) {
            for i in 0..chunk.len() {
                chunk.get_mut(i).unwrap().name.make_ascii_uppercase();
            }
        }
    }
    assert_eq!(v.index(0).name, "NA");
    assert_eq!(v.index(3).name, "ZN");
}

#[test]
fn chunks_mut_count() {
    let mut v = make_particles();
    {
        let mut slice_mut = v.as_mut_slice();
        assert_eq!(slice_mut.chunks_mut(2).count(), 4);
        assert_eq!(slice_mut.chunks_mut(3).count(), 3);
    }
}

// --- Mutable chunks_exact_mut ---

#[test]
fn chunks_exact_mut_basic() {
    let mut v = make_particles();
    {
        let mut slice_mut = v.as_mut_slice();
        let mut iter = slice_mut.chunks_exact_mut(3);
        let c1 = iter.next().unwrap();
        assert_eq!(c1.len(), 3);

        let c2 = iter.next().unwrap();
        assert_eq!(c2.len(), 3);

        assert!(iter.next().is_none());
    }
}

#[test]
fn chunks_exact_mut_remainder() {
    let mut v = make_particles();
    {
        let mut slice_mut = v.as_mut_slice();
        let mut iter = slice_mut.chunks_exact_mut(3);
        iter.next();
        iter.next();
        let rem = iter.into_remainder();
        assert_eq!(rem.len(), 1);
    }
}

#[test]
fn chunks_exact_mut_modify() {
    let mut v = make_particles();
    {
        let mut slice_mut = v.as_mut_slice();
        let mut iter = slice_mut.chunks_exact_mut(3);
        let mut c1 = iter.next().unwrap();
        for i in 0..c1.len() {
            c1.get_mut(i).unwrap().name.make_ascii_uppercase();
        }
        let mut c2 = iter.next().unwrap();
        for i in 0..c2.len() {
            c2.get_mut(i).unwrap().name.make_ascii_uppercase();
        }
    }
    // First 6 should be uppercased, last one (remainder) should not
    assert_eq!(v.index(0).name, "NA");
    assert_eq!(v.index(5).name, "AU");
    assert_eq!(v.index(6).name, "Ag");
}

// --- Edge cases ---

#[test]
#[should_panic(expected = "chunk size must be non-zero")]
fn chunks_zero_panics() {
    let v = make_particles();
    let slice = v.as_slice();
    slice.chunks(0);
}

#[test]
#[should_panic(expected = "chunk size must be non-zero")]
fn chunks_exact_zero_panics() {
    let v = make_particles();
    let slice = v.as_slice();
    slice.chunks_exact(0);
}

#[test]
#[should_panic(expected = "chunk size must be non-zero")]
fn chunks_mut_zero_panics() {
    let mut v = make_particles();
    let mut slice_mut = v.as_mut_slice();
    slice_mut.chunks_mut(0);
}

#[test]
#[should_panic(expected = "chunk size must be non-zero")]
fn chunks_exact_mut_zero_panics() {
    let mut v = make_particles();
    let mut slice_mut = v.as_mut_slice();
    slice_mut.chunks_exact_mut(0);
}

#[test]
fn chunks_empty_slice() {
    let v = ParticleVec::new();
    let slice = v.as_slice();
    assert_eq!(slice.chunks(3).count(), 0);
    assert_eq!(slice.chunks_exact(3).count(), 0);
}

#[test]
fn chunks_mut_empty_slice() {
    let mut v = ParticleVec::new();
    let mut slice_mut = v.as_mut_slice();
    assert_eq!(slice_mut.chunks_mut(3).count(), 0);
}
