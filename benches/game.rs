#![allow(clippy::needless_return)]

//! Game-entity benchmark: `bool` (1 byte/entity) vs `Compact<bool>` (1
//! bit/entity).
//!
//! Both structs are identical except the `active` column storage, so any speed
//! difference isolates the bit-packed vs byte-backed boolean column.

use bencher::{benchmark_group, benchmark_main, Bencher};
use layout::{Compact, SOA};

/// Number of entities — large enough that the `active` column spans many cache
/// lines (100k bools = ~97 KiB vs 100k bits = ~12 KiB).
const N: usize = 100_000;

#[derive(SOA)]
pub struct Entity {
    id: u32,
    x: f32,
    y: f32,
    z: f32,
    vx: f32,
    vy: f32,
    vz: f32,
    health: f32,
    active: bool, // 1 byte per entity (Vec<bool>)
    kind: u8,
}

#[derive(SOA)]
pub struct EntityCompact {
    id: u32,
    x: f32,
    y: f32,
    z: f32,
    vx: f32,
    vy: f32,
    vz: f32,
    health: f32,
    active: Compact<bool>, // 1 bit per entity (bit-packed)
    kind: u8,
}

/// ~2/3 active, a mixed (non-trivial, non-uniform) bit pattern.
fn active_for(i: usize) -> bool {
    i % 3 != 0
}

fn build_regular() -> EntityVec {
    let mut v = EntityVec::new();
    for i in 0..N {
        v.push(Entity {
            id: i as u32,
            x: 1.0,
            y: 2.0,
            z: 3.0,
            vx: 0.1,
            vy: 0.0,
            vz: 0.2,
            health: 100.0,
            active: active_for(i),
            kind: (i % 4) as u8,
        });
    }
    v
}

fn build_compact() -> EntityCompactVec {
    let mut v = EntityCompactVec::new();
    for i in 0..N {
        v.push(EntityCompact {
            id: i as u32,
            x: 1.0,
            y: 2.0,
            z: 3.0,
            vx: 0.1,
            vy: 0.0,
            vz: 0.2,
            health: 100.0,
            active: Compact(active_for(i)),
            kind: (i % 4) as u8,
        });
    }
    v
}

// --- build (fill) ---

fn build_regular_bench(b: &mut Bencher) {
    b.iter(build_regular);
}

fn build_compact_bench(b: &mut Bencher) {
    b.iter(build_compact);
}

// --- read the `active` column directly (purest byte-vs-bit comparison) ---

fn read_active_regular(b: &mut Bencher) {
    let v = build_regular();
    b.iter(|| {
        let mut count = 0u32;
        for &a in v.active.iter() {
            if a {
                count += 1;
            }
        }
        count
    });
}

fn read_active_compact(b: &mut Bencher) {
    let v = build_compact();
    b.iter(|| {
        let mut count = 0u32;
        for a in v.active.as_slice().iter() {
            if a.get() {
                count += 1;
            }
        }
        count
    });
}

// --- read `active` via full-row iteration ---

fn iter_active_regular(b: &mut Bencher) {
    let v = build_regular();
    b.iter(|| {
        let mut count = 0u32;
        for r in v.iter() {
            if *r.active {
                count += 1;
            }
        }
        count
    });
}

fn iter_active_compact(b: &mut Bencher) {
    let v = build_compact();
    b.iter(|| {
        let mut count = 0u32;
        for r in v.iter() {
            if r.active.get() {
                count += 1;
            }
        }
        count
    });
}

// --- write `active` (toggle every entity) ---

fn write_active_regular(b: &mut Bencher) {
    let mut v = build_regular();
    b.iter(|| {
        let mut count = 0u32;
        for r in v.iter_mut() {
            *r.active = !*r.active;
            if *r.active {
                count += 1;
            }
        }
        count
    });
}

fn write_active_compact(b: &mut Bencher) {
    let mut v = build_compact();
    b.iter(|| {
        let mut count = 0u32;
        for mut r in v.iter_mut() {
            r.active.set(!r.active.get());
            if r.active.get() {
                count += 1;
            }
        }
        count
    });
}

// --- count entities where `active == true` (bulk count API) ---

fn count_active_regular(b: &mut Bencher) {
    let v = build_regular();
    b.iter(|| v.active.iter().filter(|b| **b).count() as u32);
}

fn count_active_compact(b: &mut Bencher) {
    let v = build_compact();
    b.iter(|| v.active.count(true) as u32);
}

// --- realistic game tick: advance active entities, decay+regen health, update
// flag --- Health oscillates in [0, 200) so the workload is stable across
// iterations (no exhaustion), and `active` is recomputed and rewritten every
// tick.

fn game_tick_regular(b: &mut Bencher) {
    let mut v = build_regular();
    b.iter(|| {
        let mut active_count = 0u32;
        for r in v.iter_mut() {
            *r.x += *r.vx;
            *r.y += *r.vy;
            *r.z += *r.vz;
            *r.health = (*r.health + 1.0) % 200.0;
            let alive = *r.health > 50.0;
            *r.active = alive;
            if alive {
                active_count += 1;
            }
        }
        active_count
    });
}

fn game_tick_compact(b: &mut Bencher) {
    let mut v = build_compact();
    b.iter(|| {
        let mut active_count = 0u32;
        for mut r in v.iter_mut() {
            *r.x += *r.vx;
            *r.y += *r.vy;
            *r.z += *r.vz;
            *r.health = (*r.health + 1.0) % 200.0;
            let alive = *r.health > 50.0;
            r.active.set(alive);
            if alive {
                active_count += 1;
            }
        }
        active_count
    });
}

benchmark_group!(
    game,
    build_regular_bench,
    build_compact_bench,
    read_active_regular,
    read_active_compact,
    iter_active_regular,
    iter_active_compact,
    write_active_regular,
    write_active_compact,
    count_active_regular,
    count_active_compact,
    game_tick_regular,
    game_tick_compact,
);
benchmark_main!(game);
