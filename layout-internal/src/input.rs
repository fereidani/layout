use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    punctuated::Punctuated, Attribute, Data, DeriveInput, Field, Meta,
    MetaList, Path, Token, Visibility,
};

/// Representing the struct we are deriving
pub struct Input {
    /// The input struct name
    pub name: syn::Ident,
    /// The list of fields in the struct
    pub fields: Vec<Field>,
    /// Is field marked with `#[nested_soa]`
    pub field_is_nested: Vec<bool>,
    /// For compact (`Compact<T>` / `CompactBool`) fields: the inner element
    /// type. `None` for non-compact and nested-SoA fields.
    pub field_is_compact: Vec<Option<syn::Type>>,
    /// The struct overall visibility
    pub visibility: Visibility,
    /// Additional attributes requested with `#[soa_attr(...)]` or
    /// `#[layout()]`
    pub attrs: ExtraAttributes,
}

pub struct ExtraAttributes {
    // did the user explicitly asked us to derive clone?
    pub derive_clone: bool,

    pub vec: Vec<Meta>,
    pub slice: Vec<Meta>,
    pub slice_mut: Vec<Meta>,
    pub ref_: Vec<Meta>,
    pub ref_mut: Vec<Meta>,
    pub ptr: Vec<Meta>,
    pub ptr_mut: Vec<Meta>,
}

impl ExtraAttributes {
    fn new() -> ExtraAttributes {
        ExtraAttributes {
            derive_clone: false,
            vec: Vec::new(),
            slice: Vec::new(),
            slice_mut: Vec::new(),
            ref_: Vec::new(),
            ref_mut: Vec::new(),
            ptr: Vec::new(),
            ptr_mut: Vec::new(),
        }
    }

    /// Add a single trait from `#[layout]`
    fn add_derive(&mut self, ident: &proc_macro2::Ident) {
        let derive_only_vec = |ident| {
            static EXCEPTIONS: &[&str] = &["Clone", "Deserialize", "Serialize"];
            for exception in EXCEPTIONS {
                if ident == exception {
                    return true;
                }
            }
            return false;
        };

        let derive = Meta::List(MetaList {
            path: Path::from(syn::Ident::new("derive", Span::call_site())),
            delimiter: syn::MacroDelimiter::Paren(syn::token::Paren(
                Span::call_site(),
            )),
            tokens: quote! { #ident },
        });

        if !derive_only_vec(ident) {
            self.slice.push(derive.clone());
            self.slice_mut.push(derive.clone());
            self.ref_.push(derive.clone());
            self.ref_mut.push(derive.clone());
            self.ptr.push(derive.clone());
            self.ptr_mut.push(derive.clone());
        }

        // always add this derive to the Vec struct
        self.vec.push(derive);

        if ident == "Clone" {
            self.derive_clone = true;
        }
    }
}

fn contains_nested_soa(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("nested_soa") {
            return true;
        }
    }
    return false;
}

/// If `ty` is a compact column marker (`Compact<T>` or the `CompactBool`
/// alias), return the inner element type; otherwise `None`.
///
/// Detection is by the path-segment NAME (`Compact` / `CompactBool`). A renamed
/// or re-exported import (e.g. `use layout::Compact as Packed; field:
/// Packed<bool>`) is therefore NOT recognized and the field silently falls back
/// to a plain `Vec`. This is a proc-macro limitation — without type resolution
/// the derive cannot map an arbitrary alias back to `Compact`. Users must keep
/// the import names `Compact` / `CompactBool` (fully-qualified
/// `::layout::Compact<T>` and `path::Compact<T>` still work, since the last
/// segment is still `Compact`).
fn compact_inner(ty: &syn::Type) -> Option<syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    let last = type_path.path.segments.last()?;
    if last.ident == "CompactBool" {
        return Some(syn::parse_quote!(bool));
    }
    if last.ident == "Compact" {
        if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
            if args.args.len() == 1 {
                if let syn::GenericArgument::Type(inner) = &args.args[0] {
                    return Some(inner.clone());
                }
            }
        }
        return None;
    }
    None
}

impl Input {
    pub fn new(input: DeriveInput) -> Input {
        let mut fields = Vec::new();
        let mut field_is_nested = Vec::new();
        let mut field_is_compact = Vec::new();
        match input.data {
            Data::Struct(s) => {
                for field in s.fields.iter().cloned() {
                    let compact = compact_inner(&field.ty);
                    let is_nested =
                        contains_nested_soa(&field.attrs) || compact.is_some();
                    field_is_compact.push(compact);
                    field_is_nested.push(is_nested);
                    fields.push(field.clone());
                }
            }
            _ => panic!("#[derive(SOA)] only supports struct"),
        }

        assert!(
            !fields.is_empty(),
            "#[derive(SOA)] only supports struct with fields"
        );

        let mut extra_attrs = ExtraAttributes::new();

        for attr in input.attrs {
            if attr.path().is_ident("layout") {
                attr.parse_nested_meta(|meta| {
                    match meta.path.get_ident() {
                        Some(ident) => {
                            assert!(ident != "Copy", "can not derive Copy for SoA vectors");
                            if ident != "Default" {
                                // ignore as Default is already derived for SoA vectors, slices and mut slices
                                extra_attrs.add_derive(ident);
                            }
                        }
                        None => {
                            panic!(
                                "expected #[layout(Traits, To, Derive)], got #[{}]",
                                quote!(attr)
                            );
                        }
                    }
                    Ok(())
                })
                .expect("failed to parse layout");
            }

            if attr.path().is_ident("soa_attr") {
                let nested = attr
                    .parse_args_with(
                        Punctuated::<Meta, Token![,]>::parse_terminated,
                    )
                    .expect(
                        "expected attribute like #[soa_attr(<Type>, <attr>)]",
                    );
                assert!(
                    nested.len() == 2,
                    "expected attribute like #[soa_attr(<Type>, <attr>)]"
                );

                let soa_type = nested.first().expect("should have 2 elements");
                let attr =
                    nested.last().expect("should have 2 elements").clone();

                match soa_type.path().get_ident() {
                    Some(ident) => {
                        if ident == "Vec" {
                            extra_attrs.vec.push(attr);
                        } else if ident == "Slice" {
                            extra_attrs.slice.push(attr);
                        } else if ident == "SliceMut" {
                            extra_attrs.slice_mut.push(attr);
                        } else if ident == "Ref" {
                            extra_attrs.ref_.push(attr);
                        } else if ident == "RefMut" {
                            extra_attrs.ref_mut.push(attr);
                        } else if ident == "Ptr" {
                            extra_attrs.ptr.push(attr);
                        } else if ident == "PtrMut" {
                            extra_attrs.ptr_mut.push(attr);
                        } else {
                            panic!(
                                "expected one of the SoA type, got {}",
                                quote!(#soa_type)
                            );
                        }
                    }
                    None => panic!(
                        "expected one of the SoA type, got {}",
                        quote!(#soa_type)
                    ),
                }
            }
        }

        Input {
            name: input.ident,
            fields: fields,
            visibility: input.vis,
            attrs: extra_attrs,
            field_is_nested,
            field_is_compact,
        }
    }

    /// True iff every field is a compact column (`Compact<T>` / `CompactBool`).
    ///
    /// In that case the generated immutable `Ref<'a>` has no field that
    /// actually references `'a` (compact fields are owning `Compact<T>`
    /// snapshots), so the struct's lifetime parameter would be unused
    /// (E0392). Callers add a `PhantomData<&'a ()>` marker to keep the
    /// lifetime structural. This only affects all-compact structs, which do
    /// not compile today, so it breaks no existing code.
    pub(crate) fn ref_needs_lifetime_marker(&self) -> bool {
        !self.field_is_compact.is_empty()
            && self.field_is_compact.iter().all(Option::is_some)
    }

    /// Map over all fields in the struct, calling the first function if the
    /// field is a nested struct of array (including compact columns), the
    /// second function otherwise. The nested closure also receives the compact
    /// inner type as `Some(inner)` for compact fields, `None` otherwise.
    pub(crate) fn map_fields_nested_or<'a, A, B>(
        &'a self,
        nested: A,
        not_nested: B,
    ) -> impl Iterator<Item = TokenStream> + 'a
    where
        A: Fn(&syn::Ident, &syn::Type, Option<&syn::Type>) -> TokenStream + 'a,
        B: Fn(&syn::Ident, &syn::Type) -> TokenStream + 'a,
    {
        self.fields
            .iter()
            .zip(self.field_is_nested.iter())
            .zip(self.field_is_compact.iter())
            .map(move |((field, &is_nested), compact)| {
                if is_nested {
                    nested(
                        field.ident.as_ref().expect("missing ident"),
                        &field.ty,
                        compact.as_ref(),
                    )
                } else {
                    not_nested(
                        field.ident.as_ref().expect("missing ident"),
                        &field.ty,
                    )
                }
            })
    }
}
