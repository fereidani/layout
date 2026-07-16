#![cfg(feature = "serde")]
use layout::{Compact, CompactBool, CompactRepr, SOA};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, SOA)]
#[layout(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Particle {
    pub name: String,
    pub mass: f64,
}

impl Particle {
    pub fn new(name: String, mass: f64) -> Self {
        Particle { name, mass }
    }
}

#[test]
fn serde_test() -> Result<(), serde_json::Error> {
    let mut soa = ParticleVec::new();
    soa.push(Particle::new(String::from("Na"), 56.0));
    soa.push(Particle::new(String::from("Cl"), 35.0));

    let json = serde_json::to_string(&soa)?;
    assert_eq!(json, r#"{"name":["Na","Cl"],"mass":[56.0,35.0]}"#);
    let soa2: ParticleVec = serde_json::from_str(&json)?;
    assert_eq!(soa, soa2);
    Ok(())
}

// A fieldless enum opts into compact storage via CompactRepr; it must also be
// (de)serializable so Compact<Kind> and CompactVec<Kind> can round-trip.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, CompactRepr, Serialize, Deserialize)]
pub enum Kind {
    Player,
    Enemy,
    Npc,
}

#[derive(Debug, Clone, PartialEq, SOA)]
#[layout(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: u32,
    pub active: CompactBool,
    pub kind: Compact<Kind>,
}

#[test]
fn serde_compact_test() -> Result<(), serde_json::Error> {
    let mut soa = EntityVec::new();
    soa.push(Entity {
        id: 1,
        active: Compact(true),
        kind: Compact(Kind::Player),
    });
    soa.push(Entity {
        id: 2,
        active: Compact(false),
        kind: Compact(Kind::Enemy),
    });

    let json = serde_json::to_string(&soa)?;
    assert_eq!(
        json,
        r#"{"id":[1,2],"active":[true,false],"kind":["Player","Enemy"]}"#
    );
    let soa2: EntityVec = serde_json::from_str(&json)?;
    assert_eq!(soa, soa2);
    Ok(())
}
