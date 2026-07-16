use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parse_quote, visit::Visit, visit_mut::VisitMut, Expr, ExprField, Ident,
    Member,
};

use crate::names;

pub fn soa_impl_transform(
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let Ok(item_impl) = syn::parse::<syn::ItemImpl>(item) else {
        panic!("soa_impl can only be applied to impl blocks");
    };

    let struct_ident = extract_struct_ident(&item_impl);
    let field_names = collect_self_field_names(&item_impl);

    let original_tokens = item_impl.to_token_stream();
    let ref_impl = generate_ref_impl(&item_impl, &struct_ident, &field_names);
    let ref_mut_impl =
        generate_ref_mut_impl(&item_impl, &struct_ident, &field_names);

    quote! {
        #original_tokens
        #ref_impl
        #ref_mut_impl
    }
    .into()
}

fn extract_struct_ident(item_impl: &syn::ItemImpl) -> syn::Ident {
    if let syn::Type::Path(type_path) = &*item_impl.self_ty {
        type_path
            .path
            .segments
            .first()
            .expect("expected a type name")
            .ident
            .clone()
    } else {
        panic!(
            "soa_impl can only be applied to impl blocks for a named struct"
        );
    }
}

// ---------------------------------------------------------------------------
// Field name collection via read-only AST walk
// ---------------------------------------------------------------------------

struct FieldNameCollector {
    field_names: Vec<Ident>,
}

impl FieldNameCollector {
    fn new() -> Self {
        FieldNameCollector {
            field_names: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for FieldNameCollector {
    fn visit_expr_field(&mut self, expr: &'ast ExprField) {
        if is_self_field_expr(&expr.base) {
            if let Member::Named(ident) = &expr.member {
                if !self.field_names.contains(ident) {
                    self.field_names.push(ident.clone());
                }
            }
        }
        syn::visit::visit_expr_field(self, expr);
    }
}

fn is_self_field_expr(expr: &Expr) -> bool {
    if let Expr::Path(path_expr) = expr {
        path_expr.path.is_ident("self")
    } else {
        false
    }
}

fn collect_self_field_names(item_impl: &syn::ItemImpl) -> Vec<Ident> {
    let mut collector = FieldNameCollector::new();
    collector.visit_item_impl(item_impl);
    collector.field_names
}

// ---------------------------------------------------------------------------
// AST mutation: insert dereferences for self.field accesses
// ---------------------------------------------------------------------------

fn is_compound_assign_op(op: &syn::BinOp) -> bool {
    matches!(
        op,
        syn::BinOp::AddAssign(_)
            | syn::BinOp::SubAssign(_)
            | syn::BinOp::MulAssign(_)
            | syn::BinOp::DivAssign(_)
            | syn::BinOp::RemAssign(_)
            | syn::BinOp::BitAndAssign(_)
            | syn::BinOp::BitOrAssign(_)
            | syn::BinOp::BitXorAssign(_)
            | syn::BinOp::ShlAssign(_)
            | syn::BinOp::ShrAssign(_)
    )
}

struct SelfFieldTransformer<'a> {
    field_names: &'a [Ident],
    is_ref_mut: bool,
    /// When true, wrapping of `self.field` reads is suppressed for the
    /// sub-expression currently being visited (used by the place-expression
    /// handlers for assignment LHS and `&`/`&mut` operands).
    suppress: bool,
}

impl SelfFieldTransformer<'_> {
    fn is_known_field(&self, expr: &Expr) -> bool {
        if let Expr::Field(field_expr) = expr {
            if is_self_field_expr(&field_expr.base) {
                if let Member::Named(ident) = &field_expr.member {
                    return self.field_names.contains(ident);
                }
            }
        }
        false
    }

    fn wrap_deref(expr: &Expr) -> Expr {
        let tokens = expr.to_token_stream();
        parse_quote! { (*#tokens) }
    }

    fn prefix_deref(expr: &Expr) -> Expr {
        let tokens = expr.to_token_stream();
        parse_quote! { *#tokens }
    }
}

impl VisitMut for SelfFieldTransformer<'_> {
    /// General dispatcher: any bare `self.field` read in a value position is
    /// wrapped as `(*self.field)` unless wrapping is suppressed for this
    /// sub-expression. Special-case handlers (assignment LHS, compound-assign
    /// LHS, `&`/`&mut` operands) take over via early return so they can manage
    /// their operands' suppression/prefixing themselves.
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match expr {
            Expr::Assign(assign) => {
                self.visit_expr_assign_mut(assign);
                return;
            }
            Expr::Binary(binary) if is_compound_assign_op(&binary.op) => {
                self.visit_expr_binary_mut(binary);
                return;
            }
            Expr::Reference(reference) => {
                self.visit_expr_reference_mut(reference);
                return;
            }
            _ => {}
        }

        if !self.suppress && self.is_known_field(expr) {
            // Wrap in-place; do NOT recurse — the wrapped field is `self.field`
            // and re-visiting it would double-wrap.
            *expr = SelfFieldTransformer::wrap_deref(expr);
            return;
        }

        syn::visit_mut::visit_expr_mut(self, expr);
    }

    fn visit_expr_assign_mut(&mut self, expr: &mut syn::ExprAssign) {
        // RHS wraps normally (general dispatcher); LHS is a place, so suppress
        // wrapping there and prefix `*` afterwards when it is a known field.
        // NB: recurse via `self.visit_expr_mut` (the trait method, i.e. the
        // general dispatcher) — NOT `syn::visit_mut::visit_expr_mut`, whose
        // free function dispatches by variant and would bypass our wrapping.
        let prev = self.suppress;
        self.suppress = true;
        self.visit_expr_mut(&mut expr.left);
        self.suppress = prev;

        self.visit_expr_mut(&mut expr.right);

        if self.is_ref_mut && self.is_known_field(&expr.left) {
            let prefixed = SelfFieldTransformer::prefix_deref(&expr.left);
            *expr.left = prefixed;
        }
    }

    fn visit_expr_binary_mut(&mut self, expr: &mut syn::ExprBinary) {
        if is_compound_assign_op(&expr.op) {
            // LHS is a place (suppress + prefix when ref_mut); RHS wraps
            // normally.
            let prev = self.suppress;
            self.suppress = true;
            self.visit_expr_mut(&mut expr.left);
            self.suppress = prev;

            self.visit_expr_mut(&mut expr.right);

            if self.is_ref_mut && self.is_known_field(&expr.left) {
                let prefixed = SelfFieldTransformer::prefix_deref(&expr.left);
                *expr.left = prefixed;
            }
        } else {
            // Plain binary op: recurse both sides, general dispatcher wraps.
            self.visit_expr_mut(&mut expr.left);
            self.visit_expr_mut(&mut expr.right);
        }
    }

    fn visit_expr_reference_mut(&mut self, expr: &mut syn::ExprReference) {
        // Do NOT wrap self.field inside & — it is already a reference on Ref.
        // Suppress wrapping for the operand but still recurse so nested value
        // positions (e.g. `&(self.a + 1)`) get handled correctly.
        let prev = self.suppress;
        self.suppress = true;
        self.visit_expr_mut(&mut expr.expr);
        self.suppress = prev;
    }

    fn visit_expr_method_call_mut(&mut self, expr: &mut syn::ExprMethodCall) {
        // A direct `self.field` receiver is a place that auto-derefs (e.g.
        // `self.flag.get()` on a `Compact<T>`, or `self.name.clone()` on a
        // `String`), so it must NOT be wrapped — skip it. Any other receiver is
        // a value expression whose nested `self.field` reads are operands, and
        // those DO need wrapping: otherwise on `RefMut` they stay `&mut T` and
        // arithmetic such as `(self.x * self.x).sqrt()` fails to compile
        // (`&mut T * &mut T` is not implemented, unlike `&T * &T`). Recurse
        // normally (wrapping enabled) into every non-direct-field receiver.
        if !self.is_known_field(&expr.receiver) {
            self.visit_expr_mut(&mut expr.receiver);
        }

        for arg in &mut expr.args {
            self.visit_expr_mut(arg);
        }
    }

    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        syn::visit_mut::visit_block_mut(self, block);
    }
}

// ---------------------------------------------------------------------------
// Method classification helpers
// ---------------------------------------------------------------------------

fn is_ref_self_method(method: &syn::ImplItemFn) -> bool {
    if let Some(syn::FnArg::Receiver(receiver)) = method.sig.inputs.first() {
        receiver.reference.is_some() && receiver.mutability.is_none()
    } else {
        false
    }
}

fn is_mut_self_method(method: &syn::ImplItemFn) -> bool {
    if let Some(syn::FnArg::Receiver(receiver)) = method.sig.inputs.first() {
        receiver.reference.is_some() && receiver.mutability.is_some()
    } else {
        false
    }
}

fn returns_self(method: &syn::ImplItemFn) -> bool {
    if let syn::ReturnType::Type(_, ty) = &method.sig.output {
        if let syn::Type::Path(type_path) = &**ty {
            return type_path.path.is_ident("Self");
        }
    }
    false
}

/// True if the method mentions `Self` anywhere outside the receiver.
///
/// On the generated `Ref`/`RefMut`, `Self` resolves to the ref type, not the
/// owned struct, so anything that depends on `Self` breaks when cloned:
/// `&Self` / `&mut Self` / `Option<Self>` parameters, compound `Self` returns,
/// `Self::associated()` calls, and `Self { ... }` construction. Such methods
/// are kept on the owned struct only. The shorthand receiver `self`/`&self`/
/// `&mut self` tokenizes as the lowercase ident `self`, so it never matches
/// `Self` and ordinary methods are unaffected.
fn mentions_self(method: &syn::ImplItemFn) -> bool {
    tokens_mention_self(method.to_token_stream())
}

/// Recursively scan a token stream (descending into groups, which is where the
/// parameter list and body live) for the `Self` ident. The receiver tokenizes
/// as lowercase `self`, so it never matches.
fn tokens_mention_self(tokens: proc_macro2::TokenStream) -> bool {
    use proc_macro2::TokenTree;
    for tt in tokens {
        match tt {
            TokenTree::Ident(i) if i == "Self" => return true,
            TokenTree::Group(g) if tokens_mention_self(g.stream()) => {
                return true
            }
            _ => {}
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

fn generate_ref_impl(
    item_impl: &syn::ItemImpl,
    struct_ident: &syn::Ident,
    field_names: &[Ident],
) -> TokenStream {
    let ref_name = names::ref_name(struct_ident);
    let mut methods: Vec<syn::ImplItemFn> = Vec::new();

    for item in &item_impl.items {
        if let syn::ImplItem::Fn(method) = item {
            if is_ref_self_method(method)
                && !returns_self(method)
                && !mentions_self(method)
            {
                let mut cloned = method.clone();
                let mut visitor = SelfFieldTransformer {
                    field_names,
                    is_ref_mut: false,
                    suppress: false,
                };
                visitor.visit_impl_item_fn_mut(&mut cloned);
                methods.push(cloned);
            }
        }
    }

    if methods.is_empty() {
        return TokenStream::new();
    }

    quote! {
        impl<'a> #ref_name<'a> {
            #(#methods)*
        }
    }
}

fn generate_ref_mut_impl(
    item_impl: &syn::ItemImpl,
    struct_ident: &syn::Ident,
    field_names: &[Ident],
) -> TokenStream {
    let ref_mut_name = names::ref_mut_name(struct_ident);
    let mut methods: Vec<syn::ImplItemFn> = Vec::new();

    for item in &item_impl.items {
        if let syn::ImplItem::Fn(method) = item {
            // RefMut gets both `&mut self` and `&self` methods: a mutable
            // borrow can do everything an immutable one can (mirrors `&mut T`
            // being usable for `&self` calls). A `&self` method does no field
            // assignments, so the `is_ref_mut` transform (which only diverges
            // on assignment LHS) produces the same result as for `Ref`.
            if (is_mut_self_method(method) || is_ref_self_method(method))
                && !returns_self(method)
                && !mentions_self(method)
            {
                let mut cloned = method.clone();
                let mut visitor = SelfFieldTransformer {
                    field_names,
                    is_ref_mut: true,
                    suppress: false,
                };
                visitor.visit_impl_item_fn_mut(&mut cloned);
                methods.push(cloned);
            }
        }
    }

    if methods.is_empty() {
        return TokenStream::new();
    }

    quote! {
        impl<'a> #ref_mut_name<'a> {
            #(#methods)*
        }
    }
}
