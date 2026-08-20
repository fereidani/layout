use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{Ident, Type};

/// Build the ident `{name}{suffix}` at the call site.
fn suffixed(name: impl ToTokens, suffix: &str) -> Ident {
    Ident::new(
        &format!("{}{}", name.to_token_stream(), suffix),
        Span::call_site(),
    )
}

/// Define the `{name}{Suffix}` ident helpers for every generated type.
macro_rules! generated_names {
    ($($(#[$doc:meta])* $func:ident => $suffix:literal,)*) => {
        $(
            $(#[$doc])*
            pub fn $func(name: impl ToTokens) -> Ident {
                suffixed(name, $suffix)
            }
        )*
    };
}

generated_names! {
    /// Get the ident for the `Vec` type associated with `name`
    vec_name => "Vec",
    /// Get the ident for the slice type associated with `name`
    slice_name => "Slice",
    /// Get the ident for the mutable slice type associated with `name`
    slice_mut_name => "SliceMut",
    /// Get the ident for the reference type associated with `name`
    ref_name => "Ref",
    /// Get the ident for the mutable reference type associated with `name`
    ref_mut_name => "RefMut",
    /// Get the ident for the iterator type associated with `name`
    iter_name => "Iter",
    /// Get the ident for the mutable iterator type associated with `name`
    iter_mut_name => "IterMut",
    /// Get the ident for the pointer type associated with `name`
    ptr_name => "Ptr",
    /// Get the ident for the mutable pointer type associated with `name`
    ptr_mut_name => "PtrMut",
    /// Get the ident for the drain type associated with `name`
    drain_name => "Drain",
    /// Get the ident for the chunks iterator type associated with `name`
    chunks_name => "Chunks",
    /// Get the ident for the mutable chunks iterator type associated with
    /// `name`
    chunks_mut_name => "ChunksMut",
    /// Get the ident for the exact chunks iterator type associated with `name`
    chunks_exact_name => "ChunksExact",
    /// Get the ident for the mutable exact chunks iterator type associated
    /// with `name`
    chunks_exact_mut_name => "ChunksExactMut",
}

/// Define the "type of a nested column" helpers. A compact column
/// (`Compact<T>` / `CompactBool`) is backed by the generic `::layout::Compact*`
/// type parameterized by its inner element type; any other nested-SoA column
/// uses the type generated for the field's own struct.
macro_rules! nested_types {
    ($($(#[$doc:meta])* $func:ident => ($compact:ident, $plain:ident $(, $lt:lifetime)?),)*) => {
        $(
            $(#[$doc])*
            pub fn $func(
                field_type: &Type,
                compact: Option<&Type>,
            ) -> TokenStream {
                match compact {
                    Some(inner) => {
                        quote! { ::layout::$compact<$($lt,)? #inner> }
                    }
                    None => {
                        let id = $plain(field_type);
                        quote! { #id $(<$lt>)? }
                    }
                }
            }
        )*
    };
}

nested_types! {
    /// Owning column type of a nested field.
    nested_vec_ty => (CompactVec, vec_name),
    /// Immutable slice type of a nested field.
    nested_slice_ty => (CompactSlice, slice_name, 'a),
    /// Mutable slice type of a nested field.
    nested_slice_mut_ty => (CompactSliceMut, slice_mut_name, 'a),
    /// Mutable reference type of a nested field.
    nested_ref_mut_ty => (CompactRefMut, ref_mut_name, 'a),
    /// Immutable pointer type of a nested field.
    nested_ptr_ty => (CompactPtr, ptr_name),
    /// Mutable pointer type of a nested field.
    nested_ptr_mut_ty => (CompactPtrMut, ptr_mut_name),
    /// Drain iterator type of a nested field.
    nested_drain_ty => (CompactDrain, drain_name, 'a),
}
