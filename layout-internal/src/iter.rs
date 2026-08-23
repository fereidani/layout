use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

use crate::{input::Input, names};

pub fn derive(input: &Input) -> TokenStream {
    let name = &input.name;
    let visibility = &input.visibility;
    let vec_name = names::vec_name(&input.name);
    let slice_name = names::slice_name(name);
    let slice_mut_name = names::slice_mut_name(&input.name);
    let ref_name = names::ref_name(&input.name);
    let ref_mut_name = names::ref_mut_name(&input.name);
    let iter_name = names::iter_name(&input.name);
    let iter_mut_name = names::iter_mut_name(&input.name);

    let doc_url = format!("[`{0}`](struct.{0}.html)", name);
    let ref_doc_url = format!("[`{0}`](struct.{0}.html)", ref_name);
    let ref_mut_doc_url = format!("[`{0}`](struct.{0}.html)", ref_mut_name);

    let fields_names = &input.field_idents();

    let fields_types = &input
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    // Remaining-length counter; hygienic name so it cannot collide with a
    // user field.
    let rem = Ident::new("___layout_private_rem", Span::call_site());

    let iter_fields_types = input
        .map_fields_nested_or(
            |_, field_type, _| quote! { <#field_type as layout::SoAIter<'a>>::Iter },
            |_, field_type| quote! { ::layout::ColumnCursor<'a, #field_type> },
        )
        .collect::<Vec<_>>();

    let iter_mut_fields_types = input
        .map_fields_nested_or(
            |_, field_type, _| quote! { <#field_type as layout::SoAIter<'a>>::IterMut },
            |_, field_type| quote! { ::layout::ColumnCursorMut<'a, #field_type> },
        )
        .collect::<Vec<_>>();

    // Sub-iterator construction: consuming a slice moves compact/nested
    // fields into their owning iterators; borrowing paths reborrow.
    let into_iter_fields = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.into_iter() },
            |ident, _| quote! { ::layout::ColumnCursor::new(self.#ident) },
        )
        .collect::<Vec<_>>();

    let into_iter_mut_fields = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.into_iter() },
            |ident, _| quote! { ::layout::ColumnCursorMut::new(self.#ident) },
        )
        .collect::<Vec<_>>();

    // The Ref construction sites below use trailing-comma field-init shorthand,
    // so the marker init carries no leading comma. Only non-empty for
    // all-compact structs.
    let ref_marker_init = input.ref_marker_init(false);

    let generated = quote! {
        /// Iterator over
        #[doc = #doc_url]
        ///
        /// Holds one remaining-length counter and one cursor per column, so
        /// each element costs a single bounds decision regardless of the
        /// number of columns.
        #[allow(missing_debug_implementations)]
        #visibility struct #iter_name<'a> {
            #rem: usize,
            #( #fields_names: #iter_fields_types, )*
        }

        impl<'a> Iterator for #iter_name<'a> {
            type Item = #ref_name<'a>;

            #[inline]
            fn next(&mut self) -> Option<#ref_name<'a>> {
                if self.#rem == 0 {
                    return None;
                }
                self.#rem -= 1;
                // SAFETY: `rem` was positive, so every column cursor has at
                // least one front element left (all columns share one
                // length).
                unsafe {
                    Some(#ref_name {
                        #( #fields_names: ::layout::SoACursor::cursor_next(&mut self.#fields_names), )*
                        #ref_marker_init
                    })
                }
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                (self.#rem, Some(self.#rem))
            }

            #[inline]
            fn count(self) -> usize {
                self.#rem
            }
        }

        impl<'a> DoubleEndedIterator for #iter_name<'a> {
            #[inline]
            fn next_back(&mut self) -> Option<#ref_name<'a>> {
                if self.#rem == 0 {
                    return None;
                }
                self.#rem -= 1;
                // SAFETY: as in `next`, for the back end.
                unsafe {
                    Some(#ref_name {
                        #( #fields_names: ::layout::SoACursor::cursor_next_back(&mut self.#fields_names), )*
                        #ref_marker_init
                    })
                }
            }
        }

        impl<'a> ExactSizeIterator for #iter_name<'a> {
            #[inline]
            fn len(&self) -> usize {
                self.#rem
            }
        }

        impl<'a> ::layout::SoACursor for #iter_name<'a> {
            type Item = #ref_name<'a>;

            #[inline(always)]
            unsafe fn cursor_next(&mut self) -> #ref_name<'a> {
                // Driven as a nested column by an enclosing single-counter
                // iterator; the parent's counter carries the length
                // guarantee, so `rem` is not maintained here.
                // SAFETY: forwarded caller contract.
                unsafe {
                    #ref_name {
                        #( #fields_names: ::layout::SoACursor::cursor_next(&mut self.#fields_names), )*
                        #ref_marker_init
                    }
                }
            }

            #[inline(always)]
            unsafe fn cursor_next_back(&mut self) -> #ref_name<'a> {
                // SAFETY: forwarded caller contract.
                unsafe {
                    #ref_name {
                        #( #fields_names: ::layout::SoACursor::cursor_next_back(&mut self.#fields_names), )*
                        #ref_marker_init
                    }
                }
            }
        }

        impl #vec_name {
            /// Get an iterator over the
            #[doc = #ref_doc_url]
            /// in this vector
            #[inline]
            pub fn iter(&self) -> #iter_name {
                self.as_slice().into_iter()
            }
        }

        impl<'a> #slice_name<'a> {
            /// Get an iterator over the
            #[doc = #ref_doc_url]
            /// in this slice.
            #[inline]
            pub fn iter(&self) -> #iter_name {
                self.reborrow().into_iter()
            }

            /// Get an iterator over the
            #[doc = #ref_doc_url]
            /// in this slice.
            #[inline]
            pub fn into_iter(self) -> #iter_name<'a> {
                #iter_name {
                    #rem: self.len(),
                    #( #fields_names: #into_iter_fields, )*
                }
            }
        }

        /// Mutable iterator over
        #[doc = #doc_url]
        ///
        /// Holds one remaining-length counter and one cursor per column, so
        /// each element costs a single bounds decision regardless of the
        /// number of columns.
        #[allow(missing_debug_implementations)]
        #visibility struct #iter_mut_name<'a> {
            #rem: usize,
            #( #fields_names: #iter_mut_fields_types, )*
        }

        impl<'a> Iterator for #iter_mut_name<'a> {
            type Item = #ref_mut_name<'a>;

            #[inline]
            fn next(&mut self) -> Option<#ref_mut_name<'a>> {
                if self.#rem == 0 {
                    return None;
                }
                self.#rem -= 1;
                // SAFETY: `rem` was positive, so every column cursor has at
                // least one front element left (all columns share one
                // length).
                unsafe {
                    Some(#ref_mut_name {
                        #( #fields_names: ::layout::SoACursor::cursor_next(&mut self.#fields_names), )*
                    })
                }
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                (self.#rem, Some(self.#rem))
            }

            #[inline]
            fn count(self) -> usize {
                self.#rem
            }
        }

        impl<'a> DoubleEndedIterator for #iter_mut_name<'a> {
            #[inline]
            fn next_back(&mut self) -> Option<#ref_mut_name<'a>> {
                if self.#rem == 0 {
                    return None;
                }
                self.#rem -= 1;
                // SAFETY: as in `next`, for the back end.
                unsafe {
                    Some(#ref_mut_name {
                        #( #fields_names: ::layout::SoACursor::cursor_next_back(&mut self.#fields_names), )*
                    })
                }
            }
        }

        impl<'a> ExactSizeIterator for #iter_mut_name<'a> {
            #[inline]
            fn len(&self) -> usize {
                self.#rem
            }
        }

        impl<'a> ::layout::SoACursor for #iter_mut_name<'a> {
            type Item = #ref_mut_name<'a>;

            #[inline(always)]
            unsafe fn cursor_next(&mut self) -> #ref_mut_name<'a> {
                // See the immutable cursor impl: the enclosing iterator's
                // counter carries the length guarantee.
                // SAFETY: forwarded caller contract.
                unsafe {
                    #ref_mut_name {
                        #( #fields_names: ::layout::SoACursor::cursor_next(&mut self.#fields_names), )*
                    }
                }
            }

            #[inline(always)]
            unsafe fn cursor_next_back(&mut self) -> #ref_mut_name<'a> {
                // SAFETY: forwarded caller contract.
                unsafe {
                    #ref_mut_name {
                        #( #fields_names: ::layout::SoACursor::cursor_next_back(&mut self.#fields_names), )*
                    }
                }
            }
        }

        impl #vec_name {
            /// Get a mutable iterator over the
            #[doc = #ref_mut_doc_url]
            /// in this vector
            #[inline]
            pub fn iter_mut(&mut self) -> #iter_mut_name {
                self.as_mut_slice().into_iter()
            }
        }

        impl<'a> #slice_mut_name<'a> {
            /// Get an iterator over the
            #[doc = #ref_doc_url]
            /// in this vector
            #[inline]
            pub fn iter(&mut self) -> #iter_name {
                self.as_ref().into_iter()
            }

            /// Get a mutable iterator over the
            #[doc = #ref_mut_doc_url]
            /// in this vector
            #[inline]
            pub fn iter_mut(&mut self) -> #iter_mut_name {
                self.reborrow().into_iter()
            }

            /// Get a mutable iterator over the
            #[doc = #ref_mut_doc_url]
            /// in this vector
            #[inline]
            pub fn into_iter(self) -> #iter_mut_name<'a> {
                #iter_mut_name {
                    #rem: self.len(),
                    #( #fields_names: #into_iter_mut_fields, )*
                }
            }
        }

        impl<'a> layout::SoAIter<'a> for #name {
            type Ref = #ref_name<'a>;
            type RefMut = #ref_mut_name<'a>;
            type Iter = #iter_name<'a>;
            type IterMut = #iter_mut_name<'a>;
        }

        impl<'a> IntoIterator for #slice_name<'a> {
            type Item = #ref_name<'a>;
            type IntoIter = #iter_name<'a>;
            #[inline]
            fn into_iter(self) -> Self::IntoIter {
                #slice_name::into_iter(self)
            }
        }


        impl core::iter::FromIterator<#name> for #vec_name {
            fn from_iter<T: IntoIterator<Item=#name>>(iter: T) -> Self {
                let iterator = iter.into_iter();
                // Lower bound like `std`'s `collect`: a filter's upper bound is
                // the source length, so reserving it over-allocates every column.
                let capacity = iterator.size_hint().0;
                let mut result = #vec_name::with_capacity(capacity);
                iterator.for_each(|element| result.push(element));
                result
            }
        }

        impl<'a, 'b> IntoIterator for &'a #slice_name<'b> {
            type Item = #ref_name<'a>;
            type IntoIter = #iter_name<'a>;
            #[inline]
            fn into_iter(self) -> Self::IntoIter {
                self.reborrow().into_iter()
            }
        }

        impl<'a> IntoIterator for &'a #vec_name {
            type Item = #ref_name<'a>;
            type IntoIter = #iter_name<'a>;
            #[inline]
            fn into_iter(self) -> Self::IntoIter {
                self.as_slice().into_iter()
            }
        }

        impl<'a> IntoIterator for #slice_mut_name<'a> {
            type Item = #ref_mut_name<'a>;
            type IntoIter = #iter_mut_name<'a>;
            #[inline]
            fn into_iter(self) -> Self::IntoIter {
                #slice_mut_name::into_iter(self)
            }
        }

        impl<'a> IntoIterator for &'a mut #vec_name {
            type Item = #ref_mut_name<'a>;
            type IntoIter = #iter_mut_name<'a>;
            #[inline]
            fn into_iter(self) -> Self::IntoIter {
                self.as_mut_slice().into_iter()
            }
        }

        impl Extend<#name> for #vec_name {
            fn extend<I: IntoIterator<Item = #name>>(&mut self, iter: I) {
                let iter = iter.into_iter();
                // Reserve the lower bound, like `std` and this crate's
                // `FromIterator`: a filter's upper bound is the source
                // length, so reserving it over-allocates every column.
                self.reserve(iter.size_hint().0);
                for item in iter {
                    self.push(item)
                }
            }
        }

        impl<'a> Extend<#ref_name<'a>> for #vec_name
            // only expose if all fields are Clone
            // https://github.com/rust-lang/rust/issues/48214#issuecomment-1150463333
            where #( for<'b> #fields_types: Clone, )*
        {
            fn extend<I: IntoIterator<Item = #ref_name<'a>>>(&mut self, iter: I) {
                <Self as Extend<#name>>::extend(self, iter.into_iter().map(|item| item.to_owned()))
            }
        }

        impl<'a> ::layout::IntoSoAIter<'a, #name> for #slice_name<'a> {}
    };

    return generated;
}
