#![allow(clippy::needless_return)]

use bencher::{benchmark_group, benchmark_main, Bencher};
use layout::SOA;

#[derive(SOA)]
pub struct Small {
    x: f64,
    y: f64,
    z: f64,
}

impl Small {
    fn new() -> Small {
        Small {
            x: 1.0,
            y: 0.2,
            z: -2.3,
        }
    }

    fn aos_vec(size: usize) -> Vec<Small> {
        let mut vec = Vec::new();
        for _ in 0..size {
            vec.push(Small::new())
        }
        return vec;
    }

    fn soa_vec(size: usize) -> SmallVec {
        let mut vec = SmallVec::new();
        for _ in 0..size {
            vec.push(Small::new())
        }
        return vec;
    }
}

#[derive(SOA)]
pub struct Big {
    position: (f64, f64, f64),
    velocity: (f64, f64, f64),
    data: [usize; 18],
    name: String,
    userdata: String,
}

impl Big {
    fn new() -> Big {
        Big {
            position: (1.0, 0.2, -2.3),
            velocity: (1.0, 0.2, -2.3),
            data: [67; 18],
            name: "foo".into(),
            userdata: "bar".into(),
        }
    }

    fn aos_vec(size: usize) -> Vec<Big> {
        let mut vec = Vec::new();
        for _ in 0..size {
            vec.push(Big::new())
        }
        return vec;
    }

    fn soa_vec(size: usize) -> BigVec {
        let mut vec = BigVec::new();
        for _ in 0..size {
            vec.push(Big::new())
        }
        return vec;
    }
}

fn aos_small_push(bencher: &mut Bencher) {
    let mut vec = Vec::new();
    bencher.iter(|| vec.push(Small::new()))
}

fn soa_small_push(bencher: &mut Bencher) {
    let mut vec = SmallVec::new();
    bencher.iter(|| vec.push(Small::new()))
}

fn aos_big_push(bencher: &mut Bencher) {
    let mut vec = Vec::new();
    bencher.iter(|| vec.push(Big::new()))
}

fn soa_big_push(bencher: &mut Bencher) {
    let mut vec = BigVec::new();
    bencher.iter(|| vec.push(Big::new()))
}

fn aos_small_do_work_100k(bencher: &mut Bencher) {
    let vec = Small::aos_vec(100_000);
    bencher.iter(|| {
        let mut s = 0.0;
        for v in &vec {
            s += v.x + v.y;
        }
        s
    })
}

fn soa_small_do_work_100k(bencher: &mut Bencher) {
    let vec = Small::soa_vec(100_000);
    bencher.iter(|| {
        let mut s = 0.0;
        for (x, y) in vec.x.iter().zip(&vec.y) {
            s += x + y;
        }
        s
    })
}

fn aos_big_do_work_10k(bencher: &mut Bencher) {
    let vec = Big::aos_vec(10_000);
    bencher.iter(|| {
        let mut s = 0.0;
        for v in &vec {
            s += v.position.0 + v.velocity.0 * 0.1;
        }
        s
    })
}

fn aos_big_do_work_100k(bencher: &mut Bencher) {
    let vec = Big::aos_vec(100_000);
    bencher.iter(|| {
        let mut s = 0.0;
        for v in &vec {
            s += v.position.0 + v.velocity.0 * 0.1;
        }
        s
    })
}

fn soa_big_do_work_10k(bencher: &mut Bencher) {
    let vec = Big::soa_vec(10_000);
    bencher.iter(|| {
        let mut s = 0.0;
        for (position, velocity) in vec.position.iter().zip(&vec.velocity) {
            s += position.0 + velocity.0;
        }
        s
    })
}

fn soa_big_do_work_100k(bencher: &mut Bencher) {
    let vec = Big::soa_vec(100_000);
    bencher.iter(|| {
        let mut s = 0.0;
        for (position, velocity) in vec.position.iter().zip(&vec.velocity) {
            s += position.0 + velocity.0;
        }
        s
    })
}

fn soa_big_do_work_simple_100k(bencher: &mut Bencher) {
    let vec = Big::soa_vec(100_000);
    bencher.iter(|| {
        let mut s = 0.0;
        for elem in vec.iter() {
            s += elem.position.0 + elem.velocity.0;
        }
        s
    })
}

#[derive(PartialEq, Clone, Copy, Debug, Default)]
#[repr(u8)]
pub enum CreatureType {
    #[default]
    Orc,
    Elf,
    Human,
    Goblin,
}

#[derive(SOA, Default)]
pub struct Creature {
    health: u8,
    ctype: CreatureType,
    position: (f64, f64, f64),
    velocity: (f64, f64, f64),
    is_alive: bool,
    mana: u16,
    stamina: u16,
    strength: u16,
    agility: u16,
    intelligence: u16,
    wisdom: u16,
    charisma: u16,
    luck: u16,
    armor: u16,
    resistance: u16,
    attack_speed: f32,
    move_speed: f32,
    experience: u64,
    level: u16,
    gold: u32,
    inventory: [u32; 8],
    player_id: Option<u32>,
    faction: u8,
    target_id: Option<u32>,
    status_effects: [u8; 4],
}

impl Creature {
    fn new_random(seed: &mut u64) -> Creature {
        // Simple linear congruential generator for no_std environments
        fn next_u64(seed: &mut u64) -> u64 {
            // Constants from Numerical Recipes
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            *seed
        }

        fn gen_range<T>(seed: &mut u64, min: T, max: T) -> T
        where
            T: Copy
                + std::ops::Add<Output = T>
                + std::ops::Sub<Output = T>
                + std::ops::AddAssign
                + TryFrom<u64>
                + Into<u64>,
        {
            let range = (max - min) + T::try_from(1u64).ok().unwrap();
            let rand = next_u64(seed) % range.into();
            min + T::try_from(rand).ok().unwrap()
        }

        fn gen_bool(seed: &mut u64) -> bool {
            (next_u64(seed) % 2) == 0
        }

        fn gen_f64(seed: &mut u64, min: f64, max: f64) -> f64 {
            let val = (next_u64(seed) as f64) / (u64::MAX as f64);
            min + (max - min) * val
        }

        Creature {
            health: gen_range::<u8>(seed, 0, 100),
            ctype: {
                match gen_range(seed, 0u8, 3u8) {
                    0 => CreatureType::Orc,
                    1 => CreatureType::Elf,
                    2 => CreatureType::Human,
                    _ => CreatureType::Goblin,
                }
            },
            position: (
                gen_f64(seed, -1000.0, 1000.0),
                gen_f64(seed, -1000.0, 1000.0),
                gen_f64(seed, -1000.0, 1000.0),
            ),
            is_alive: gen_bool(seed),
            ..Default::default()
        }
    }

    fn aos_vec(size: usize) -> Vec<Creature> {
        let mut seed = 666u64;
        (0..size).map(|_| Creature::new_random(&mut seed)).collect()
    }

    fn soa_vec(size: usize) -> CreatureVec {
        let mut seed = 666u64;
        let mut vec = CreatureVec::new();
        for _ in 0..size {
            vec.push(Creature::new_random(&mut seed));
        }
        vec
    }
}

fn aos_creature_count_alive_1m(bencher: &mut Bencher) {
    let vec = core::hint::black_box(Creature::aos_vec(1_000_000));
    bencher.iter(|| {
        vec.iter()
            .filter(|c| c.is_alive && c.health > 50 && c.ctype == CreatureType::Goblin)
            .count()
    })
}

fn soa_creature_count_alive_1m(bencher: &mut Bencher) {
    let vec = core::hint::black_box(Creature::soa_vec(1_000_000));
    bencher.iter(|| {
        vec.iter()
            .filter(|c| *c.is_alive && *c.health > 50 && *c.ctype == CreatureType::Goblin)
            .count()
    })
}

// DO NOT USE THIS IN STYLE YOUR PROGRAMS, IT IS HERE TO BENCHMARK UNSAFE ACCESS VS SAFE ACCESS
//  TO UNDERSTAND BOUND CHECKING AND ABSTRACTION OVERHEAD
fn soa_creature_count_alive_1m_unsafe(bencher: &mut Bencher) {
    let vec = core::hint::black_box(Creature::soa_vec(1_000_000));
    bencher.iter(|| {
        let len = vec.len();
        let is_alive = vec.is_alive.as_ptr();
        let health_ptr = vec.health.as_ptr();
        let ctype_ptr = vec.ctype.as_ptr();
        let mut count = 0;
        for i in 0..len {
            let alive = unsafe { *is_alive.add(i) };
            let health = unsafe { *health_ptr.add(i) };
            let ctype = unsafe { &*ctype_ptr.add(i) };
            if alive && health > 50 && *ctype == CreatureType::Goblin {
                count += 1;
            }
        }
        count
    })
}

benchmark_group!(
    aos,
    aos_small_push,
    aos_big_push,
    aos_small_do_work_100k,
    aos_big_do_work_10k,
    aos_big_do_work_100k,
    aos_creature_count_alive_1m,
);
benchmark_group!(
    soa,
    soa_small_push,
    soa_big_push,
    soa_small_do_work_100k,
    soa_big_do_work_10k,
    soa_big_do_work_100k,
    soa_big_do_work_simple_100k,
    soa_creature_count_alive_1m,
    soa_creature_count_alive_1m_unsafe,
);

benchmark_main!(soa, aos);
