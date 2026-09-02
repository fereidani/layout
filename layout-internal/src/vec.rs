use proc_macro2::TokenStream;
use quote::{quote, TokenStreamExt};

use crate::{input::Input, names};
extern crate alloc;
use alloc::{format, vec::Vec};

pub fn derive(input: &Input) -> TokenStream {
    let name = &input.name;
    let vec_name_str = format!("Vec<{}>", name);
    let attrs = &input.attrs.vec;
    let visibility = &input.visibility;
    let vec_name = names::vec_name(&input.name);
    let slice_name = names::slice_name(name);
    let slice_mut_name = names::slice_mut_name(&input.name);
    let ref_name = names::ref_name(&input.name);
    let ref_mut_name = names::ref_mut_name(&input.name);
    let ptr_name = names::ptr_name(&input.name);
    let ptr_mut_name = names::ptr_mut_name(&input.name);
    let drain_name = names::drain_name(&input.name);

    let doc_url = format!("[`{0}`](struct.{0}.html)", input.name);

    let fields_names = &input.field_idents();

    let fields_names_hygienic = input.hygienic_idents("");

    let first_field = &fields_names[0];

    let vec_fields_types = input
        .map_fields_nested_or(
            |_, field_type, compact| names::nested_vec_ty(field_type, compact),
            |_, field_type| quote! { ::layout::Column<#field_type> },
        )
        .collect::<Vec<_>>();

    let vec_with_capacity = input
        .map_fields_nested_or(
            |_, field_type, _| quote! { <#field_type as SOA>::Type::with_capacity(capacity) },
            |_, _| quote! { ::layout::Column::with_capacity(capacity) },
        )
        .collect::<Vec<_>>();

    let vec_slice = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.slice(range.clone()) },
            |ident, _| quote! { &self.#ident[range.clone()] },
        )
        .collect::<Vec<_>>();

    let vec_slice_mut = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.slice_mut(range.clone()) },
            |ident, _| quote! { &mut self.#ident[range.clone()] },
        )
        .collect::<Vec<_>>();

    let vec_from_raw_parts = input
        .map_fields_nested_or(
            |ident, field_type, compact| {
                let vec_type = names::nested_vec_ty(field_type, compact);
                if compact.is_some() {
                    quote! { <#vec_type>::from_raw_parts(data.#ident) }
                } else {
                    quote! { <#vec_type>::from_raw_parts(data.#ident, len, capacity) }
                }
            },
            |ident, _| quote! { ::layout::Column::from_raw_parts(data.#ident, len, capacity) },
        )
        .collect::<Vec<_>>();

    let vec_replace = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.replace(index, field) },
            |ident, _| quote! { ::core::mem::replace(&mut self.#ident[index], field) },
        )
        .collect::<Vec<_>>();

    let drain_fields_types = input
        .map_fields_nested_or(
            |_, field_type, compact| {
                names::nested_drain_ty(field_type, compact)
            },
            |_, field_type| quote! { ::layout::Drain<'a, #field_type> },
        )
        .collect::<Vec<_>>();

    // `retain` and `retain_mut` share one compaction loop (`__retain_rows`);
    // they differ only in the row handle handed to the predicate.
    let retain_guard_name = quote::format_ident!("{}RetainGuard", name);

    let set_len_fields = input
        .map_fields_nested_or(
            |ident, _, _| quote! { self.#ident.__set_len(len) },
            |ident, _| quote! { self.#ident.set_len(len) },
        )
        .collect::<Vec<_>>();

    let mut generated = quote! {
        /// An analog to `
        #[doc = #vec_name_str]
        /// ` with Struct of Array (SoA) layout
        #[allow(dead_code)]
        #(#[#attrs])*
        #[derive(Default)]
        #visibility struct #vec_name {
            #(
                /// a vector of `
                #[doc = stringify!(#fields_names)]
                ///` from a
                #[doc = #doc_url]
                pub #fields_names: #vec_fields_types,
            )*
        }

        #[allow(dead_code)]
        impl #vec_name {
            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::new()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.new)
            pub fn new() -> #vec_name {
                Default::default()
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::with_capacity()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.with_capacity),
            /// initializing all fields with the given `capacity`.
            pub fn with_capacity(capacity: usize) -> #vec_name {
                #vec_name {
                    #( #fields_names: #vec_with_capacity, )*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::capacity()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.capacity),
            /// the minimum capacity across all fields. Compact columns have word-granular
            /// capacity, so per-field capacities may differ; this returns the most
            /// conservative (binding) value.
            pub fn capacity(&self) -> usize {
                // Structs always have >= 1 field, so the fold never returns MAX.
                let mut capacity = usize::MAX;
                #(capacity = capacity.min(self.#fields_names.capacity());)*
                capacity
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::reserve()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.reserve),
            /// reserving the same `additional` space for all fields.
            pub fn reserve(&mut self, additional: usize) {
                #(self.#fields_names.reserve(additional);)*
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::reserve_exact()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.reserve_exact)
            /// reserving the same `additional` space for all fields.
            pub fn reserve_exact(&mut self, additional: usize) {
                #(self.#fields_names.reserve_exact(additional);)*
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::shrink_to_fit()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.shrink_to_fit)
            /// shrinking all fields.
            pub fn shrink_to_fit(&mut self) {
                #(self.#fields_names.shrink_to_fit();)*
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::truncate()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.truncate)
            /// truncating all fields.
            pub fn truncate(&mut self, len: usize) {
                // SAFETY: every column is truncated to the same length.
                unsafe {
                    #(self.#fields_names.truncate(len);)*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::push()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.push).
            pub fn push(&mut self, value: #name) {
                // ManuallyDrop: fields are read out via ptr::read, so a mid-push unwind can't double-free them.
                let mut value = ::core::mem::ManuallyDrop::new(value);
                unsafe {
                    #(self.#fields_names.push(::core::ptr::read(&value.#fields_names));)*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::len()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.len),
            /// all the fields should have the same length.
            #[inline]
            pub fn len(&self) -> usize {
                let len = self.#first_field.len();
                #(debug_assert_eq!(self.#fields_names.len(), len);)*
                len
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::is_empty()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.is_empty),
            /// all the fields should have the same length.
            #[inline]
            pub fn is_empty(&self) -> bool {
                let empty = self.#first_field.is_empty();
                #(debug_assert_eq!(self.#fields_names.is_empty(), empty);)*
                empty
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::swap_remove()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.swap_remove).
            pub fn swap_remove(&mut self, index: usize) -> #name {
                // SAFETY: the same index is swap-removed from every column.
                #(
                    let #fields_names_hygienic =
                        unsafe { self.#fields_names.swap_remove(index) };
                )*
                #name{#(#fields_names: #fields_names_hygienic),*}
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::insert()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.insert).
            pub fn insert(&mut self, index: usize, element: #name) {
                let len = self.len();
                if ::layout::branches::unlikely(index > len) {
                    ::layout::panics::insert_index_fail(index, len);
                }

                // ManuallyDrop: see `push` — a mid-insert unwind can't double-free read-out fields.
                let mut element = ::core::mem::ManuallyDrop::new(element);
                unsafe {
                    #(self.#fields_names.insert(index, ::core::ptr::read(&element.#fields_names));)*
                }
            }

            /// Similar to [`core::mem::replace()`](https://doc.rust-lang.org/std/mem/fn.replace.html).
            pub fn replace(&mut self, index: usize, element: #name) -> #name {
                let len = self.len();
                if ::layout::branches::unlikely(index >= len) {
                    ::layout::panics::index_out_of_bounds(index, len);
                }

                // ManuallyDrop: see `push` — a mid-replace unwind can't double-free read-out fields.
                let mut element = ::core::mem::ManuallyDrop::new(element);
                #(
                    let field = unsafe { ::core::ptr::read(&element.#fields_names) };
                    let #fields_names_hygienic = #vec_replace;
                )*

                #name{#(#fields_names: #fields_names_hygienic),*}
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::remove()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.remove).
            pub fn remove(&mut self, index: usize) -> #name {
                // SAFETY: the same index is removed from every column.
                #(
                    let #fields_names_hygienic =
                        unsafe { self.#fields_names.remove(index) };
                )*
                #name{#(#fields_names: #fields_names_hygienic),*}
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::pop()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.pop).
            pub fn pop(&mut self) -> Option<#name> {
                if ::layout::branches::unlikely(self.is_empty()) {
                    None
                } else {
                    // SAFETY: every column is popped once.
                    #(
                        let #fields_names_hygienic =
                            unsafe { self.#fields_names.pop().unwrap() };
                    )*
                    Some(#name{#(#fields_names: #fields_names_hygienic),*})
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::append()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.append).
            pub fn append(&mut self, other: &mut #vec_name) {
                // SAFETY: every column appends its sibling.
                unsafe {
                    #(
                        self.#fields_names.append(&mut other.#fields_names);
                    )*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::clear()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.clear).
            pub fn clear(&mut self) {
                // SAFETY: every column is cleared.
                unsafe {
                    #(self.#fields_names.clear();)*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::split_off()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.split_off).
            pub fn split_off(&mut self, at: usize) -> #vec_name {
                // SAFETY: every column splits at the same index.
                unsafe {
                    #vec_name {
                        #(#fields_names: self.#fields_names.split_off(at), )*
                    }
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::as_slice()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.as_slice).
            pub fn as_slice(&self) -> #slice_name {
                #slice_name {
                    #(#fields_names: self.#fields_names.as_slice(), )*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::as_mut_slice()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.as_mut_slice).
            pub fn as_mut_slice(&mut self) -> #slice_mut_name {
                #slice_mut_name {
                    #(#fields_names: self.#fields_names.as_mut_slice(), )*
                }
            }

            /// Create a slice of this vector matching the given `range`. This
            /// is analogous to `Index<Range<usize>>`.
            pub fn slice(&self, range: ::core::ops::Range<usize>) -> #slice_name {
                #slice_name {
                    #( #fields_names: #vec_slice, )*
                }
            }

            /// Create a mutable slice of this vector matching the given
            /// `range`. This is analogous to `IndexMut<Range<usize>>`.
            pub fn slice_mut(&mut self, range: ::core::ops::Range<usize>) -> #slice_mut_name {
                #slice_mut_name {
                    #( #fields_names: #vec_slice_mut, )*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::retain()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.retain).
            pub fn retain<F>(&mut self, mut f: F) where F: FnMut(#ref_name) -> bool {
                // SAFETY: `__retain_rows` only asks for rows below the length
                // that are still live.
                self.__retain_rows(|ptrs, i| f(unsafe { ptrs.__row_unchecked(i) }));
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::retain_mut()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.retain_mut).
            pub fn retain_mut<F>(&mut self, mut f: F) where F: FnMut(#ref_mut_name) -> bool {
                // SAFETY: as in `retain`; the row handle is dropped before
                // the loop touches that row again.
                self.__retain_rows(|ptrs, i| f(unsafe { ptrs.__row_mut_unchecked(i) }));
            }

            /// Shared body of `retain` and `retain_mut`: keep the rows for
            /// which `f` (given the column pointers and a row index) returns
            /// `true`, moving them towards the front.
            ///
            /// This is `Vec::retain_mut`'s write-index compaction, applied to
            /// every column at once:
            ///
            /// ```text
            /// rows: [Kept, Kept, Hole, Hole, Hole, Hole, Unchecked, Unchecked]
            ///       |            ^- write                ^- read             |
            ///       |<-              original_len                          ->|
            /// ```
            ///
            /// Kept rows before the first rejection are never written. A
            /// rejected row is dropped in place; a kept row after the first
            /// rejection is moved down into the hole with one copy per
            /// column; every column's length is set once at the end. If the
            /// predicate or a destructor panics inside the critical section,
            /// the guard shifts the unchecked rows down over the holes and
            /// sets the length, so no row is dropped twice or leaked.
            fn __retain_rows<F>(&mut self, mut f: F)
            where
                F: FnMut(&#ptr_mut_name, usize) -> bool,
            {
                let original_len = self.len();
                if original_len == 0 {
                    return;
                }
                // The guard takes the vector first: a compact column's
                // pointer addresses the column's store inside this struct,
                // so the pointers below must be derived after the last
                // unique reborrow of the vector, and the loops never touch
                // `g.vec` again. With `read == write == 0` the guard's drop
                // restores the original length unchanged.
                let mut g = #retain_guard_name {
                    vec: self,
                    read: 0,
                    write: 0,
                    original_len,
                };
                // Column base pointers, read once: nothing below grows a
                // column, so no buffer can move while they are in use.
                let ptrs = g.vec.as_mut_ptr();

                let mut read = 0;
                loop {
                    if ::layout::branches::unlikely(!f(&ptrs, read)) {
                        break;
                    }
                    read += 1;
                    if read == original_len {
                        // Every row is kept: nothing to move or drop.
                        ::core::mem::forget(g);
                        return;
                    }
                }

                // Critical section: at least one row is removed from here
                // on. `read` is advanced past the rejected row before it is
                // dropped, so a panicking destructor cannot make the guard
                // drop it again.
                g.write = read;
                g.read = read + 1;
                // SAFETY: `read < original_len`, and the row was rejected.
                unsafe { ptrs.__drop_row(read) };

                while g.read < g.original_len {
                    let cur = g.read;
                    if !f(&ptrs, cur) {
                        g.read += 1;
                        // SAFETY: row `cur` is live, was rejected, and is
                        // never touched again.
                        unsafe { ptrs.__drop_row(cur) };
                    } else {
                        // SAFETY: `read > write`, so the slots do not
                        // overlap; the source row is never touched again.
                        unsafe { ptrs.__move_row(cur, g.write) };
                        g.write += 1;
                        g.read += 1;
                    }
                }

                // Leaving the critical section without a panic: commit the
                // length and disarm the guard.
                // SAFETY: rows `[0, write)` are live and contiguous.
                unsafe { g.vec.__set_len(g.write) };
                ::core::mem::forget(g);
            }

            /// Set every column's length to `len` without touching any
            /// element. Do not use this method directly.
            ///
            /// # Safety
            ///
            /// `len` must not exceed any column's capacity, and the first
            /// `len` rows must be initialized.
            #[doc(hidden)]
            pub unsafe fn __set_len(&mut self, len: usize) {
                unsafe {
                    #( #set_len_fields; )*
                }
            }
        }

        /// Panic guard of `retain`: runs only when the predicate or a
        /// destructor unwinds inside the critical section. It shifts the
        /// unchecked rows down over the holes and sets the final length.
        #[doc(hidden)]
        struct #retain_guard_name<'a> {
            vec: &'a mut #vec_name,
            /// First unchecked row.
            read: usize,
            /// First hole: every row below it is kept.
            write: usize,
            original_len: usize,
        }

        impl Drop for #retain_guard_name<'_> {
            #[cold]
            fn drop(&mut self) {
                let remaining = self.original_len - self.read;
                // SAFETY: rows `[read, original_len)` are live and were
                // never touched, and `write <= read`, so the shift moves
                // live rows over holes only.
                unsafe {
                    self.vec.as_mut_ptr().__shift_rows(
                        self.read,
                        self.write,
                        remaining,
                    );
                }
                // SAFETY: the kept rows are now contiguous in
                // `[0, write + remaining)`.
                unsafe {
                    self.vec.__set_len(self.write + remaining);
                }
            }
        }

        #[allow(dead_code)]
        impl #vec_name {

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::get<I>()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.get).
            pub fn get<'a, I>(&'a self, index: I) -> Option<I::RefOutput>
            where
                I: ::layout::SoAIndex<&'a #vec_name>
            {
                index.get(self)
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::get_unchecked<I>()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.get_unchecked).
            pub unsafe fn get_unchecked<'a, I>(&'a self, index: I) -> I::RefOutput
            where
                I: ::layout::SoAIndex<&'a #vec_name>
            {
                index.get_unchecked(self)
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::index<I>()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.index).
            pub fn index<'a, I>(&'a self, index: I) -> I::RefOutput
            where
                I: ::layout::SoAIndex<&'a #vec_name>
            {
                index.index(self)
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::get_mut<I>()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.get_mut).
            pub fn get_mut<'a, I>(&'a mut self, index: I) -> Option<I::MutOutput>
            where
                I: ::layout::SoAIndexMut<&'a mut #vec_name>
            {
                index.get_mut(self)
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::get_unchecked_mut<I>()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.get_unchecked_mut).
            pub unsafe fn get_unchecked_mut<'a, I>(&'a mut self, index: I) -> I::MutOutput
            where
                I: ::layout::SoAIndexMut<&'a mut #vec_name>
            {
                index.get_unchecked_mut(self)
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::index_mut<I>()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.index_mut).
            pub fn index_mut<'a, I>(&'a mut self, index: I) -> I::MutOutput
            where
                I: ::layout::SoAIndexMut<&'a mut #vec_name>
            {
                index.index_mut(self)
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::as_ptr()`](https://doc.rust-lang.org/std/struct.Vec.html#method.as_ptr).
            pub fn as_ptr(&self) -> #ptr_name {
                #ptr_name {
                    #(#fields_names: self.#fields_names.as_ptr(),)*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::as_mut_ptr()`](https://doc.rust-lang.org/std/struct.Vec.html#method.as_mut_ptr).
            pub fn as_mut_ptr(&mut self) -> #ptr_mut_name {
                #ptr_mut_name {
                    #(#fields_names: self.#fields_names.as_mut_ptr(),)*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::from_raw_parts()`](https://doc.rust-lang.org/std/struct.Vec.html#method.from_raw_parts).
            pub unsafe fn from_raw_parts(data: #ptr_mut_name, len: usize, capacity: usize) -> #vec_name {
                #vec_name {
                    #( #fields_names: #vec_from_raw_parts, )*
                }
            }

            /// Similar to [`
            #[doc = #vec_name_str]
            /// ::drain()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.drain).
            pub fn drain<R: ::core::ops::RangeBounds<usize> + Clone>(&mut self, range: R) -> #drain_name<'_> {
                // SAFETY: every column drains the same range.
                unsafe {
                    #drain_name {
                        #( #fields_names: self.#fields_names.drain(range.clone()), )*
                    }
                }
            }
        }

        /// A draining iterator for
        #[doc = #doc_url]
        /// inside a
        #[doc = #vec_name_str]
        /// .
        #[allow(missing_debug_implementations)]
        #visibility struct #drain_name<'a> {
            #(
                /// drain of `
                #[doc = stringify!(#fields_names)]
                ///` from a
                #[doc = #doc_url]
                pub #fields_names: #drain_fields_types,
            )*
        }

        #[allow(dead_code)]
        impl<'a> Iterator for #drain_name<'a> {
            type Item = #name;

            #[inline]
            fn next(&mut self) -> Option<#name> {
                #(
                    let #fields_names_hygienic = self.#fields_names.next()?;
                )*
                Some(#name{#(#fields_names: #fields_names_hygienic),*})
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                self.#first_field.size_hint()
            }
        }

        #[allow(dead_code)]
        impl<'a> DoubleEndedIterator for #drain_name<'a> {
            #[inline]
            fn next_back(&mut self) -> Option<#name> {
                #(
                    let #fields_names_hygienic = self.#fields_names.next_back()?;
                )*
                Some(#name{#(#fields_names: #fields_names_hygienic),*})
            }
        }

        #[allow(dead_code)]
        impl<'a> ::core::iter::ExactSizeIterator for #drain_name<'a> {}

        #[allow(clippy::drop_non_drop)]
        impl Drop for #vec_name {
            fn drop(&mut self) {
                if !::core::mem::needs_drop::<#name>() {
                    // Trivially droppable: the column vectors free their own
                    // buffers.
                    return;
                }
                // Drop in insertion order, matching `Vec<T>`. Draining moves
                // each row out of the columns so their own `Drop` reaps nothing.
                for _ in self.drain(..) {}
            }
        }
    };

    if input.attrs.derive_clone {
        generated.append_all(quote! {
            #[allow(dead_code)]
            impl #vec_name {
                /// Similar to [`
                #[doc = #vec_name_str]
                /// ::resize()`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.resize).
                pub fn resize(&mut self, new_len: usize, value: #name) {
                    // SAFETY: every column is resized to the same length.
                    unsafe {
                        #(
                            self.#fields_names.resize(new_len, value.#fields_names);
                        )*
                    }
                }
            }

            impl ::layout::SoAAppendVec<#name> for #vec_name {
                fn extend_from_slice(&mut self, other: Self::Slice<'_>) {
                    // SAFETY: every column extends from its sibling slice.
                    unsafe {
                        #(
                            self.#fields_names.extend_from_slice(other.#fields_names);
                        )*
                    }
                }
            }
        });
    }

    return generated;
}
