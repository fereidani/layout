use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::Ident;

/// Get the ident for the `Vec` type associated with `name`
pub fn vec_name(name: impl ToTokens) -> Ident {
    Ident::new(&format!("{}Vec", name.to_token_stream()), Span::call_site())
}

/// Get the ident for the slice type associated with `name`
pub fn slice_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}Slice", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the mutable slice type associated with `name`
pub fn slice_mut_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}SliceMut", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the reference type associated with `name`
pub fn ref_name(name: impl ToTokens) -> Ident {
    Ident::new(&format!("{}Ref", name.to_token_stream()), Span::call_site())
}

/// Get the ident for the mutable reference type associated with `name`
pub fn ref_mut_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}RefMut", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the iterator type associated with `name`
pub fn iter_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}Iter", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the mutable iterator type associated with `name`
pub fn iter_mut_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}IterMut", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the pointer type associated with `name`
pub fn ptr_name(name: impl ToTokens) -> Ident {
    Ident::new(&format!("{}Ptr", name.to_token_stream()), Span::call_site())
}

/// Get the ident for the mutable pointer type associated with `name`
pub fn ptr_mut_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}PtrMut", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the drain type associated with `name`
pub fn drain_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}Drain", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the chunks iterator type associated with `name`
pub fn chunks_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}Chunks", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the mutable chunks iterator type associated with `name`
pub fn chunks_mut_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}ChunksMut", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the exact chunks iterator type associated with `name`
pub fn chunks_exact_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}ChunksExact", name.to_token_stream()),
        Span::call_site(),
    )
}

/// Get the ident for the mutable exact chunks iterator type associated with
/// `name`
pub fn chunks_exact_mut_name(name: impl ToTokens) -> Ident {
    Ident::new(
        &format!("{}ChunksExactMut", name.to_token_stream()),
        Span::call_site(),
    )
}

pub fn vec_name_compact(inner: &syn::Type) -> TokenStream {
    quote! { ::layout::CompactVec<#inner> }
}

pub fn slice_name_compact(inner: &syn::Type) -> TokenStream {
    quote! { ::layout::CompactSlice<'a, #inner> }
}

pub fn slice_mut_name_compact(inner: &syn::Type) -> TokenStream {
    quote! { ::layout::CompactSliceMut<'a, #inner> }
}

pub fn ref_mut_name_compact(inner: &syn::Type) -> TokenStream {
    quote! { ::layout::CompactRefMut<'a, #inner> }
}

pub fn ptr_name_compact(inner: &syn::Type) -> TokenStream {
    quote! { ::layout::CompactPtr<#inner> }
}

pub fn ptr_mut_name_compact(inner: &syn::Type) -> TokenStream {
    quote! { ::layout::CompactPtrMut<#inner> }
}

pub fn drain_name_compact(inner: &syn::Type) -> TokenStream {
    quote! { ::layout::CompactDrain<'a, #inner> }
}
