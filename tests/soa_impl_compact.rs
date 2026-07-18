//! Verifies `#[soa_impl]` works with compact `bool` and compact enum fields:
//! read methods (`get`), write methods (`set`), and mixed regular+compact
//! access, called on the owned struct, `Ref`, and `RefMut`.

// `#[soa_impl]` clones every method onto the owned struct, `Ref` and
// `RefMut`; the tests exercise each method on some but not all three
// receivers, so the unexercised copies trip dead_code.
#![allow(dead_code)]

use layout::{soa_impl, Compact, CompactRepr, SOA};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, CompactRepr)]
enum Faction {
    Red,
    Blue,
    Green,
}

#[derive(SOA)]
struct Agent {
    id: u32,
    active: Compact<bool>,
    faction: Compact<Faction>,
    hp: i32,
}

#[soa_impl]
impl Agent {
    fn new(id: u32, faction: Faction) -> Self {
        Agent {
            id,
            active: Compact(true),
            faction: Compact(faction),
            hp: 100,
        }
    }

    // &self: read compact bool, compact enum, regular field, and combinations.
    fn is_active(&self) -> bool {
        self.active.get()
    }
    fn get_faction(&self) -> Faction {
        self.faction.get()
    }
    fn get_id(&self) -> u32 {
        self.id
    }
    fn is_enemy_of_red(&self) -> bool {
        self.faction.get() != Faction::Red && self.active.get()
    }
    fn status_code(&self) -> u32 {
        self.id + self.active.get() as u32 + self.faction.get() as u32
    }

    // &mut self: write compact bool, compact enum, regular field, and mixes.
    fn deactivate(&mut self) {
        self.active.set(false);
    }
    fn switch_faction(&mut self, f: Faction) {
        self.faction.set(f);
    }
    fn damage(&mut self, amount: i32) {
        self.hp -= amount;
        if self.hp <= 0 {
            self.active.set(false);
        }
    }
    fn resurrect(&mut self) {
        self.hp = 100;
        self.active.set(true);
        self.faction.set(Faction::Red);
    }
}

#[test]
fn soa_impl_compact_on_owned() {
    let mut a = Agent::new(7, Faction::Blue);
    assert!(a.is_active());
    assert_eq!(a.get_faction(), Faction::Blue);
    assert_eq!(a.get_id(), 7);
    assert!(a.is_enemy_of_red());
    assert_eq!(a.status_code(), 9); // 7 + active(1) + Blue(1)

    a.switch_faction(Faction::Red);
    assert!(!a.is_enemy_of_red());
    a.deactivate();
    assert!(!a.is_active());

    a.resurrect();
    assert!(a.is_active());
    assert_eq!(a.get_faction(), Faction::Red);
    assert_eq!(a.hp, 100);

    a.damage(150);
    assert!(!a.is_active());
    assert_eq!(a.hp, -50);
}

#[test]
fn soa_impl_compact_on_ref_refmut() {
    let mut vec = AgentVec::new();
    vec.push(Agent::new(1, Faction::Red));
    vec.push(Agent::new(2, Faction::Blue));
    vec.push(Agent::new(3, Faction::Green));

    // &self methods via Ref (get / iter)
    assert!(vec.get(0).unwrap().is_active());
    assert_eq!(vec.get(1).unwrap().get_faction(), Faction::Blue);
    assert_eq!(vec.get(2).unwrap().get_id(), 3);
    assert!(vec.get(1).unwrap().is_enemy_of_red());

    let codes: Vec<u32> = vec.iter().map(|a| a.status_code()).collect();
    assert_eq!(codes.len(), 3);
    // 1 + active(1) + Red(0) = 2; 2 + 1 + Blue(1) = 4; 3 + 1 + Green(2) = 6
    assert_eq!(codes, vec![2, 4, 6]);

    // &mut self methods via RefMut (get_mut)
    vec.get_mut(0).unwrap().switch_faction(Faction::Green);
    assert_eq!(vec.get(0).unwrap().get_faction(), Faction::Green);

    vec.get_mut(1).unwrap().deactivate();
    assert!(!vec.get(1).unwrap().is_active());

    vec.get_mut(2).unwrap().damage(200);
    assert!(!vec.get(2).unwrap().is_active());
    assert_eq!(vec.hp[2], -100); // regular column read directly

    vec.get_mut(2).unwrap().resurrect();
    assert!(vec.get(2).unwrap().is_active());
    assert_eq!(vec.get(2).unwrap().get_faction(), Faction::Red);

    // iter_mut applying a soa_impl write method
    for mut a in vec.iter_mut() {
        a.damage(1);
    }
    assert_eq!(vec.hp[0], 99);
    assert_eq!(vec.hp[1], 99);
    assert_eq!(vec.hp[2], 99);
}
