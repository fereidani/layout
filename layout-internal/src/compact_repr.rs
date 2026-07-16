//! Derive macro for `CompactRepr`.
//!
//! `#[derive(CompactRepr)]` opts a **fieldless** enum into compact
//! (bit-packed) struct-of-arrays storage. It:
//!
//! * errors unless the enum carries an unsigned integer `#[repr(uN)]`,
//! * errors unless every variant is a unit (fieldless) variant,
//! * computes the storage width from the largest discriminant, and
//! * emits an `impl layout::CompactRepr` whose `decode` round-trips the real
//!   discriminant value via a safe exhaustive `match`.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse::Parser, punctuated::Punctuated, spanned::Spanned, Attribute, Data,
    DeriveInput, Expr, ExprLit, Fields, Lit, Meta, Path, Token,
};

/// Entry point invoked by the `#[derive(CompactRepr)]` proc-macro.
pub fn derive_compact_repr(input: &DeriveInput) -> TokenStream {
    let name = &input.ident;

    let Data::Enum(data_enum) = &input.data else {
        return err_span(
            input.ident.span(),
            "#[derive(CompactRepr)] only supports enums".to_string(),
        );
    };

    let Some(_repr_int) = find_unsigned_repr(&input.attrs) else {
        return err_span(
            name.span(),
            "#[derive(CompactRepr)] requires an unsigned integer repr \
             (one of #[repr(u8)], #[repr(u16)], #[repr(u32)], #[repr(u64)] or \
             #[repr(usize)])"
                .to_string(),
        );
    };

    // Walk the variants: collect each `(variant_ident, discriminant)` pair in
    // declaration order, rejecting any variant that carries data.
    let mut pairs: Vec<(Ident, u128)> = Vec::new();
    let mut next: Option<u128> = Some(0);
    for variant in &data_enum.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return err_span(
                variant.span(),
                format!(
                    "#[derive(CompactRepr)] only supports fieldless (unit) \
                     variants; variant `{}` carries data",
                    variant.ident
                ),
            );
        }
        let disc = match &variant.discriminant {
            Some((_, expr)) => match eval_discriminant(expr) {
                Ok(v) => v,
                Err(msg) => return err_span(variant.span(), msg),
            },
            None => match next {
                Some(n) => n,
                None => {
                    return err_span(
                        name.span(),
                        format!("enum `{name}` has too many variants to assign discriminants"),
                    )
                }
            },
        };
        next = disc.checked_add(1);
        pairs.push((variant.ident.clone(), disc));
    }

    let discriminants: Vec<u128> = pairs.iter().map(|(_, d)| *d).collect();

    if discriminants.is_empty() {
        return err_span(
            name.span(),
            "#[derive(CompactRepr)] does not support empty enums".to_string(),
        );
    }

    let max = *discriminants.iter().max().expect("non-empty");
    let bits: u32 = match max {
        0 | 1 => 1,
        2..=3 => 2,
        4..=15 => 4,
        _ => {
            // Above 4 bits (16 values) `Compact` storage is byte-for-byte the
            // same size as a plain `Vec<Enum>` with
            // `#[repr(u8)]`/`#[repr(u16)]`, but it adds
            // encode/decode + bit-op overhead on every access — the
            // compaction buys nothing. Reject it and point the user at a plain
            // field instead.
            return err_span(
                name.span(),
                format!(
                    "#[derive(CompactRepr)] on `{name}`: the largest discriminant is \
                     {max}, which needs more than 4 bits. At 8 bits and above \
                     `Compact<{name}>` is the same size as a plain `Vec<{name}>` \
                     (use `#[repr(u8)]` or `#[repr(u16)]`) but adds encode/decode \
                     overhead, so compacting it is redundant. Drop `Compact` and \
                     store the field as a plain `{name}` (e.g. `flag: {name}` \
                     instead of `flag: Compact<{name}>`)."
                ),
            );
        }
    };

    let valid_values: Vec<usize> =
        discriminants.iter().map(|d| *d as usize).collect();
    let idents: Vec<Ident> = pairs.iter().map(|(id, _)| id.clone()).collect();
    // Emit each discriminant as an unsuffixed integer literal so it infers to
    // `usize` as a match-pattern on `raw: usize` (a suffixed `u128` literal
    // would not type-check against a `usize` scrutinee).
    let discs: Vec<proc_macro2::Literal> = discriminants
        .iter()
        .map(|d| proc_macro2::Literal::usize_unsuffixed(*d as usize))
        .collect();
    let storage_ty = quote! { ::layout::bitpack::PackedArray<#bits> };

    quote! {
        impl ::layout::CompactRepr for #name {
            type Storage = #storage_ty;

            const BITS: u32 = #bits;

            #[inline]
            fn encode(self) -> usize {
                self as usize
            }

            #[inline]
            fn decode(raw: usize) -> Self {
                debug_assert!(
                    [#( #valid_values ),*].contains(&raw),
                    "invalid compact discriminant for {}",
                    stringify!(#name)
                );
                match raw {
                    #( #discs => Self::#idents, )*
                    #[allow(unreachable_patterns)]
                    _ => unreachable!(
                        "invalid compact discriminant for {}: {}",
                        stringify!(#name),
                        raw
                    ),
                }
            }
        }
    }
}

/// Resolve an unsigned integer `#[repr(uN)]` from the attribute list, if any.
fn find_unsigned_repr(attrs: &[Attribute]) -> Option<Ident> {
    for attr in attrs {
        if !attr.path().is_ident("repr") {
            continue;
        }
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        let parser = Punctuated::<Path, Token![,]>::parse_terminated;
        let Ok(paths) = parser.parse2(list.tokens.clone()) else {
            continue;
        };
        for path in paths {
            if let Some(ident) = path.get_ident() {
                if matches!(
                    ident.to_string().as_str(),
                    "u8" | "u16" | "u32" | "u64" | "usize"
                ) {
                    return Some(Ident::new(&ident.to_string(), ident.span()));
                }
            }
        }
    }
    None
}

/// Evaluate a discriminant expression to a non-negative integer value.
///
/// Only plain integer literals are accepted (any base). Negative values and
/// arbitrary const expressions are rejected with a clear message.
fn eval_discriminant(expr: &Expr) -> Result<u128, String> {
    if let Expr::Lit(ExprLit {
        lit: Lit::Int(li), ..
    }) = expr
    {
        li.base10_parse::<u128>()
            .map_err(|e| format!("#[derive(CompactRepr)]: could not parse discriminant literal: {e}"))
    } else {
        Err(
            "#[derive(CompactRepr)] requires non-negative integer literal \
             discriminants (custom const expressions are not supported)"
                .to_string(),
        )
    }
}

/// We need a (re-exported) `proc_macro2::Ident` for the repr integer type.
use proc_macro2::Ident;

fn err_span(span: Span, msg: String) -> TokenStream {
    syn::Error::new(span, msg).to_compile_error()
}
