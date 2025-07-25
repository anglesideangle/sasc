use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Expr, ExprAwait, FnArg, GenericArgument, ItemFn, Pat, ReturnType,
    Signature, parse_macro_input, visit_mut::VisitMut,
};

#[proc_macro_attribute]
pub fn async_scoped(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as ItemFn);

    // Wraps *every* async expression within the function block with
    // `ScopedFutureWrapper`, allowing them to be treated as regular `Future`
    // impls.
    //
    // This will cause a compiler error if any expression being awaited is not
    // a `ScopedFuture`, which is intentional because the `Future` and
    // `ScopedFuture` systems are incompatible.
    ScopedFutureWrappingVisitor.visit_item_fn_mut(&mut input);

    // Wrap the function with `UnscopedFutureWrapper` to convert it back into
    // a `ScopedFuture`.
    wrap_async_with_scoped(&input).into()
}

/// Takes async fn that returns anonymous `Future` impl.
/// Generates fn that returns `UnscopedFutureWrapper` wrapper for the the anonymous `Future` impl.
///
/// ```rust
/// fn [original name]<'a, 'b>(a: &'a A, b: &'b B) -> impl ScopedFuture<'a + 'b, Output = T> + 'a + 'b {
///   async fn [__inner]<'a, 'b>(a: &'a A, b: &'b B) -> T { [body] } // compilers turns this into -> impl Future<Output = T> + 'a + 'b
///   unsafe { UnscopedFutureWrapper::from_future(__inner()) }
/// }
/// ```
///
/// see https://rust-lang.github.io/rfcs/2394-async_await.html#lifetime-capture-in-the-anonymous-future
/// for more context on lifetime capture
/// - resulting ScopedFuture needs to be constrained to not outlive the lifetimes of any references
///
/// to actually implement this (capture all lifetimes) we use `ScopedFuture<'_> + '_` so the compiler can infer
/// lifetimes from the anonymous future impl returned by the actual inner async fn
fn wrap_async_with_scoped(
    ItemFn {
        attrs,
        vis,
        sig:
            Signature {
                constness,
                unsafety,
                ident,
                generics,
                inputs,
                output,
                ..
            },
        block,
    }: &ItemFn,
) -> proc_macro2::TokenStream {
    let output = match output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    let inner_args: Vec<syn::Ident> = inputs
        .iter()
        .filter_map(|param| match param {
            FnArg::Receiver(_) => Some(quote::format_ident!("self")),
            FnArg::Typed(typed) => {
                if let Pat::Ident(ident) = &*typed.pat {
                    Some(ident.ident.to_owned())
                } else {
                    None
                }
            }
        })
        .collect();

    let has_lifetime_dependency = inputs.iter().any(|param| match param {
        FnArg::Receiver(receiver) => receiver.reference.is_some(),
        FnArg::Typed(pat) => has_lifetime_dependency(&pat.ty),
    });

    let outer_output = if has_lifetime_dependency {
        quote! { futures_core::ScopedFuture<'_, Output = #output> + '_ }
    } else {
        quote! { futures_core::ScopedFuture<'static, Output = #output> }
    };

    quote! {
        #(#attrs)* #vis #constness #unsafety fn #ident #generics (#inputs) -> impl #outer_output {
            async #constness #unsafety fn __inner (#inputs) -> #output #block

            let future = __inner(#(#inner_args),*);

            unsafe { futures_compat::UnscopedFutureWrapper::from_future(future) }
        }
    }
}

/// Determines if typed pattern contains a reference or dependency on a
/// lifetime (used for deciding between '_ and 'static ScopedFuture).
fn has_lifetime_dependency(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Reference(_) => true,
        syn::Type::Path(type_path) => {
            type_path.path.segments.iter().any(|segment| {
                if let syn::PathArguments::AngleBracketed(args) =
                    &segment.arguments
                {
                    args.args.iter().any(|arg| match arg {
                        GenericArgument::Type(ty) => {
                            has_lifetime_dependency(ty)
                        }
                        syn::GenericArgument::Lifetime(_) => true,
                        _ => false,
                    })
                } else {
                    false
                }
            })
        }
        syn::Type::Tuple(tuple) => {
            tuple.elems.iter().any(has_lifetime_dependency)
        }
        syn::Type::Slice(slice) => has_lifetime_dependency(&slice.elem),
        syn::Type::Array(array) => has_lifetime_dependency(&array.elem),
        syn::Type::Ptr(ptr) => has_lifetime_dependency(&ptr.elem),
        syn::Type::Group(group) => has_lifetime_dependency(&group.elem),
        syn::Type::Paren(paren) => has_lifetime_dependency(&paren.elem),
        syn::Type::BareFn(bare_fn) => {
            bare_fn
                .inputs
                .iter()
                .any(|input| has_lifetime_dependency(&input.ty))
                || match &bare_fn.output {
                    ReturnType::Default => false,
                    ReturnType::Type(_, ty) => has_lifetime_dependency(ty),
                }
        }

        _ => false,
    }
}

/// Uses the `syn::visit_mut` api to wrap every `.await` expression in
/// `ScopedFutureWrapper`.
struct ScopedFutureWrappingVisitor;

impl VisitMut for ScopedFutureWrappingVisitor {
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        if let Expr::Await(ExprAwait { attrs, base, .. }) = expr {
            *expr = syn::parse_quote! {
                unsafe { futures_compat::ScopedFutureWrapper::from_scoped(#(#attrs)* #base) }.await
            };
        }

        syn::visit_mut::visit_expr_mut(self, expr);
    }
}
