use proc_macro2::{Span, TokenStream};
use quote::{quote, TokenStreamExt};
use syn::Ident;

use crate::{input::Input, names};

pub fn derive(input: &Input) -> TokenStream {
    let name = &input.name;
    let visibility = &input.visibility;
    let slice_name = names::slice_name(&input.name);
    let attrs = &input.attrs.slice;
    let vec_name = names::vec_name(&input.name);
    let ref_name = names::ref_name(&input.name);
    let ptr_name = names::ptr_name(&input.name);
    let chunks_name = names::chunks_name(&input.name);
    let chunks_exact_name = names::chunks_exact_name(&input.name);

    // The chunk iterators keep their cursor state next to one field per
    // column, so the state fields get hygienic names that cannot collide
    // with a user field called `pos`, `end` or `chunk_size`.
    let pos = Ident::new("___layout_private_pos", Span::call_site());
    let chunk_size =
        Ident::new("___layout_private_chunk_size", Span::call_site());
    let end_field = Ident::new("___layout_private_end", Span::call_site());

    let slice_chunk_fields = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.slice(self.#pos..end) },
            |ident, _| quote! { &self.#ident[self.#pos..end] },
        )
        .collect::<Vec<_>>();

    let slice_chunks_exact_fields = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.slice(self.#pos..chunk_end) },
            |ident, _| quote! { &self.#ident[self.#pos..chunk_end] },
        )
        .collect::<Vec<_>>();

    let slice_chunks_exact_remainder_fields = input
        .map_fields_nested_or(
            |ident, _, _| quote! { { let end = self.#ident.len(); self.#ident.slice(rem_start..end) } },
            |ident, _| quote! { &self.#ident[rem_start..] },
        )
        .collect::<Vec<_>>();

    // The immutable-Ref construction sites below use comma-separated field
    // lists (no trailing comma), so the marker init carries a leading
    // comma. It is only non-empty for all-compact structs (whose `Ref<'a>`
    // needs a PhantomData marker to use its lifetime).
    let ref_marker_init = input.ref_marker_init(true);

    let slice_subslice_fields = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.slice(range.clone()) },
            |ident, _| quote! { &self.#ident[range.clone()] },
        )
        .collect::<Vec<_>>();

    let slice_name_str = format!("[{}]", input.name);
    let doc_url = format!("[`{0}`](struct.{0}.html)", input.name);
    let vec_doc_url = format!("[`{0}`](struct.{0}.html)", vec_name);

    let fields_names = &input.field_idents();

    let first_field = &fields_names[0];

    let fields_names_hygienic_1 = input.hygienic_idents("1_");
    let fields_names_hygienic_2 = input.hygienic_idents("2_");

    let slice_fields_types = input
        .map_fields_nested_or(
            |_, field_type, compact| {
                names::nested_slice_ty(field_type, compact)
            },
            |_, field_type| quote! { &'a [#field_type] },
        )
        .collect::<Vec<_>>();

    let slice_reborrow = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.reborrow() },
            |ident, _| quote! { &self.#ident },
        )
        .collect::<Vec<_>>();

    // `to_vec` field values: plain columns wrap their `Vec` in `Column`;
    // compact/nested columns already produce the matching owning type.
    let slice_to_vec_fields = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.to_vec() },
            |ident, _| quote! { ::layout::Column::from_vec(self.#ident.to_vec()) },
        )
        .collect::<Vec<_>>();

    let slice_from_raw_parts = input
        .map_fields_nested_or(
            |ident, field_type, compact| {
                let slice_type = names::nested_slice_ty(field_type, compact);
                quote! { <#slice_type>::from_raw_parts(data.#ident, len) }
            },
            |ident, _| quote! { ::core::slice::from_raw_parts(data.#ident, len) },
        )
        .collect::<Vec<_>>();

    let mut generated = quote! {
        /// A slice of
        #[doc = #doc_url]
        /// inside a
        #[doc = #vec_doc_url]
        /// .
        #[allow(dead_code)]
        #[derive(Copy, Clone)]
        #(#[#attrs])*
        #[derive(Default)]
        #visibility struct #slice_name<'a> {
            #(
                /// slice of `
                #[doc = stringify!(#fields_names)]
                ///` inside a
                #[doc = #vec_doc_url]
                pub #fields_names: #slice_fields_types,
            )*
        }

        #[allow(dead_code)]
        impl<'a> #slice_name<'a> {
            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::len()`](https://doc.rust-lang.org/std/primitive.slice.html#method.len),
            /// the length of all fields should be the same.
            #[inline]
            pub fn len(&self) -> usize {
                let len = self.#first_field.len();
                #(debug_assert_eq!(self.#fields_names.len(), len);)*
                len
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::is_empty()`](https://doc.rust-lang.org/std/primitive.slice.html#method.is_empty),
            /// the length of all fields should be the same.
            #[inline]
            pub fn is_empty(&self) -> bool {
                let empty = self.#first_field.is_empty();
                #(debug_assert_eq!(self.#fields_names.is_empty(), empty);)*
                empty
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::first()`](https://doc.rust-lang.org/std/primitive.slice.html#method.first).
            pub fn first(&self) -> Option<#ref_name<'a>> {
                if ::layout::branches::unlikely(self.is_empty()) {
                    None
                } else {
                    #(
                        let #fields_names_hygienic_1 = self.#fields_names.first().unwrap();
                    )*
                    Some(#ref_name{#(#fields_names: #fields_names_hygienic_1),* #ref_marker_init})
                }
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::split_first()`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_first).
            pub fn split_first(&self) -> Option<(#ref_name<'a>, #slice_name<'a>)> {
                if ::layout::branches::unlikely(self.is_empty()) {
                    None
                } else {
                    #(
                        let (#fields_names_hygienic_1, #fields_names_hygienic_2) = self.#fields_names.split_first().unwrap();
                    )*
                    let ref_ = #ref_name{#(#fields_names: #fields_names_hygienic_1),* #ref_marker_init};
                    let slice = #slice_name{#(#fields_names: #fields_names_hygienic_2),*};
                    Some((ref_, slice))
                }
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::last()`](https://doc.rust-lang.org/std/primitive.slice.html#method.last).
            pub fn last(&self) -> Option<#ref_name<'a>> {
                if ::layout::branches::unlikely(self.is_empty()) {
                    None
                } else {
                    #(
                        let #fields_names_hygienic_1 = self.#fields_names.last().unwrap();
                    )*
                    Some(#ref_name{#(#fields_names: #fields_names_hygienic_1),* #ref_marker_init})
                }
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::split_last()`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_last).
            pub fn split_last(&self) -> Option<(#ref_name<'a>, #slice_name<'a>)> {
                if ::layout::branches::unlikely(self.is_empty()) {
                    None
                } else {
                    #(
                        let (#fields_names_hygienic_1, #fields_names_hygienic_2) = self.#fields_names.split_last().unwrap();
                    )*
                    let ref_ = #ref_name{#(#fields_names: #fields_names_hygienic_1),* #ref_marker_init};
                    let slice = #slice_name{#(#fields_names: #fields_names_hygienic_2),*};
                    Some((ref_, slice))
                }
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::split_at()`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_at).
            pub fn split_at(&self, mid: usize) -> (#slice_name<'a>, #slice_name<'a>) {
                #(
                    let (#fields_names_hygienic_1, #fields_names_hygienic_2) = self.#fields_names.split_at(mid);
                )*
                let left = #slice_name{#(#fields_names: #fields_names_hygienic_1),*};
                let right = #slice_name{#(#fields_names: #fields_names_hygienic_2),*};
                (left, right)
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::get()`](https://doc.rust-lang.org/std/primitive.slice.html#method.get).
            pub fn get<'b, I>(&'b self, index: I) -> Option<I::RefOutput>
            where
                I: ::layout::SoAIndex<#slice_name<'b>>,
                'a: 'b
            {
                let slice: #slice_name<'b> = self.reborrow();
                index.get(slice)
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::get_unchecked()`](https://doc.rust-lang.org/std/primitive.slice.html#method.get_unchecked).
            pub unsafe fn get_unchecked<'b, I>(&'b self, index: I) -> I::RefOutput
            where
                I: ::layout::SoAIndex<#slice_name<'b>>,
                'a: 'b
            {
                let slice: #slice_name<'b> = self.reborrow();
                index.get_unchecked(slice)
            }

            /// Similar to the
            /// [`core::ops::Index`](https://doc.rust-lang.org/std/ops/trait.Index.html)
            /// trait for `&
            #[doc = #slice_name_str]
            ///` .
            /// This is required because we cannot implement `core::ops::Index` directly since it requires returning a reference.
            pub fn index<'b, I>(&'b self, index: I) -> I::RefOutput
            where
                I: ::layout::SoAIndex<#slice_name<'b>>,
                'a: 'b
            {
                let slice: #slice_name<'b> = self.reborrow();
                index.index(slice)
            }

            /// Reborrows the slices in a narrower lifetime
            pub fn reborrow<'b>(&'b self) -> #slice_name<'b>
            where
                'a: 'b
            {
                #slice_name {
                    #( #fields_names: #slice_reborrow, )*
                }
            }

            /// Create a sub-slice matching the given `range`. This
            /// is analogous to `Index<Range<usize>>`.
            pub fn slice(&self, range: ::core::ops::Range<usize>) -> #slice_name<'a> {
                #slice_name {
                    #( #fields_names: #slice_subslice_fields, )*
                }
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::as_ptr()`](https://doc.rust-lang.org/std/primitive.slice.html#method.as_ptr).
            pub fn as_ptr(&self) -> #ptr_name {
                #ptr_name {
                    #(#fields_names: self.#fields_names.as_ptr(),)*
                }
            }

            /// Similar to [`core::slice::from_raw_parts()`](https://doc.rust-lang.org/std/slice/fn.from_raw_parts.html).
            pub unsafe fn from_raw_parts<'b>(data: #ptr_name, len: usize) -> #slice_name<'b> {
                #slice_name {
                    #( #fields_names: #slice_from_raw_parts, )*
                }
            }

            // --- binary_search ---

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::binary_search_by()`](https://doc.rust-lang.org/std/primitive.slice.html#method.binary_search_by).
            pub fn binary_search_by<F>(&self, mut f: F) -> ::core::result::Result<usize, usize>
            where
                F: FnMut(#ref_name) -> ::core::cmp::Ordering,
            {
                let mut left = 0usize;
                let mut right = self.len();
                while left < right {
                    let mid = left + (right - left) / 2;
                    match f(self.index(mid)) {
                        ::core::cmp::Ordering::Less => left = mid + 1,
                        ::core::cmp::Ordering::Greater => right = mid,
                        ::core::cmp::Ordering::Equal => return Ok(mid),
                    }
                }
                Err(left)
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::binary_search_by_key()`](https://doc.rust-lang.org/std/primitive.slice.html#method.binary_search_by_key).
            pub fn binary_search_by_key<K, F>(&self, key: &K, mut f: F) -> ::core::result::Result<usize, usize>
            where
                K: ::core::cmp::Ord,
                F: FnMut(#ref_name) -> K,
            {
                self.binary_search_by(|probe| f(probe).cmp(key))
            }

            // --- chunks ---

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::chunks()`](https://doc.rust-lang.org/std/primitive.slice.html#method.chunks).
            pub fn chunks(&self, chunk_size: usize) -> #chunks_name<'a> {
                assert!(chunk_size != 0, "chunk size must be non-zero");
                #chunks_name {
                    #( #fields_names: self.#fields_names, )*
                    #chunk_size: chunk_size,
                    #pos: 0,
                }
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::chunks_exact()`](https://doc.rust-lang.org/std/primitive.slice.html#method.chunks_exact).
            pub fn chunks_exact(&self, chunk_size: usize) -> #chunks_exact_name<'a> {
                assert!(chunk_size != 0, "chunk size must be non-zero");
                let rem = self.len() % chunk_size;
                let end = self.len() - rem;
                #chunks_exact_name {
                    #( #fields_names: self.#fields_names, )*
                    #chunk_size: chunk_size,
                    #pos: 0,
                    #end_field: end,
                }
            }
        }

        /// An iterator over non-overlapping chunks of a SoA slice.
        #[allow(missing_debug_implementations)]
        #visibility struct #chunks_name<'a> {
            #( #fields_names: #slice_fields_types, )*
            #chunk_size: usize,
            #pos: usize,
        }

        #[allow(dead_code)]
        impl<'a> Iterator for #chunks_name<'a> {
            type Item = #slice_name<'a>;

            #[inline]
            fn next(&mut self) -> Option<#slice_name<'a>> {
                let len = self.#first_field.len();
                if self.#pos >= len || self.#chunk_size == 0 {
                    return None;
                }
                let end = (self.#pos + self.#chunk_size).min(len);
                let result = #slice_name {
                    #( #fields_names: #slice_chunk_fields, )*
                };
                self.#pos = end;
                Some(result)
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                if self.#chunk_size == 0 {
                    return (0, Some(0));
                }
                let remaining = self.#first_field.len().saturating_sub(self.#pos);
                let count =
                    remaining / self.#chunk_size + usize::from(remaining % self.#chunk_size != 0);
                (count, Some(count))
            }

            #[inline]
            fn count(self) -> usize {
                self.size_hint().0
            }
        }

        #[allow(dead_code)]
        impl<'a> ::core::iter::ExactSizeIterator for #chunks_name<'a> {}

        /// An iterator over non-overlapping exact chunks of a SoA slice.
        #[allow(missing_debug_implementations)]
        #visibility struct #chunks_exact_name<'a> {
            #( #fields_names: #slice_fields_types, )*
            #chunk_size: usize,
            #pos: usize,
            #end_field: usize,
        }

        #[allow(dead_code)]
        impl<'a> Iterator for #chunks_exact_name<'a> {
            type Item = #slice_name<'a>;

            #[inline]
            fn next(&mut self) -> Option<#slice_name<'a>> {
                if self.#pos >= self.#end_field || self.#chunk_size == 0 {
                    return None;
                }
                let chunk_end = self.#pos + self.#chunk_size;
                let result = #slice_name {
                    #( #fields_names: #slice_chunks_exact_fields, )*
                };
                self.#pos = chunk_end;
                Some(result)
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                if self.#chunk_size == 0 {
                    return (0, Some(0));
                }
                let remaining = self.#end_field.saturating_sub(self.#pos);
                let count = remaining / self.#chunk_size;
                (count, Some(count))
            }

            #[inline]
            fn count(self) -> usize {
                self.size_hint().0
            }
        }

        #[allow(dead_code)]
        impl<'a> #chunks_exact_name<'a> {
            /// Returns the remainder of the original slice not yielded by the iterator.
            pub fn remainder(&self) -> #slice_name<'a> {
                let rem_start = self.#end_field.min(self.#first_field.len());
                #slice_name {
                    #( #fields_names: #slice_chunks_exact_remainder_fields, )*
                }
            }
        }

    };

    if input.attrs.derive_clone {
        generated.append_all(quote! {
            #[allow(dead_code)]
            impl<'a> #slice_name<'a> {
                /// Similar to [`&
                #[doc = #slice_name_str]
                /// ::to_vec()`](https://doc.rust-lang.org/std/primitive.slice.html#method.to_vec).
                pub fn to_vec(&self) -> #vec_name {
                    #vec_name {
                        #(#fields_names: #slice_to_vec_fields,)*
                    }
                }
            }
        });

        generated.append_all(quote! {
            impl<'a> ::layout::ToSoAVec<#name> for #slice_name<'a> {
                type SoAVecType = #vec_name;

                fn to_vec(&self) -> Self::SoAVecType {
                    self.to_vec()
                }
            }
        });
    }

    return generated;
}

pub fn derive_mut(input: &Input) -> TokenStream {
    let name = &input.name;
    let visibility = &input.visibility;
    let slice_name = names::slice_name(&input.name);
    let slice_mut_name = names::slice_mut_name(&input.name);
    let vec_name = names::vec_name(&input.name);
    let attrs = &input.attrs.slice_mut;
    let ref_name = names::ref_name(&input.name);
    let ref_mut_name = names::ref_mut_name(&input.name);
    let ptr_name = names::ptr_name(&input.name);
    let ptr_mut_name = names::ptr_mut_name(&input.name);
    let chunks_mut_name = names::chunks_mut_name(&input.name);
    let chunks_exact_mut_name = names::chunks_exact_mut_name(&input.name);

    // Column swap after one shared bounds check: plain columns swap through
    // raw pointers (`slice::swap` would re-check both indices per column),
    // compact and nested columns keep their own checked op, whose cost is
    // dominated by the bit or sub-column work.
    let swap_unchecked = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.swap(a, b) },
            |ident, _| {
                quote! {
                    {
                        let base = self.#ident.as_mut_ptr();
                        // SAFETY: `a` and `b` are both `< len`, which is the
                        // length of every column.
                        unsafe {
                            ::core::ptr::swap(base.add(a), base.add(b));
                        }
                    }
                }
            },
        )
        .collect::<Vec<_>>();

    let slice_name_str = format!("[{}]", input.name);
    let doc_url = format!("[`{0}`](struct.{0}.html)", input.name);
    let slice_doc_url = format!("[`{0}`](struct.{0}.html)", slice_name);
    let slice_mut_doc_url = format!("[`{0}`](struct.{0}.html)", slice_mut_name);
    let vec_doc_url = format!("[`{0}`](struct.{0}.html)", vec_name);

    let fields_names = &input.field_idents();

    let first_field = &fields_names[0];
    let fields_names_hygienic_1 = &input.hygienic_idents("slice_1_");
    let fields_names_hygienic_2 = &input.hygienic_idents("slice_2_");

    let slice_mut_fields_types = input
        .map_fields_nested_or(
            |_, field_type, compact| {
                names::nested_slice_mut_ty(field_type, compact)
            },
            |_, field_type| quote! { &'a mut [#field_type] },
        )
        .collect::<Vec<_>>();

    let slice_as_ref = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.as_ref() },
            |ident, _| quote! { self.#ident },
        )
        .collect::<Vec<_>>();

    let slice_as_slice = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.as_slice() },
            |ident, _| quote! { &self.#ident },
        )
        .collect::<Vec<_>>();

    let slice_reborrow = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.reborrow() },
            |ident, _| quote! { &mut self.#ident },
        )
        .collect::<Vec<_>>();

    // `to_vec` field values (see the matching definition for the immutable
    // slice): plain columns wrap their `Vec` in `Column`.
    let slice_to_vec_fields = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.to_vec() },
            |ident, _| quote! { ::layout::Column::from_vec(self.#ident.to_vec()) },
        )
        .collect::<Vec<_>>();

    let slice_from_raw_parts_mut = input
        .map_fields_nested_or(
            |ident, field_type, compact| {
                let slice_type = names::nested_slice_mut_ty(field_type, compact);
                quote! { <#slice_type>::from_raw_parts_mut(data.#ident, len) }
            },
            |ident, _| quote! {::core::slice::from_raw_parts_mut(data.#ident, len) },
        )
        .collect::<Vec<_>>();

    let mut nested_ord = input
        .map_fields_nested_or(
            |_, field_type, compact| {
                if compact.is_some() {
                    // Compact columns: Ord is unconditional, so no bound is
                    // needed.
                    quote! {}
                } else {
                    let field_ref_type = names::ref_name(field_type);
                    quote! { for<'b> #field_ref_type<'b>: Ord }
                }
            },
            |_, _| quote! {},
        )
        .filter(|stream| !stream.is_empty())
        .collect::<Vec<_>>();
    nested_ord.push(quote! { for<'b> #ref_name<'b>: Ord });

    let apply_permutation_unchecked = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.__private_apply_permutation_unchecked(dest, visited) },
            |ident, _| quote! { ::layout::__apply_permutation_inplace_unchecked(&mut self.#ident, dest, visited) },
        )
        .collect::<Vec<_>>();

    let apply_argsort_unchecked = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.__private_apply_argsort_unchecked(argsort) },
            |ident, _| quote! { ::layout::__apply_argsort_unchecked(&mut self.#ident, argsort) },
        )
        .collect::<Vec<_>>();

    let fields_names_len =
        Ident::new("___layout_private_len", Span::call_site());
    // Hygienic cursor state for the mutable chunk iterators (see `derive`).
    let pos = Ident::new("___layout_private_pos", Span::call_site());
    let chunk_size =
        Ident::new("___layout_private_chunk_size", Span::call_site());
    let end_field = Ident::new("___layout_private_end", Span::call_site());

    // Raw pointer field types for mutable chunk iterators.
    // Storing raw pointers instead of &mut [T] avoids creating overlapping
    // mutable references, which is UB under the Stacked Borrows model.
    let chunks_mut_ptr_fields_types = input
        .map_fields_nested_or(
            |_, field_type, compact| {
                names::nested_ptr_mut_ty(field_type, compact)
            },
            |_, field_type| quote! { *mut #field_type },
        )
        .collect::<Vec<_>>();

    // A mutable chunk rebuilds a sub-slice of `len` elements from the raw
    // pointer stored per column, offset to the iterator's current position.
    let chunk_from_ptr = |len: TokenStream| {
        input
            .map_fields_nested_or(
                |ident, field_type, compact| {
                    let slice_mut_type =
                        names::nested_slice_mut_ty(field_type, compact);
                    quote! { unsafe { <#slice_mut_type>::from_raw_parts_mut(self.#ident.add(self.#pos), #len) } }
                },
                |ident, _| quote! { unsafe { ::core::slice::from_raw_parts_mut(self.#ident.add(self.#pos), #len) } },
            )
            .collect::<Vec<_>>()
    };

    let chunks_mut_next_fields = chunk_from_ptr(quote! { chunk_len });
    let chunks_exact_mut_next_fields =
        chunk_from_ptr(quote! { self.#chunk_size });

    let chunks_exact_mut_remainder_fields = input
        .map_fields_nested_or(
            |ident, field_type, compact| {
                let slice_mut_type =
                    names::nested_slice_mut_ty(field_type, compact);
                quote! { unsafe { <#slice_mut_type>::from_raw_parts_mut(self.#ident.add(rem_start), rem_len) } }
            },
            |ident, _| quote! { unsafe { ::core::slice::from_raw_parts_mut(self.#ident.add(rem_start), rem_len) } },
        )
        .collect::<Vec<_>>();

    let mut generated = quote! {
        /// A mutable slice of
        #[doc = #doc_url]
        /// inside a
        #[doc = #vec_doc_url]
        /// .
        #[allow(dead_code)]
        #(#[#attrs])*
        #[derive(Default)]
        #visibility struct #slice_mut_name<'a> {
            #(
                /// slice of `
                #[doc = stringify!(#fields_names)]
                ///` inside a
                #[doc = #vec_doc_url]
                pub #fields_names: #slice_mut_fields_types,
            )*
        }

        #[allow(dead_code)]
        impl<'a> #slice_mut_name<'a> {
            /// Convert a
            #[doc = #slice_mut_doc_url]
            /// to a
            #[doc = #slice_doc_url]
            /// in order to be able to use the methods on the non mutable
            /// version of the slices.
            pub fn as_ref(&self) -> #slice_name {
                #slice_name {
                    #( #fields_names: #slice_as_ref, )*
                }
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::len()`](https://doc.rust-lang.org/std/primitive.slice.html#method.len),
            /// the length of all fields should be the same.
            #[inline]
            pub fn len(&self) -> usize {
                let len = self.#first_field.len();
                #(debug_assert_eq!(self.#fields_names.len(), len);)*
                len
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::is_empty()`](https://doc.rust-lang.org/std/primitive.slice.html#method.is_empty),
            /// the length of all fields should be the same.
            #[inline]
            pub fn is_empty(&self) -> bool {
                let empty = self.#first_field.is_empty();
                #(debug_assert_eq!(self.#fields_names.is_empty(), empty);)*
                empty
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::first_mut()`](https://doc.rust-lang.org/std/primitive.slice.html#method.first_mut).
            pub fn first_mut(&mut self) -> Option<#ref_mut_name> {
                if ::layout::branches::unlikely(self.is_empty()) {
                    None
                } else {
                    #(
                        let #fields_names = self.#fields_names.first_mut().unwrap();
                    )*
                    Some(#ref_mut_name{#(#fields_names: #fields_names),*})
                }
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            ///::split_first_mut()`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_first_mut).
            ///
            /// The main difference is that this function consumes the slice.
            /// You should use [`Self::reborrow()`] first if you want the
            /// returned values to have a shorter lifetime.
            pub fn split_first_mut(mut self) -> Option<(#ref_mut_name<'a>, #slice_mut_name<'a>)> {
                if ::layout::branches::unlikely(self.is_empty()) {
                    None
                } else {
                    #(
                        let (#fields_names, #fields_names_hygienic_1) = self.#fields_names.split_first_mut().unwrap();
                    )*
                    let ref_ = #ref_mut_name{#(#fields_names: #fields_names),*};
                    let slice = #slice_mut_name{#(#fields_names: #fields_names_hygienic_1),*};
                    Some((ref_, slice))
                }
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::last_mut()`](https://doc.rust-lang.org/std/primitive.slice.html#method.last_mut).
            pub fn last_mut(&mut self) -> Option<#ref_mut_name> {
                if ::layout::branches::unlikely(self.is_empty()) {
                    None
                } else {
                    #(
                        let #fields_names = self.#fields_names.last_mut().unwrap();
                    )*
                    Some(#ref_mut_name{#(#fields_names: #fields_names),*})
                }
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::last_mut()`](https://doc.rust-lang.org/std/primitive.slice.html#method.last_mut).
            ///
            /// The main difference is that this function consumes the slice.
            /// You should use [`Self::reborrow()`] first if you want the
            /// returned values to have a shorter lifetime.
            pub fn split_last_mut(mut self) -> Option<(#ref_mut_name<'a>, #slice_mut_name<'a>)> {
                if ::layout::branches::unlikely(self.is_empty()) {
                    None
                } else {
                    #(
                        let (#fields_names, #fields_names_hygienic_1) = self.#fields_names.split_last_mut().unwrap();
                    )*
                    let ref_ = #ref_mut_name{#(#fields_names: #fields_names),*};
                    let slice = #slice_mut_name{#(#fields_names: #fields_names_hygienic_1),*};
                    Some((ref_, slice))
                }
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::split_at_mut()`](https://doc.rust-lang.org/std/primitive.slice.html#method.split_at_mut).
            ///
            /// The main difference is that this function consumes the slice.
            /// You should use [`Self::reborrow()`] first if you want the
            /// returned values to have a shorter lifetime.
            pub fn split_at_mut(mut self, mid: usize) -> (#slice_mut_name<'a>, #slice_mut_name<'a>) {
                #(
                    let (#fields_names_hygienic_1, #fields_names_hygienic_2) = self.#fields_names.split_at_mut(mid);
                )*
                let left = #slice_mut_name{#(#fields_names: #fields_names_hygienic_1),*};
                let right = #slice_mut_name{#(#fields_names: #fields_names_hygienic_2),*};
                (left, right)
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::swap()`](https://doc.rust-lang.org/std/primitive.slice.html#method.swap).
            #[inline]
            pub fn swap(&mut self, a: usize, b: usize) {
                let len = self.len();
                if ::layout::branches::unlikely(a >= len) {
                    ::layout::panics::index_out_of_bounds(a, len);
                }
                if ::layout::branches::unlikely(b >= len) {
                    ::layout::panics::index_out_of_bounds(b, len);
                }
                #( #swap_unchecked; )*
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::get()`](https://doc.rust-lang.org/std/primitive.slice.html#method.get).
            pub fn get<'b, I>(&'b self, index: I) -> Option<I::RefOutput>
            where
                I: ::layout::SoAIndex<#slice_name<'b>>,
                'a: 'b
            {
                let slice: #slice_name<'b> = self.as_slice();
                index.get(slice)
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::get_unchecked()`](https://doc.rust-lang.org/std/primitive.slice.html#method.get_unchecked).
            pub unsafe fn get_unchecked<'b, I>(&'b self, index: I) -> I::RefOutput
            where
                I: ::layout::SoAIndex<#slice_name<'b>>,
                'a: 'b
            {
                let slice: #slice_name<'b> = self.as_slice();
                index.get_unchecked(slice)
            }


            /// Similar to the
            /// [`core::ops::Index`](https://doc.rust-lang.org/std/ops/trait.Index.html)
            /// trait for `&
            #[doc = #slice_name_str]
            ///` .
            /// This is required because we cannot implement that trait.
            pub fn index<'b, I>(&'b self, index: I) -> I::RefOutput
            where
                I: ::layout::SoAIndex<#slice_name<'b>>,
                'a: 'b
            {
                let slice: #slice_name<'b> = self.as_slice();
                index.index(slice)
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::get_mut()`](https://doc.rust-lang.org/std/primitive.slice.html#method.get_mut).
            pub fn get_mut<'b, I>(&'b mut self, index: I) -> Option<I::MutOutput>
            where
                I: ::layout::SoAIndexMut<#slice_mut_name<'b>>,
                'a: 'b
            {
                let slice: #slice_mut_name<'b> = self.reborrow();
                index.get_mut(slice)
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::get_unchecked_mut()`](https://doc.rust-lang.org/std/primitive.slice.html#method.get_unchecked_mut).
            pub unsafe fn get_unchecked_mut<'b, I>(&'b mut self, index: I) -> I::MutOutput
            where
                I: ::layout::SoAIndexMut<#slice_mut_name<'b>>,
                'a: 'b
            {
                let slice: #slice_mut_name<'b> = self.reborrow();
                index.get_unchecked_mut(slice)
            }

            /// Similar to the
            /// [`core::ops::IndexMut`](https://doc.rust-lang.org/std/ops/trait.IndexMut.html)
            /// trait for `&mut
            #[doc = #slice_name_str]
            ///` .
            /// This is required because we cannot implement `core::ops::IndexMut` directly since it requires returning a mutable reference.
            pub fn index_mut<'b, I>(&'b mut self, index: I) -> I::MutOutput
            where
                I: ::layout::SoAIndexMut<#slice_mut_name<'b>>,
                'a: 'b
            {
                let slice: #slice_mut_name<'b> = self.reborrow();
                index.index_mut(slice)
            }

            /// Returns a non-mutable slice from this mutable slice.
            pub fn as_slice<'b>(&'b self) -> #slice_name<'b>
            where
                'a: 'b
            {
                #slice_name {
                    #( #fields_names: #slice_as_slice, )*
                }
            }

            /// Reborrows the slices in a narrower lifetime
            pub fn reborrow<'b>(&'b mut self) -> #slice_mut_name<'b>
            where
                'a: 'b
            {
                #slice_mut_name {
                    #( #fields_names: #slice_reborrow, )*
                }
            }

            /// Similar to [`&
            #[doc = #slice_name_str]
            /// ::as_ptr()`](https://doc.rust-lang.org/std/primitive.slice.html#method.as_ptr).
            pub fn as_ptr(&self) -> #ptr_name {
                #ptr_name {
                    #(#fields_names: self.#fields_names.as_ptr(),)*
                }
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::as_mut_ptr()`](https://doc.rust-lang.org/std/primitive.slice.html#method.as_mut_ptr).
            pub fn as_mut_ptr(&mut self) -> #ptr_mut_name {
                #ptr_mut_name {
                    #(#fields_names: self.#fields_names.as_mut_ptr(),)*
                }
            }

            /// Similar to [`core::slice::from_raw_parts_mut()`](https://doc.rust-lang.org/std/slice/fn.from_raw_parts_mut.html).
            pub unsafe fn from_raw_parts_mut<'b>(data: #ptr_mut_name, len: usize) -> #slice_mut_name<'b> {
                #slice_mut_name {
                    #( #fields_names: #slice_from_raw_parts_mut, )*
                }
            }

            #[doc(hidden)]
            /// This is `pub` due to there will be compile-error if `#[nested_soa]` is used.
            /// Do not use this method directly.
            pub fn __private_apply_permutation(&mut self, dest: &[usize]) {
                let mut visited = ::layout::__validate_permutation(dest, self.len());
                // SAFETY: `dest` was just validated as a permutation of
                // `0..len`.
                unsafe {
                    self.__private_apply_permutation_unchecked(&dest, &mut visited);
                }
            }

            #[doc(hidden)]
            /// Apply a destination permutation to every column without
            /// re-validating it per column. Do not use this method directly.
            ///
            /// # Safety
            ///
            /// `dest` must be a permutation of `0..self.len()` and `visited`
            /// must have been created with capacity for at least `self.len()`
            /// bits.
            pub unsafe fn __private_apply_permutation_unchecked(
                &mut self,
                dest: &[usize],
                visited: &mut ::layout::VisitedBits,
            ) {
                #( #apply_permutation_unchecked; )*
            }

            #[doc(hidden)]
            /// Reorder every column so that position `pos` receives the row
            /// that was at `argsort[pos]`, without validating `argsort`. Do
            /// not use this method directly.
            ///
            /// # Safety
            ///
            /// `argsort` must be a permutation of `0..self.len()`.
            pub unsafe fn __private_apply_argsort_unchecked(
                &mut self,
                argsort: &[usize],
            ) {
                #( #apply_argsort_unchecked; )*
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::sort_by()`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by).
            pub fn sort_by<F>(&mut self, mut f: F)
            where
                F: FnMut(#ref_name, #ref_name) -> core::cmp::Ordering,
            {
                let len = self.len();
                if len <= 1 {
                    return;
                }
                let mut permutation: Vec<usize> = (0..len).collect();
                // Unstable sort with an index tiebreak: equal elements keep
                // their original order (the observable result of a stable
                // sort) without the stable sort's temporary buffer.
                permutation.sort_unstable_by(|j, k| {
                    f(self.index(*j), self.index(*k)).then_with(|| j.cmp(k))
                });

                // SAFETY: `permutation` is `0..len` reordered by the sort, so
                // it is a permutation of `0..len` by construction.
                unsafe {
                    self.__private_apply_argsort_unchecked(&permutation);
                }
            }

            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::sort_by_key()`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by_key).
            pub fn sort_by_key<F, K>(&mut self, mut f: F)
            where
                F: FnMut(#ref_name) -> K,
                K: Ord,
            {
                let len = self.len();
                if len <= 1 {
                    return;
                }
                // Evaluate the key once per element, then sort (key, index)
                // pairs so comparisons touch only the cached keys instead of
                // rebuilding a Ref (every column) on each of the O(n log n)
                // comparisons. The index tiebreak keeps equal keys in their
                // original order (stable result) while permitting an unstable
                // sort, which needs no temporary buffer.
                let mut keyed: Vec<(K, usize)> =
                    (0..len).map(|i| (f(self.index(i)), i)).collect();
                keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
                let permutation: Vec<usize> =
                    keyed.iter().map(|&(_, i)| i).collect();
                drop(keyed);

                // SAFETY: the pair indices are `0..len` reordered by the sort,
                // so `permutation` is a permutation of `0..len` by
                // construction.
                unsafe {
                    self.__private_apply_argsort_unchecked(&permutation);
                }
            }
        }

        #[allow(dead_code)]
        impl<'a> #slice_mut_name<'a>
        where
            #( #nested_ord, )*
        {
            /// Similar to [`&mut
            #[doc = #slice_name_str]
            /// ::sort()`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort).
            pub fn sort(&mut self) {
                let len = self.len();
                if len <= 1 {
                    return;
                }
                let mut permutation: Vec<usize> = (0..len).collect();
                permutation.sort_unstable_by(|j, k| {
                    self.index(*j).cmp(&self.index(*k)).then_with(|| j.cmp(k))
                });

                // SAFETY: `permutation` is `0..len` reordered by the sort, so
                // it is a permutation of `0..len` by construction.
                unsafe {
                    self.__private_apply_argsort_unchecked(&permutation);
                }
            }
        }
    };

    // --- dedup methods on SliceMut ---

    generated.append_all(quote! {
            #[allow(dead_code)]
            impl<'a> #slice_mut_name<'a> {
                /// Similar to [`&mut
                #[doc = #slice_name_str]
                /// ::dedup_by()`](https://doc.rust-lang.org/std/primitive/slice.html#method.dedup_by),
                /// but returns the new length: a slice cannot resize itself, so
                /// the compacted elements occupy `[0, return)` and the slots in
                /// `[return, len)` still hold stale values. Truncate the owning
                /// `Vec` to the returned length to drop them.
                ///
                /// As in `std`, the predicate receives the two elements in
                /// opposite order from their order in the slice: if
                /// `same_bucket(a, b)` returns `true`, `a` (the later element)
                /// is the one dropped from the compacted prefix.
                pub fn dedup_by<F>(&mut self, mut same_bucket: F) -> usize
                where
                    F: FnMut(#ref_name, #ref_name) -> bool,
                {
                    let len = self.len();
                    if len <= 1 {
                        return len;
                    }
                    let mut write = 1;
                    for read in 1..len {
                        if !same_bucket(self.index(read), self.index(write - 1)) {
                            if write != read {
                                self.swap(write, read);
                            }
                            write += 1;
                        }
                    }
                    write
                }

                /// Similar to [`&mut
                #[doc = #slice_name_str]
                /// ::dedup_by_key()`](https://doc.rust-lang.org/std/primitive/slice.html#method.dedup_by_key).
                /// Returns the new length (see [`dedup_by`](Self::dedup_by)).
                pub fn dedup_by_key<K, F>(&mut self, mut key: F) -> usize
                where
                    K: PartialEq,
                    F: FnMut(#ref_name) -> K,
                {
                    self.dedup_by(|a, b| key(a) == key(b))
                }

                /// Similar to [`&mut
                #[doc = #slice_name_str]
                /// ::binary_search_by()`](https://doc.rust-lang.org/std/primitive.slice.html#method.binary_search_by).
                pub fn binary_search_by<F>(&self, mut f: F) -> ::core::result::Result<usize, usize>
                where
                    F: FnMut(#ref_name) -> ::core::cmp::Ordering,
                {
                    self.as_ref().binary_search_by(f)
                }

                /// Similar to [`&mut
                #[doc = #slice_name_str]
                /// ::binary_search_by_key()`](https://doc.rust-lang.org/std/primitive.slice.html#method.binary_search_by_key).
                pub fn binary_search_by_key<K, F>(&self, key: &K, mut f: F) -> ::core::result::Result<usize, usize>
                where
                    K: ::core::cmp::Ord,
                    F: FnMut(#ref_name) -> K,
                {
                    self.as_ref().binary_search_by_key(key, f)
                }
            }
        });

    // --- chunks_mut iterator types ---

    generated.append_all(quote! {
            /// An iterator over non-overlapping mutable chunks of a SoA slice.
            #[allow(missing_debug_implementations)]
            #visibility struct #chunks_mut_name<'a> {
                #( #fields_names: #chunks_mut_ptr_fields_types, )*
                #fields_names_len: usize,
                #chunk_size: usize,
                #pos: usize,
                _marker: ::core::marker::PhantomData<&'a mut #name>,
            }

            #[allow(dead_code)]
            impl<'a> Iterator for #chunks_mut_name<'a> {
                type Item = #slice_mut_name<'a>;

                #[inline]
                fn next(&mut self) -> Option<#slice_mut_name<'a>> {
                    if self.#pos >= self.#fields_names_len || self.#chunk_size == 0 {
                        return None;
                    }
                    let end = (self.#pos + self.#chunk_size).min(self.#fields_names_len);
                    let chunk_len = end - self.#pos;
                    let result = #slice_mut_name {
                        #( #fields_names: #chunks_mut_next_fields, )*
                    };
                    self.#pos = end;
                    Some(result)
                }

                #[inline]
                fn size_hint(&self) -> (usize, Option<usize>) {
                    if self.#chunk_size == 0 {
                        return (0, Some(0));
                    }
                    let remaining = self.#fields_names_len.saturating_sub(self.#pos);
                    let count =
                        remaining / self.#chunk_size
                            + usize::from(remaining % self.#chunk_size != 0);
                    (count, Some(count))
                }

                #[inline]
                fn count(self) -> usize {
                    self.size_hint().0
                }
            }

            #[allow(dead_code)]
            impl<'a> ::core::iter::ExactSizeIterator for #chunks_mut_name<'a> {}

            /// An iterator over non-overlapping mutable exact chunks of a SoA slice.
            #[allow(missing_debug_implementations)]
            #visibility struct #chunks_exact_mut_name<'a> {
                #( #fields_names: #chunks_mut_ptr_fields_types, )*
                #fields_names_len: usize,
                #chunk_size: usize,
                #pos: usize,
                #end_field: usize,
                _marker: ::core::marker::PhantomData<&'a mut #name>,
            }

            #[allow(dead_code)]
            impl<'a> Iterator for #chunks_exact_mut_name<'a> {
                type Item = #slice_mut_name<'a>;

                #[inline]
                fn next(&mut self) -> Option<#slice_mut_name<'a>> {
                    if self.#pos >= self.#end_field || self.#chunk_size == 0 {
                        return None;
                    }
                    let result = #slice_mut_name {
                        #( #fields_names: #chunks_exact_mut_next_fields, )*
                    };
                    self.#pos += self.#chunk_size;
                    Some(result)
                }

                #[inline]
                fn size_hint(&self) -> (usize, Option<usize>) {
                    if self.#chunk_size == 0 {
                        return (0, Some(0));
                    }
                    let remaining = self.#end_field.saturating_sub(self.#pos);
                    let count = remaining / self.#chunk_size;
                    (count, Some(count))
                }

                #[inline]
                fn count(self) -> usize {
                    self.size_hint().0
                }
            }

            #[allow(dead_code)]
            impl<'a> #chunks_exact_mut_name<'a> {
                /// Returns the remainder of the original slice not yielded by the iterator.
                pub fn into_remainder(self) -> #slice_mut_name<'a> {
                    let rem_start = self.#end_field.min(self.#fields_names_len);
                    let rem_len = self.#fields_names_len - rem_start;
                    #slice_mut_name {
                        #( #fields_names: #chunks_exact_mut_remainder_fields, )*
                    }
                }
            }

            #[allow(dead_code)]
            impl<'a> #slice_mut_name<'a> {
                /// Similar to [`&mut
                #[doc = #slice_name_str]
                /// ::chunks_mut()`](https://doc.rust-lang.org/std/primitive.slice.html#method.chunks_mut).
                pub fn chunks_mut<'b>(&'b mut self, chunk_size: usize) -> #chunks_mut_name<'b>
                where
                    'a: 'b,
                {
                    assert!(chunk_size != 0, "chunk size must be non-zero");
                    let #fields_names_len = self.len();
                    #chunks_mut_name {
                        #( #fields_names: self.#fields_names.as_mut_ptr(), )*
                        #fields_names_len,
                        #chunk_size: chunk_size,
                        #pos: 0,
                        _marker: ::core::marker::PhantomData,
                    }
                }

                /// Similar to [`&mut
                #[doc = #slice_name_str]
                /// ::chunks_exact_mut()`](https://doc.rust-lang.org/std/primitive.slice.html#method.chunks_exact_mut).
                pub fn chunks_exact_mut<'b>(&'b mut self, chunk_size: usize) -> #chunks_exact_mut_name<'b>
                where
                    'a: 'b,
                {
                    assert!(chunk_size != 0, "chunk size must be non-zero");
                    let #fields_names_len = self.len();
                    let rem = #fields_names_len % chunk_size;
                    let end = #fields_names_len - rem;
                    #chunks_exact_mut_name {
                        #( #fields_names: self.#fields_names.as_mut_ptr(), )*
                        #fields_names_len,
                        #chunk_size: chunk_size,
                        #pos: 0,
                        #end_field: end,
                        _marker: ::core::marker::PhantomData,
                    }
                }
            }
        });

    if input.attrs.derive_clone {
        generated.append_all(quote! {
            #[allow(dead_code)]
            impl<'a> #slice_mut_name<'a> {
                /// Similar to [`&
                #[doc = #slice_name_str]
                /// ::to_vec()`](https://doc.rust-lang.org/std/primitive.slice.html#method.to_vec).
                pub fn to_vec(&self) -> #vec_name {
                    #vec_name {
                        #(#fields_names: #slice_to_vec_fields,)*
                    }
                }
            }
        });

        generated.append_all(quote! {
            impl<'a> ::layout::ToSoAVec<#name> for #slice_mut_name<'a> {
                type SoAVecType = #vec_name;

                fn to_vec(&self) -> Self::SoAVecType {
                    self.to_vec()
                }
            }
        });
    }

    return generated;
}
