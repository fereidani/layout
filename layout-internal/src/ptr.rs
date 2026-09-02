use proc_macro2::TokenStream;
use quote::quote;

use crate::{input::Input, names};

pub fn derive(input: &Input) -> TokenStream {
    let name = &input.name;
    let visibility = &input.visibility;
    let attrs = &input.attrs.ptr;
    let mut_attrs = &input.attrs.ptr_mut;
    let vec_name = names::vec_name(&input.name);
    let ptr_name = names::ptr_name(&input.name);
    let ptr_mut_name = names::ptr_mut_name(&input.name);
    let ref_name = names::ref_name(&input.name);
    let ref_mut_name = names::ref_mut_name(&input.name);

    let doc_url = format!("[`{0}`](struct.{0}.html)", name);
    let vec_doc_url = format!("[`{0}`](struct.{0}.html)", vec_name);
    let ptr_doc_url = format!("[`{0}`](struct.{0}.html)", ptr_name);
    let ptr_mut_doc_url = format!("[`{0}`](struct.{0}.html)", ptr_mut_name);
    let ref_doc_url = format!("[`{0}`](struct.{0}.html)", ref_name);
    let ref_mut_doc_url = format!("[`{0}`](struct.{0}.html)", ref_mut_name);

    let fields_names = &input.field_idents();

    let ptr_fields_types = input
        .map_fields_nested_or(
            |_, field_type, compact| names::nested_ptr_ty(field_type, compact),
            |_, field_type| quote! { *const #field_type },
        )
        .collect::<Vec<_>>();

    let ptr_mut_fields_types = input
        .map_fields_nested_or(
            |_, field_type, compact| {
                names::nested_ptr_mut_ty(field_type, compact)
            },
            |_, field_type| quote! { *mut #field_type },
        )
        .collect::<Vec<_>>();

    let as_ptr = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.as_ptr() },
            |ident, _| quote! { self.#ident as *const _ },
        )
        .collect::<Vec<_>>();

    let as_mut_ptr = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.as_mut_ptr() },
            |ident, _| quote! { self.#ident as *mut _ },
        )
        .collect::<Vec<_>>();

    // The Ref construction sites below use trailing-comma field lists, so the
    // marker init carries no leading comma. Only non-empty for all-compact
    // structs (whose `Ref<'a>` needs a PhantomData marker to use its lifetime).
    let ref_marker_init = input.ref_marker_init(false);

    // Row operations on the column pointers, for the generated `retain`.
    // Plain columns are raw `*mut T` and use the pointer primitives
    // directly; compact and nested columns forward to the same-named
    // helpers on their own pointer type.
    let row_unchecked = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.__row_unchecked(i) },
            |ident, _| quote! { &*self.#ident.add(i) },
        )
        .collect::<Vec<_>>();

    let row_mut_unchecked = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.__row_mut_unchecked(i) },
            |ident, _| quote! { &mut *self.#ident.add(i) },
        )
        .collect::<Vec<_>>();

    let move_row = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.__move_row(src, dst) },
            |ident, _| quote! { ::core::ptr::copy_nonoverlapping(self.#ident.add(src), self.#ident.add(dst), 1) },
        )
        .collect::<Vec<_>>();

    let drop_row = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.__drop_row(i) },
            |ident, _| quote! { ::core::ptr::drop_in_place(self.#ident.add(i)) },
        )
        .collect::<Vec<_>>();

    let shift_rows = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.__shift_rows(src, dst, count) },
            |ident, _| quote! { ::core::ptr::copy(self.#ident.add(src), self.#ident.add(dst), count) },
        )
        .collect::<Vec<_>>();

    quote! {
        /// An analog of a pointer to
        #[doc = #doc_url]
        /// with struct of array layout.
        #(#[#attrs])*
        #[derive(Copy, Clone)]
        #visibility struct #ptr_name {
            #(
                /// pointer to the `
                #[doc = stringify!(#fields_names)]
                ///` field of a single
                #[doc = #doc_url]
                /// inside a
                #[doc = #vec_doc_url]
                pub #fields_names: #ptr_fields_types,
            )*
        }

        /// An analog of a mutable pointer to
        #[doc = #doc_url]
        /// with struct of array layout.
        #(#[#mut_attrs])*
        #[derive(Copy, Clone)]
        #visibility struct #ptr_mut_name {
            #(
                /// pointer to the `
                #[doc = stringify!(#fields_names)]
                ///` field of a single
                #[doc = #doc_url]
                /// inside a
                #[doc = #vec_doc_url]
                pub #fields_names: #ptr_mut_fields_types,
            )*
        }

        #[allow(dead_code)]
        impl #ptr_name {
            /// Convert a
            #[doc = #ptr_doc_url]
            /// to a
            #[doc = #ptr_mut_doc_url]
            /// ; *i.e.* do a `*const T as *mut T` transformation.
            #visibility fn as_mut_ptr(&self) -> #ptr_mut_name {
                #ptr_mut_name {
                    #( #fields_names: #as_mut_ptr, )*
                }
            }

            /// Similar to [`*const T::is_null()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.is_null).
            pub fn is_null(self) -> bool {
                false #( || self.#fields_names.is_null())*
            }

            /// Similar to [`*const T::as_ref()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.as_ref),
            /// with the same safety caveats.
            pub unsafe fn as_ref<'a>(self) -> Option<#ref_name<'a>> {
                if ::layout::branches::unlikely(self.is_null()) {
                    None
                } else {
                    Some(#ref_name {
                        #(#fields_names: self.#fields_names.as_ref().expect("should not be null"), )*
                        #ref_marker_init
                    })
                }
            }

            /// Similar to [`*const T::offset()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.offset),
            /// with the same safety caveats.
            pub unsafe fn offset(self, count: isize) -> #ptr_name {
                #ptr_name {
                    #(#fields_names: self.#fields_names.offset(count), )*
                }
            }

            /// Similar to [`*const T::add()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.add),
            /// with the same safety caveats.
            pub unsafe fn add(self, count: usize) -> #ptr_name {
                #ptr_name {
                    #(#fields_names: self.#fields_names.add(count), )*
                }
            }

            /// Similar to [`*const T::sub()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.sub),
            /// with the same safety caveats.
            pub unsafe fn sub(self, count: usize) -> #ptr_name {
                #ptr_name {
                    #(#fields_names: self.#fields_names.sub(count), )*
                }
            }

            /// Similar to [`*const T::read()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.read),
            /// with the same safety caveats.
            pub unsafe fn read(self) -> #name {
                #name {
                    #(#fields_names: self.#fields_names.read(), )*
                }
            }
        }

        impl ::layout::SoAPointers for #name {
            type Ptr = #ptr_name;
            type MutPtr = #ptr_mut_name;
        }

        #[allow(dead_code)]
        impl #ptr_mut_name {
            /// Convert a
            #[doc = #ptr_mut_doc_url]
            /// to a
            #[doc = #ptr_doc_url]
            /// ; *i.e.* do a `*mut T as *const T` transformation
            #visibility fn as_ptr(&self) -> #ptr_name {
                #ptr_name {
                    #( #fields_names: #as_ptr, )*
                }
            }

            /// Similar to [`*mut T::is_null()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.is_null).
            pub fn is_null(self) -> bool {
                false #( || self.#fields_names.is_null())*
            }

            /// Similar to [`*mut T::as_ref()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.as_ref),
            /// with the same safety caveats.
            pub unsafe fn as_ref<'a>(self) -> Option<#ref_name<'a>> {
                if ::layout::branches::unlikely(self.is_null()) {
                    None
                } else {
                    Some(#ref_name {
                        #(#fields_names: self.#fields_names.as_ref().expect("should not be null"), )*
                        #ref_marker_init
                    })
                }
            }

            /// Similar to [`*mut T::as_mut()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.as_mut),
            /// with the same safety caveats.
            pub unsafe fn as_mut<'a>(self) -> Option<#ref_mut_name<'a>> {
                if ::layout::branches::unlikely(self.is_null()) {
                    None
                } else {
                    Some(#ref_mut_name {
                        #(#fields_names: self.#fields_names.as_mut().expect("should not be null"), )*
                    })
                }
            }

            /// Similar to [`*mut T::offset()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.offset),
            /// with the same safety caveats.
            pub unsafe fn offset(self, count: isize) -> #ptr_mut_name {
                #ptr_mut_name {
                    #(#fields_names: self.#fields_names.offset(count), )*
                }
            }

            /// Similar to [`*mut T::add()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.add),
            /// with the same safety caveats.
            pub unsafe fn add(self, count: usize) -> #ptr_mut_name {
                #ptr_mut_name {
                    #(#fields_names: self.#fields_names.add(count), )*
                }
            }

            /// Similar to [`*mut T::sub()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.sub),
            /// with the same safety caveats.
            pub unsafe fn sub(self, count: usize) -> #ptr_mut_name {
                #ptr_mut_name {
                    #(#fields_names: self.#fields_names.sub(count), )*
                }
            }

            /// Similar to [`*mut T::read()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.read),
            /// with the same safety caveats.
            pub unsafe fn read(self) -> #name {
                #name {
                    #(#fields_names: self.#fields_names.read(), )*
                }
            }

            /// Similar to [`*mut T::write()`](https://doc.rust-lang.org/std/primitive.pointer.html#method.write),
            /// with the same safety caveats.
            pub unsafe fn write(self, val: #name) {
                // ManuallyDrop: fields are read out via ptr::read, so a mid-write unwind can't double-free them.
                let mut val = ::core::mem::ManuallyDrop::new(val);
                unsafe {
                    #(self.#fields_names.write(::core::ptr::read(&val.#fields_names));)*
                }
            }

            // Row operations for the generated `retain`, which compacts
            // every column through one set of column pointers read before
            // its loop. Do not use these methods directly.

            /// Borrow row `i` through these column pointers.
            ///
            /// # Safety
            ///
            /// Every column pointer must be valid for reads at `i` for `'a`.
            #[doc(hidden)]
            #[inline]
            pub unsafe fn __row_unchecked<'a>(self, i: usize) -> #ref_name<'a> {
                unsafe {
                    #ref_name {
                        #( #fields_names: #row_unchecked, )*
                        #ref_marker_init
                    }
                }
            }

            /// Mutably borrow row `i` through these column pointers.
            ///
            /// # Safety
            ///
            /// Every column pointer must be valid for reads and writes at
            /// `i` for `'a`, with no other live reference to that row.
            #[doc(hidden)]
            #[inline]
            pub unsafe fn __row_mut_unchecked<'a>(self, i: usize) -> #ref_mut_name<'a> {
                unsafe {
                    #ref_mut_name {
                        #( #fields_names: #row_mut_unchecked, )*
                    }
                }
            }

            /// Move row `src` into slot `dst` bitwise, leaving `src` as a
            /// moved-out hole.
            ///
            /// # Safety
            ///
            /// `src != dst`, row `src` must be initialized and slot `dst`
            /// must not hold a live row.
            #[doc(hidden)]
            #[inline]
            pub unsafe fn __move_row(self, src: usize, dst: usize) {
                unsafe {
                    #( #move_row; )*
                }
            }

            /// Drop row `i` in place, leaving a hole.
            ///
            /// # Safety
            ///
            /// Row `i` must be initialized and must not be used again.
            #[doc(hidden)]
            #[inline]
            pub unsafe fn __drop_row(self, i: usize) {
                unsafe {
                    #( #drop_row; )*
                }
            }

            /// Move `count` rows from `src` to `dst` (the ranges may
            /// overlap), leaving the vacated slots as holes.
            ///
            /// # Safety
            ///
            /// Rows `src..src + count` must be initialized and the
            /// destination range must lie within every column's buffer.
            #[doc(hidden)]
            pub unsafe fn __shift_rows(self, src: usize, dst: usize, count: usize) {
                unsafe {
                    #( #shift_rows; )*
                }
            }
        }

        #[allow(dead_code)]
        impl<'a> #ref_name<'a> {
            /// Convert a
            #[doc = #ref_doc_url]
            /// to a
            #[doc = #ptr_doc_url]
            /// ; *i.e.* do a `&T as *const T` transformation
            #visibility fn as_ptr(&self) -> #ptr_name {
                #ptr_name {
                    #( #fields_names: #as_ptr, )*
                }
            }
        }

        #[allow(dead_code)]
        impl<'a> #ref_mut_name<'a> {
            /// Convert a
            #[doc = #ref_mut_doc_url]
            /// to a
            #[doc = #ptr_doc_url]
            /// ; *i.e.* do a `&mut T as *const T` transformation
            #visibility fn as_ptr(&self) -> #ptr_name {
                #ptr_name {
                    #( #fields_names: #as_ptr, )*
                }
            }

            /// Convert a
            #[doc = #ref_mut_doc_url]
            /// to a
            #[doc = #ptr_mut_doc_url]
            /// ; *i.e.* do a `&mut T as *mut T` transformation
            #visibility fn as_mut_ptr(&mut self) -> #ptr_mut_name {
                #ptr_mut_name {
                    #( #fields_names: #as_mut_ptr, )*
                }
            }
        }
    }
}
