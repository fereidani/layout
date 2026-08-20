use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, TokenStreamExt};

use crate::{input::Input, names};

pub fn derive(input: &Input) -> TokenStream {
    let vec_name = names::vec_name(&input.name);
    let slice_name = names::slice_name(&input.name);
    let slice_mut_name = names::slice_mut_name(&input.name);
    let ref_name = names::ref_name(&input.name);
    let ref_mut_name = names::ref_mut_name(&input.name);

    let fields_names = &input.field_idents();
    let first_field_name = &fields_names[0];

    let get_unchecked = input
        .map_fields_nested_or(
            |ident, _, _| quote! { ::layout::SoAIndex::get_unchecked(self.clone(), slice.#ident) },
            |ident, _| quote! { slice.#ident.get_unchecked(self.clone()) },
        )
        .collect::<Vec<_>>();

    let get_unchecked_mut = input.map_fields_nested_or(
        |ident, _, _| quote! { ::layout::SoAIndexMut::get_unchecked_mut(self.clone(), slice.#ident) },
        |ident, _| quote! { slice.#ident.get_unchecked_mut(self.clone()) },
    ).collect::<Vec<_>>();

    let index = input
        .map_fields_nested_or(
            |ident, _, _| quote! { ::layout::SoAIndex::index(self.clone(), slice.#ident) },
            |ident, _| quote! { & slice.#ident[self.clone()] },
        )
        .collect::<Vec<_>>();

    let index_mut = input
        .map_fields_nested_or(
            |ident, _, _| quote! { ::layout::SoAIndexMut::index_mut(self.clone(), slice.#ident) },
            |ident, _| quote! { &mut slice.#ident[self.clone()] },
        )
        .collect::<Vec<_>>();

    // The Ref construction sites below use trailing-comma field lists, so the
    // marker init carries no leading comma. Only non-empty for all-compact
    // structs (whose `Ref<'a>` needs a PhantomData marker to use its lifetime).
    let ref_marker_init = input.ref_marker_init(false);

    // The `Vec` and slice receivers differ only in how a range index reaches
    // the columns, so the forwarding families are generated once per target.
    let vec_target = Target {
        recv: Ident::new("soa", Span::call_site()),
        ty: quote! { &'a #vec_name },
        ty_mut: quote! { &'a mut #vec_name },
        slice: quote! { #slice_name<'a> },
        slice_mut: quote! { #slice_mut_name<'a> },
    };
    let slice_target = Target {
        recv: Ident::new("slice", Span::call_site()),
        ty: quote! { #slice_name<'a> },
        ty_mut: quote! { #slice_mut_name<'a> },
        slice: quote! { #slice_name<'a> },
        slice_mut: quote! { #slice_mut_name<'a> },
    };
    let vec_delegating = vec_target.delegating_ranges();
    let slice_delegating = slice_target.delegating_ranges();

    quote! {
        // usize
        impl<'a> ::layout::SoAIndex<&'a #vec_name> for usize {
            type RefOutput = #ref_name<'a>;

            #[inline]
            fn get(self, soa: &'a #vec_name) -> Option<Self::RefOutput> {
                if ::layout::branches::likely(self < soa.len()) {
                    Some(unsafe { ::layout::SoAIndex::get_unchecked(self, soa) })
                } else {
                    None
                }
            }

            #[inline]
            unsafe fn get_unchecked(self, soa: &'a #vec_name) -> Self::RefOutput {
                ::layout::SoAIndex::get_unchecked(self, soa.as_slice())
            }

            #[inline]
            fn index(self, soa: &'a #vec_name) -> Self::RefOutput {
                ::layout::SoAIndex::index(self, soa.as_slice())
            }
        }

        impl<'a> ::layout::SoAIndexMut<&'a mut #vec_name> for usize {
            type MutOutput = #ref_mut_name<'a>;

            #[inline]
            fn get_mut(self, soa: &'a mut #vec_name) -> Option<Self::MutOutput> {
                if ::layout::branches::likely(self < soa.len()) {
                    Some(unsafe { ::layout::SoAIndexMut::get_unchecked_mut(self, soa) })
                } else {
                    None
                }
            }

            #[inline]
            unsafe fn get_unchecked_mut(self, soa: &'a mut #vec_name) -> Self::MutOutput {
                ::layout::SoAIndexMut::get_unchecked_mut(self, soa.as_mut_slice())
            }

            #[inline]
            fn index_mut(self, soa: &'a mut #vec_name) -> Self::MutOutput {
                ::layout::SoAIndexMut::index_mut(self, soa.as_mut_slice())
            }
        }

        // Range<usize>
        impl<'a> ::layout::SoAIndex<&'a #vec_name> for ::core::ops::Range<usize> {
            type RefOutput = #slice_name<'a>;

            #[inline]
            fn get(self, soa: &'a #vec_name) -> Option<Self::RefOutput> {
                if self.start <= self.end && self.end <= soa.len() {
                    unsafe { Some(::layout::SoAIndex::get_unchecked(self, soa)) }
                } else {
                    None
                }
            }

            #[inline]
            unsafe fn get_unchecked(self, soa: &'a #vec_name) -> Self::RefOutput {
                ::layout::SoAIndex::get_unchecked(self, soa.as_slice())
            }

            #[inline]
            fn index(self, soa: &'a #vec_name) -> Self::RefOutput {
                ::layout::SoAIndex::index(self, soa.as_slice())
            }
        }

        impl<'a> ::layout::SoAIndexMut<&'a mut #vec_name> for ::core::ops::Range<usize> {
            type MutOutput = #slice_mut_name<'a>;

            #[inline]
            fn get_mut(self, soa: &'a mut #vec_name) -> Option<Self::MutOutput> {
                if self.start <= self.end && self.end <= soa.len() {
                    unsafe { Some(::layout::SoAIndexMut::get_unchecked_mut(self, soa)) }
                } else {
                    None
                }
            }

            #[inline]
            unsafe fn get_unchecked_mut(self, soa: &'a mut #vec_name) -> Self::MutOutput {
                ::layout::SoAIndexMut::get_unchecked_mut(self, soa.as_mut_slice())
            }

            #[inline]
            fn index_mut(self, soa: &'a mut #vec_name) -> Self::MutOutput {
               ::layout::SoAIndexMut::index_mut(self, soa.as_mut_slice())
            }
        }
        // RangeFull
        impl<'a> ::layout::SoAIndex<&'a #vec_name> for ::core::ops::RangeFull {
            type RefOutput = #slice_name<'a>;

            #[inline]
            fn get(self, soa: &'a #vec_name) -> Option<Self::RefOutput> {
                Some(soa.as_slice())
            }

            #[inline]
            unsafe fn get_unchecked(self, soa: &'a #vec_name) -> Self::RefOutput {
                soa.as_slice()
            }

            #[inline]
            fn index(self, soa: &'a #vec_name) -> Self::RefOutput {
                soa.as_slice()
            }
        }

        impl<'a> ::layout::SoAIndexMut<&'a mut #vec_name> for ::core::ops::RangeFull {
            type MutOutput = #slice_mut_name<'a>;

            #[inline]
            fn get_mut(self, soa: &'a mut #vec_name) -> Option<Self::MutOutput> {
                Some(soa.as_mut_slice())
            }

            #[inline]
            unsafe fn get_unchecked_mut(self, soa: &'a mut #vec_name) -> Self::MutOutput {
                soa.as_mut_slice()
            }

            #[inline]
            fn index_mut(self, soa: &'a mut #vec_name) -> Self::MutOutput {
                soa.as_mut_slice()
            }
        }
        #vec_delegating

        // usize
        impl<'a> ::layout::SoAIndex<#slice_name<'a>> for usize {
            type RefOutput = #ref_name<'a>;

            #[inline]
            fn get(self, slice: #slice_name<'a>) -> Option<Self::RefOutput> {
                if self < slice.#first_field_name.len() {
                    Some(unsafe { ::layout::SoAIndex::get_unchecked(self, slice) })
                } else {
                    None
                }
            }

            #[inline]
            unsafe fn get_unchecked(self, slice: #slice_name<'a>) -> Self::RefOutput {
                #ref_name {
                    #( #fields_names: #get_unchecked, )*
                    #ref_marker_init
                }
            }

            #[inline]
            fn index(self, slice: #slice_name<'a>) -> Self::RefOutput {
                #ref_name {
                    #( #fields_names: #index, )*
                    #ref_marker_init
                }
            }
        }

        impl<'a> ::layout::SoAIndexMut<#slice_mut_name<'a>> for usize {
            type MutOutput = #ref_mut_name<'a>;

            #[inline]
            fn get_mut(self, slice: #slice_mut_name<'a>) -> Option<Self::MutOutput> {
                if self < slice.len() {
                    Some(unsafe { ::layout::SoAIndexMut::get_unchecked_mut(self, slice) })
                } else {
                    None
                }
            }

            #[inline]
            unsafe fn get_unchecked_mut(self, slice: #slice_mut_name<'a>) -> Self::MutOutput {
                #ref_mut_name {
                    #( #fields_names: #get_unchecked_mut, )*
                }
            }

            #[inline]
            fn index_mut(self, slice: #slice_mut_name<'a>) -> Self::MutOutput {
                #ref_mut_name {
                    #( #fields_names: #index_mut, )*
                }
            }
        }



        // Range<usize>
        impl<'a> ::layout::SoAIndex<#slice_name<'a>> for ::core::ops::Range<usize> {
            type RefOutput = #slice_name<'a>;

            #[inline]
            fn get(self, slice: #slice_name<'a>) -> Option<Self::RefOutput> {
                if self.start <= self.end && self.end <= slice.#first_field_name.len() {
                    unsafe { Some(::layout::SoAIndex::get_unchecked(self, slice)) }
                } else {
                    None
                }
            }

            #[inline]
            unsafe fn get_unchecked(self, slice: #slice_name<'a>) -> Self::RefOutput {
                #slice_name {
                    #( #fields_names: #get_unchecked, )*
                }
            }

            #[inline]
            fn index(self, slice: #slice_name<'a>) -> Self::RefOutput {
                #slice_name {
                    #( #fields_names: #index, )*
                }
            }
        }

        impl<'a> ::layout::SoAIndexMut<#slice_mut_name<'a>> for ::core::ops::Range<usize> {
            type MutOutput = #slice_mut_name<'a>;

            #[inline]
            fn get_mut(self, slice: #slice_mut_name<'a>) -> Option<Self::MutOutput> {
                if self.start <= self.end && self.end <= slice.#first_field_name.len() {
                    unsafe { Some(::layout::SoAIndexMut::get_unchecked_mut(self, slice)) }
                } else {
                    None
                }
            }

            #[inline]
            unsafe fn get_unchecked_mut(self, slice: #slice_mut_name<'a>) -> Self::MutOutput {
                #slice_mut_name {
                    #( #fields_names: #get_unchecked_mut, )*
                }
            }

            #[inline]
            fn index_mut(self, slice: #slice_mut_name<'a>) -> Self::MutOutput {
                #slice_mut_name {
                    #( #fields_names: #index_mut, )*
                }
            }
        }


        // RangeFull
        impl<'a> ::layout::SoAIndex<#slice_name<'a>> for ::core::ops::RangeFull {
            type RefOutput = #slice_name<'a>;

            #[inline]
            fn get(self, slice: #slice_name<'a>) -> Option<Self::RefOutput> {
                Some(slice)
            }

            #[inline]
            unsafe fn get_unchecked(self, slice: #slice_name<'a>) -> Self::RefOutput {
                slice
            }

            #[inline]
            fn index(self, slice: #slice_name<'a>) -> Self::RefOutput {
                slice
            }
        }

        impl<'a> ::layout::SoAIndexMut<#slice_mut_name<'a>> for ::core::ops::RangeFull {
            type MutOutput = #slice_mut_name<'a>;

            #[inline]
            fn get_mut(self, slice: #slice_mut_name<'a>) -> Option<Self::MutOutput> {
                Some(slice)
            }

            #[inline]
            unsafe fn get_unchecked_mut(self, slice: #slice_mut_name<'a>) -> Self::MutOutput {
                slice
            }

            #[inline]
            fn index_mut(self, slice: #slice_mut_name<'a>) -> Self::MutOutput {
                slice
            }
        }

        #slice_delegating
    }
}
/// One indexing receiver: the `SoAIndex` / `SoAIndexMut` impl target and the
/// slice types a range index yields for it.
struct Target {
    /// Name the generated method bodies bind the receiver to.
    recv: Ident,
    /// Receiver type of the immutable impls.
    ty: TokenStream,
    /// Receiver type of the mutable impls.
    ty_mut: TokenStream,
    /// `SoAIndex::RefOutput` of a range index.
    slice: TokenStream,
    /// `SoAIndexMut::MutOutput` of a range index.
    slice_mut: TokenStream,
}

impl Target {
    /// Emit the `SoAIndex` / `SoAIndexMut` pair for a range type whose three
    /// methods all forward to the equivalent `range` (an expression in terms
    /// of `self` and the receiver binding). When `none_if` is given, `get`
    /// and `get_mut` return `None` on that condition instead of forwarding.
    fn delegating(
        &self,
        index_ty: &TokenStream,
        range: &TokenStream,
        none_if: Option<&TokenStream>,
    ) -> TokenStream {
        let Target {
            recv,
            ty,
            ty_mut,
            slice,
            slice_mut,
        } = self;
        let (get, get_mut) = match none_if {
            Some(cond) => (
                quote! {
                    if #cond {
                        None
                    } else {
                        ::layout::SoAIndex::get(#range, #recv)
                    }
                },
                quote! {
                    if #cond {
                        None
                    } else {
                        ::layout::SoAIndexMut::get_mut(#range, #recv)
                    }
                },
            ),
            None => (
                quote! { ::layout::SoAIndex::get(#range, #recv) },
                quote! { ::layout::SoAIndexMut::get_mut(#range, #recv) },
            ),
        };
        quote! {
            impl<'a> ::layout::SoAIndex<#ty> for #index_ty {
                type RefOutput = #slice;

                #[inline]
                fn get(self, #recv: #ty) -> Option<Self::RefOutput> {
                    #get
                }

                #[inline]
                unsafe fn get_unchecked(self, #recv: #ty) -> Self::RefOutput {
                    ::layout::SoAIndex::get_unchecked(#range, #recv)
                }

                #[inline]
                fn index(self, #recv: #ty) -> Self::RefOutput {
                    ::layout::SoAIndex::index(#range, #recv)
                }
            }

            impl<'a> ::layout::SoAIndexMut<#ty_mut> for #index_ty {
                type MutOutput = #slice_mut;

                #[inline]
                fn get_mut(self, #recv: #ty_mut) -> Option<Self::MutOutput> {
                    #get_mut
                }

                #[inline]
                unsafe fn get_unchecked_mut(
                    self,
                    #recv: #ty_mut,
                ) -> Self::MutOutput {
                    ::layout::SoAIndexMut::get_unchecked_mut(#range, #recv)
                }

                #[inline]
                fn index_mut(self, #recv: #ty_mut) -> Self::MutOutput {
                    ::layout::SoAIndexMut::index_mut(#range, #recv)
                }
            }
        }
    }

    /// Emit every range family that forwards to an equivalent `Range<usize>`
    /// (`RangeToInclusive` forwards to `RangeInclusive`, which normalizes it).
    /// `Range` itself, `usize` and `RangeFull` do the real work and are
    /// written out by the caller.
    fn delegating_ranges(&self) -> TokenStream {
        let recv = &self.recv;
        let families = [
            (
                quote! { ::core::ops::RangeTo<usize> },
                quote! { 0..self.end },
                None,
            ),
            (
                quote! { ::core::ops::RangeFrom<usize> },
                quote! { self.start..#recv.len() },
                None,
            ),
            (
                quote! { ::core::ops::RangeInclusive<usize> },
                quote! { *self.start()..self.end().saturating_add(1) },
                // `usize::MAX` has no exclusive-end equivalent.
                Some(quote! { *self.end() == usize::MAX }),
            ),
            (
                quote! { ::core::ops::RangeToInclusive<usize> },
                quote! { 0..=self.end },
                None,
            ),
        ];
        let mut generated = TokenStream::new();
        for (index_ty, range, none_if) in &families {
            generated.append_all(self.delegating(
                index_ty,
                range,
                none_if.as_ref(),
            ));
        }
        generated
    }
}
