use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

use crate::{input::Input, names};

pub fn derive(input: &Input) -> TokenStream {
    let name = &input.name;
    let visibility = &input.visibility;
    let attrs = &input.attrs.ref_;
    let mut_attrs = &input.attrs.ref_mut;
    let vec_name = names::vec_name(&input.name);
    let ref_name = names::ref_name(&input.name);
    let ref_mut_name = names::ref_mut_name(&input.name);

    let fields_types = &input
        .fields
        .iter()
        .map(|field| field.ty.clone())
        .collect::<Vec<_>>();

    let doc_url = format!("[`{0}`](struct.{0}.html)", name);
    let vec_doc_url = format!("[`{0}`](struct.{0}.html)", vec_name);
    let ref_doc_url = format!("[`{0}`](struct.{0}.html)", ref_name);
    let ref_mut_doc_url = format!("[`{0}`](struct.{0}.html)", ref_mut_name);

    let fields_names = &input
        .fields
        .iter()
        .map(|field| field.ident.clone().unwrap())
        .collect::<Vec<_>>();

    let fields_names_hygienic = input
        .fields
        .iter()
        .enumerate()
        .map(|(i, _)| {
            Ident::new(&format!("___layout_private_{}", i), Span::call_site())
        })
        .collect::<Vec<_>>();

    let ref_fields_types = input
        .map_fields_nested_or(
            |_, field_type, compact| {
                // Immutable access to a compact field yields the owning
                // `Compact<T>` value (Copy snapshot).
                if compact.is_some() {
                    quote! { #field_type }
                } else {
                    let field_ptr_type = names::ref_name(field_type);
                    quote! { #field_ptr_type<'a> }
                }
            },
            |_, field_type| quote! { &'a #field_type },
        )
        .collect::<Vec<_>>();

    let ref_mut_fields_types = input
        .map_fields_nested_or(
            |_, field_type, compact| {
                if let Some(inner) = compact {
                    names::ref_mut_name_compact(inner)
                } else {
                    let field_ptr_type = names::ref_mut_name(field_type);
                    quote! { #field_ptr_type<'a> }
                }
            },
            |_, field_type| quote! { &'a mut #field_type },
        )
        .collect::<Vec<_>>();

    let as_ref = input
        .map_fields_nested_or(
            |ident, _, compact| {
                // Compact<T> is Copy: snapshot the value directly.
                if compact.is_some() {
                    quote! { self.#ident }
                } else {
                    quote! { self.#ident.as_ref() }
                }
            },
            |ident, _| quote! { &self.#ident },
        )
        .collect::<Vec<_>>();

    let as_mut = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.as_mut() },
            |ident, _| quote! { &mut self.#ident },
        )
        .collect::<Vec<_>>();

    let to_owned = input
        .map_fields_nested_or(
            |ident, _, compact| {
                // Works for both Ref (field: Compact<T>) and RefMut
                // (field: CompactRefMut<T>): read via `.get()`.
                if compact.is_some() {
                    quote! { ::layout::Compact::new(self.#ident.get()) }
                } else {
                    quote! { self.#ident.to_owned() }
                }
            },
            |ident, _| quote! { self.#ident.clone() },
        )
        .collect::<Vec<_>>();

    let ref_replace = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.replace(field) },
            |ident, _| quote! { ::core::mem::replace(&mut *self.#ident, field) },
        )
        .collect::<Vec<_>>();

    // When every field is compact, the immutable `Ref<'a>` would otherwise have
    // an unused lifetime; add a hidden PhantomData marker to keep it
    // structural.
    let ref_marker_field: TokenStream = if input.ref_needs_lifetime_marker() {
        quote! {
            #[doc(hidden)]
            __layout_ref_marker: ::core::marker::PhantomData<&'a ()>,
        }
    } else {
        quote! {}
    };
    // `as_ref` builds the ref with a trailing-comma field list, so the init
    // carries no leading comma.
    let ref_marker_init: TokenStream = if input.ref_needs_lifetime_marker() {
        quote! { __layout_ref_marker: ::core::marker::PhantomData }
    } else {
        quote! {}
    };

    quote! {
        /// A reference to a
        #[doc = #doc_url]
        /// with struct of array layout.
        #(#[#attrs])*
        #[derive(Copy, Clone)]
        #visibility struct #ref_name<'a> {
            #(
                /// reference to the `
                #[doc = stringify!(#fields_names)]
                ///` field of a single
                #[doc = #doc_url]
                /// inside a
                #[doc = #vec_doc_url]
                pub #fields_names: #ref_fields_types,
            )*
            #ref_marker_field
        }

        /// A mutable reference to a
        #[doc = #doc_url]
        /// with struct of array layout.
        #(#[#mut_attrs])*
        #visibility struct #ref_mut_name<'a> {
            #(
                /// reference to the `
                #[doc = stringify!(#fields_names)]
                ///` field of a single
                #[doc = #doc_url]
                /// inside a
                #[doc = #vec_doc_url]
                pub #fields_names: #ref_mut_fields_types,
            )*
        }

        #[allow(dead_code)]
        impl #name {
            /// Create a
            #[doc = #ref_doc_url]
            /// from a borrowed
            #[doc = #doc_url]
            /// .
            #visibility fn as_ref(&self) -> #ref_name {
                #ref_name {
                    #( #fields_names: #as_ref, )*
                    #ref_marker_init
                }
            }

            /// Create a
            #[doc = #ref_mut_doc_url]
            /// from a mutably borrowed
            #[doc = #doc_url]
            /// .
            #visibility fn as_mut(&mut self) -> #ref_mut_name {
                #ref_mut_name {
                    #( #fields_names: #as_mut, )*
                }
            }
        }

        impl<'a> #ref_name<'a> {
            /// Convert a reference to
            #[doc = #doc_url]
            /// into an owned value. This is only available if all fields
            /// implement `Clone`.
            pub fn to_owned(&self) -> #name
                // only expose to_owned if all fields are Clone
                // https://github.com/rust-lang/rust/issues/48214#issuecomment-1150463333
                where #( for<'b> #fields_types: Clone, )*
            {
                #name {
                    #( #fields_names: #to_owned, )*
                }
            }
        }

        impl<'a>  From<#ref_name<'a>> for #name where #( for<'b> #fields_types: Clone, )* {
            fn from(value: #ref_name<'a>) -> #name {
                value.to_owned()
            }
        }

        impl<'a>  From<&'a #ref_name<'a>> for #name where #( for<'b> #fields_types: Clone, )* {
            fn from(value: &'a #ref_name<'a>) -> #name {
                value.to_owned()
            }
        }

        impl<'a> #ref_mut_name<'a> {
            /// Convert a mutable reference to
            #[doc = #doc_url]
            /// into an owned value. This is only available if all fields
            /// implement `Clone`.
            pub fn to_owned(&self) -> #name
                // only expose to_owned if all fields are Clone
                // https://github.com/rust-lang/rust/issues/48214#issuecomment-1150463333
                where #( for<'b> #fields_types: Clone, )*
            {
                #name {
                    #( #fields_names: #to_owned, )*
                }
            }

            /// Similar to [`core::mem::replace()`](https://doc.rust-lang.org/std/mem/fn.replace.html).
            pub fn replace(&mut self, val: #name) -> #name {
                // ManuallyDrop: fields are read out via ptr::read, so a mid-replace unwind can't double-free them.
                let mut val = ::core::mem::ManuallyDrop::new(val);
                #(
                    let field = unsafe { ::core::ptr::read(&val.#fields_names) };
                    let #fields_names_hygienic = #ref_replace;
                )*

                #name{#(#fields_names: #fields_names_hygienic),*}
            }
        }

        impl<'a>  From<#ref_mut_name<'a>> for #name where #( for<'b> #fields_types: Clone, )* {
            fn from(value: #ref_mut_name<'a>) -> #name {
                value.to_owned()
            }
        }

        impl<'a>  From<&'a #ref_mut_name<'a>> for #name where #( for<'b> #fields_types: Clone, )* {
            fn from(value: &'a #ref_mut_name<'a>) -> #name {
                value.to_owned()
            }
        }
    }
}
