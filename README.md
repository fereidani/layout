<div align="center">

# Layout

**Struct-of-Arrays** and **data-oriented design** in Rust

<img src="logo.svg" alt="Layout logo" width="250" >
<br><br>

[![Crates.io](https://img.shields.io/crates/v/layout.svg?style=for-the-badge)](https://crates.io/crates/layout)
[![License](https://img.shields.io/crates/l/layout.svg?style=for-the-badge)](https://github.com/fereidani/layout)
[![CI](https://img.shields.io/github/actions/workflow/status/fereidani/layout/tests.yml?branch=main&style=for-the-badge)](https://github.com/fereidani/layout/actions)
[![Docs](https://img.shields.io/docsrs/layout?style=for-the-badge)](https://docs.rs/layout)

</div>

## Introduction

Layout turns a plain struct into a struct of arrays with one derive. Instead of
storing whole structs back to back in a `Vec<T>`, it stores each field in its own
contiguous array, so a pass over one field loads only that field's memory.

This crate is a hard fork of [soa-derive](https://github.com/lumol-org/soa-derive)
with `no_std` support and extra features like impl block and compact bool and enums.

## Example

One struct shows everything the crate offers: the derive, extra derives for the
generated types, a nested struct of arrays, bit-packed columns, and methods
that also run on borrowed rows.

```rust
use layout::{soa_impl, soa_zip, Compact, CompactRepr, SOA};

// A fieldless enum with an unsigned repr can be bit-packed:
// four variants fit in 2 bits.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, CompactRepr)]
enum Kind {
    Player,
    Enemy,
    Projectile,
    Pickup,
}

#[derive(Debug, Clone, PartialEq, SOA)]
#[layout(Debug, Clone, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, PartialEq, SOA)]
#[layout(Debug, Clone, PartialEq)] // derives for EntityVec, EntityRef, ...
struct Entity {
    name: String,
    mass: f32,
    #[nested_soa]
    position: Position, // stored as a PositionVec, not a Vec<Position>
    active: Compact<bool>, // 1 bit per entity instead of 1 byte
    kind: Compact<Kind>, // 2 bits per entity
}

// Copies the methods onto EntityRef (&self) and EntityRefMut (&mut self).
#[soa_impl]
impl Entity {
    fn kinetic_energy(&self, velocity: f32) -> f32 {
        0.5 * self.mass * velocity * velocity
    }

    fn scale_mass(&mut self, factor: f32) {
        self.mass *= factor;
    }
}

fn main() {
    let mut entities = EntityVec::new();
    entities.push(Entity {
        name: "hero".into(),
        mass: 80.0,
        position: Position { x: 0.0, y: 0.0 },
        active: Compact(true),
        kind: Compact(Kind::Player),
    });
    entities.push(Entity {
        name: "slime".into(),
        mass: 12.0,
        position: Position { x: 4.0, y: 1.0 },
        active: Compact(false),
        kind: Compact(Kind::Enemy),
    });

    // Whole rows: each item is an EntityRef, with the #[soa_impl] methods.
    for entity in entities.iter() {
        println!("{}: {} J", entity.name, entity.kinetic_energy(2.0));
    }

    // One column: only the name array is loaded.
    for name in &entities.name {
        println!("{name}");
    }

    // Nested fields are columns as well.
    let total_x: f32 = entities.position.x.iter().sum();

    // Several columns at once; `mut` yields mutable references.
    for (mass, active) in soa_zip!(&mut entities, [mut mass, active]) {
        if active.get() {
            *mass += 1.0;
        }
    }

    // Row access by index, like vec[i] and &mut vec[i].
    entities.index_mut(0).scale_mass(2.0);
    if entities.index(1).kind.get() == Kind::Enemy {
        entities.index_mut(1).active.set(true);
    }

    // Packed columns scan whole words at a time.
    let active = entities.active.count(true);
    let enemies = entities.kind.count(Kind::Enemy);
    println!("{active} active, {enemies} enemies, total x {total_x}");
}
```

`iter()` costs about the same as reading the fields by hand, because LLVM drops
the loads for fields you never read in release builds. Borrowing one column is
the struct-of-arrays payoff: only that array is touched. The
[soa_zip!](https://docs.rs/layout/*/layout/macro.soa_zip.html) macro walks
several columns together and can zip external iterators as well.

### Generated types

`#[derive(SOA)]` on `Entity` generates `EntityVec`, which has the same API as
`Vec<Entity>` but stores one array per field:

```rust
struct EntityVec {
    name: Column<String>,     // derefs to [String]
    mass: Column<f32>,
    position: PositionVec,    // #[nested_soa]: a struct of arrays itself
    active: CompactVec<bool>, // bit-packed
    kind: CompactVec<Kind>,
}
```

The helper types mirror how you borrow an `Entity`:

| Helper           | Stands in for   |
| ---------------- | --------------- |
| `EntitySlice`    | `&[Entity]`     |
| `EntitySliceMut` | `&mut [Entity]` |
| `EntityRef`      | `&Entity`       |
| `EntityRefMut`   | `&mut Entity`   |

Every derived struct implements the `SOA` trait, so `<Entity as SOA>::Type`
names `EntityVec` in generic code.

`#[layout(...)]` passes derives through to all generated types. To attach an
attribute to a single one, use `#[soa_attr(Target, ...)]`, for example
`#[soa_attr(Vec, cfg_attr(test, derive(PartialEq)))]`. `Target` is one of
`Vec`, `Slice`, `SliceMut`, `Ref`, `RefMut`, `Ptr` or `PtrMut`.

### Methods on rows (`#[soa_impl]`)

`#[soa_impl]` copies an `impl` block onto the generated reference types:
`&self` methods land on `EntityRef`, `&mut self` methods on `EntityRefMut`,
and associated functions or `Self`-returning methods stay on `Entity` only.
The reference types hold `&T` rather than `T`, so the macro inserts
dereferences where a method reads or writes a field by value:

| Source                | Generated                      |
| --------------------- | ------------------------------ |
| `self.mass * 2.0`     | `(*self.mass) * 2.0`           |
| `self.mass *= factor` | `*self.mass *= factor`         |
| `self.mass = val`     | `*self.mass = val`             |
| `self.name.len()`     | `self.name.len()` (auto-deref) |
| `-self.x`             | `-(*self.x)`                   |
| `self.x as i32`       | `(*self.x) as i32`             |

### Compact columns (`Compact<T>`)

A `bool` column costs a byte per row and a small enum four or eight.
`Compact<T>` shrinks such columns to the minimum width: `bool` and one-bit
enums take one bit per row, larger fieldless enums two or four. A fieldless
enum opts in with `#[derive(CompactRepr)]` and an unsigned `#[repr(uN)]`. The
derive rejects variants that carry data, sizes storage from the largest
discriminant, and refuses enums that need more than four bits, since at eight
bits a packed column is no smaller than a plain one.

Read and write a packed field through `get` and `set`. A `CompactVec` also
offers `count`, which encodes the value once and scans the packed words. For
one-bit types it lowers to `count_ones` / `count_zeros`, which LLVM turns into
`POPCNT`: counting the active flag over 100k entities takes ~1.6 us versus
~4.9 us for `Vec<bool>::iter().filter().count()`, and the column drops from
~97 KiB to ~12 KiB.

Reach for `Compact<T>` when many rows carry a narrow flag or tag: entity active
bits, tile or voxel types, collision layers, visibility masks. A packed column
that fits in L1 lets a later pass run faster. The cost shows up in a tight loop
that reads or writes the bit every iteration alongside other fields, because
extracting one bit costs more than loading one byte. If a flag sits on your hot
path, measure it with `cargo bench --bench game`.

> **Keep the import names.** `#[derive(SOA)]` recognizes a compact column by
> the path-segment name `Compact` / `CompactBool`, because derive macros see
> tokens, not resolved types. A renamed import such as
> `use layout::Compact as Packed;` with a field `Packed<bool>` is not
> recognized and silently falls back to a plain column of `Packed<bool>`, a
> full byte per element with no error or warning. Use the names `Compact` /
> `CompactBool` directly, or a fully-qualified path such as
> `::layout::Compact<bool>`.

### Serialization (`serde`)

Enable the `serde` cargo feature and pass `Serialize, Deserialize` through
`#[layout(...)]` to (de)serialize the generated `Vec` as a struct of arrays.
Compact columns round-trip as their decoded values, and the feature works with
`no_std` + `alloc`.

```toml
[dependencies]
layout = { version = "0.2", features = ["serde"] }
serde  = { version = "1", features = ["derive"] }
```

With `#[layout(Debug, Clone, PartialEq, Serialize, Deserialize)]` on the
structs above (and `Serialize, Deserialize` derived on `Kind`), `EntityVec`
serializes column by column:

```json
{"name":["hero","slime"],"mass":[80.0,12.0],"position":{"x":[0.0,4.0],"y":[0.0,1.0]},"active":[true,false],"kind":["Player","Enemy"]}
```

## API and caveats

The generated code carries its own documentation, so `cargo doc` renders every
struct and function. In most cases you can swap `Vec<Entity>` for `EntityVec`.
The exceptions come from how `Vec` leans on references and `Deref`.

`EntityVec` cannot implement `Deref<Target = EntitySlice>`, because `Deref` must
return a reference and `EntitySlice` is not one. The same holds for `Index` and
`IndexMut`, which would have to return `EntityRef` / `EntityRefMut`, so
`entities[0]` does not compile; use `index(0)` / `index_mut(0)` or `get(0)` /
`get_mut(0)` instead. A few methods come in two forms, and some calls need
`as_slice()` or `as_mut_slice()` to reach the slice type.

## Benchmarks

The benchmarks compare two layouts:

- **AoS (Array of Structures):** a plain `Vec<T>` storing whole structs.
- **SoA (Structure of Arrays):** the layout from this crate, one array per field.

Reads run up to **3x faster** on the SoA side.

```
test aos_big_do_work_100k        ... bench:     161,151 ns/iter (+/- 57,573)
test aos_big_do_work_10k         ... bench:       6,979 ns/iter (+/- 158)
test aos_big_push                ... bench:          58 ns/iter (+/- 27)
test aos_small_do_work_100k      ... bench:      66,672 ns/iter (+/- 599)
test aos_small_push              ... bench:          16 ns/iter (+/- 7)
test soa_big_do_work_100k        ... bench:      69,611 ns/iter (+/- 2,165)
test soa_big_do_work_10k         ... bench:       6,708 ns/iter (+/- 117)
test soa_big_do_work_simple_100k ... bench:      76,656 ns/iter (+/- 1,675)
test soa_big_push                ... bench:          42 ns/iter (+/- 4)
test soa_small_do_work_100k      ... bench:      66,586 ns/iter (+/- 1,238)
test soa_small_push              ... bench:           6 ns/iter (+/- 3)
```

Each test has an AoS and an SoA variant, on a 24-byte struct and a 240-byte
struct. Run them yourself with `cargo bench`.

## License

Dual-licensed under MIT or Apache-2.0, at your option. Contributions are
welcome; open an issue first to discuss the change.

Thanks to Guillaume Fraux (@Luthaf) for [soa-derive](https://github.com/lumol-org/soa-derive), of which this crate is a hard fork.

Thanks to @maikklein for the initial idea: https://maikklein.github.io/soa-rust/
