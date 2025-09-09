use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{
    Expr, ExprAwait, ItemFn, ReturnType, parse_macro_input, parse_quote,
    parse2, visit_mut::VisitMut,
};

/// Takes async fn that returns anonymous `Future` impl.
/// Generates fn that returns `UnscopedFutureWrapper` wrapper for the the anonymous `Future` impl.
///
/// ```rust,ignore
/// fn my_func<'a, 'b>(a: &'a A, b: &'b B) -> impl ScopedFuture<LifetimeGuard, Output = Output> {
///   let output = async move { [body] } // compilers turns this into -> impl Future<Output = T> + 'a + 'b
///   unsafe { futures_compat::std_future_to_bespoke(output) }
/// }
/// ```
///
/// see https://rust-lang.github.io/rfcs/2394-async_await.html#lifetime-capture-in-the-anonymous-future
/// for more context on lifetime capture
/// - resulting ScopedFuture needs to be constrained to not outlive the lifetimes of any references
///
/// to actually implement this (capture all lifetimes) we use `ScopedFuture<'_> + '_` so the compiler can infer
/// lifetimes from the anonymous future impl returned by the actual inner async fn
#[proc_macro_attribute]
pub fn async_function(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut item_fn = parse_macro_input!(item as ItemFn);
    // Wraps *every* async expression within the function block with
    // `BespokeFutureWrapper`, allowing them to be treated as regular `Future`
    // impls.
    //
    // This will cause a compiler error if any expression being awaited is not
    // a `ScopedFuture`, which is intentional because the `Future` and
    // `ScopedFuture` systems are incompatible.
    BespokeFutureWrappingVisitor.visit_item_fn_mut(&mut item_fn);

    // disable async since it is moved to the block
    item_fn.sig.asyncness = None;

    // wrap block with UnscopedFutureWrapper
    let block = *item_fn.block;
    *item_fn.block = parse_quote! {
        {
            let future = async move #block;
            unsafe { futures_compat::std_future_to_bespoke(future) }
        }
    };

    let output_type = match &item_fn.sig.output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    item_fn.sig.output = parse_quote! { -> impl futures_core::Future<LocalWaker, Output = #output_type> };

    // let has_lifetime_dependency =
    //     item_fn.sig.inputs.iter().any(|param| match param {
    //         FnArg::Receiver(receiver) => receiver.reference.is_some(),
    //         FnArg::Typed(pat) => has_lifetime_dependency(&pat.ty),
    //     });

    // // set outer fn output to ScopedFuture<'_/'static, Output = #output>
    // item_fn.sig.output = if has_lifetime_dependency {
    //     parse_quote! { -> impl futures_core::ScopedFuture<'_, Output = #output> + '_ }
    // } else {
    //     parse_quote! { -> impl futures_core::ScopedFuture<'static, Output = #output> }
    // };

    item_fn.to_token_stream().into()
}

/// This currently is impossible to do the `futures_compat` workarounds not
/// being compatible with closures.
///
/// Takes async fn that returns anonymous `Future` impl.
/// Generates fn that returns `UnscopedFutureWrapper` wrapper for the the anonymous `Future` impl.
///
/// ```rust,ignore
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
// #[proc_macro]
// pub fn closure(input: TokenStream) -> TokenStream {
//     // let ExprClosure {
//     //     attrs,
//     //     lifetimes,
//     //     constness,
//     //     movability,
//     //     capture,
//     //     inputs,
//     //     output,
//     //     body,
//     //     ..
//     // } = parse_macro_input!(input as ExprClosure);
//     let mut closure = parse_macro_input!(input as ExprClosure);
//     // disable async because we move it to inner
//     closure.asyncness = None;
//     let body = closure.body;

//     // let output = match closure.output {
//     //     ReturnType::Default => parse_quote! { () },
//     //     ReturnType::Type(_, ty) => parse_quote! { #ty },
//     // };

//     // let outer_output =
//     //     parse_quote! { futures_core::ScopedFuture<'_, Output = #output> + '_ };

//     closure.body = parse_quote! {{
//         let output = async move { #body };
//         unsafe { futures_compat::UnscopedFutureWrapper::from_future(output) }
//     }};
//     // closure.output = outer_output;
//     closure.to_token_stream().into()
// }

/// Wraps a block of optionally async statements and expressions in an anonymous `ScopedFuture` impl.
///
/// This generates a modified block of the form:
///
/// ```rust,ignore
/// {
///   let output = async { <original block, mapped to convert all `ScopedFuture` to `Future`> };
///   unsafe { futures_compat::UnscopedFutureWrapper::from_future(output) }
/// }
/// ```
#[proc_macro]
pub fn async_block(input: TokenStream) -> TokenStream {
    // block is formed { **expr/stmt }, so we need to surround the inputs in {}
    let input = proc_macro2::TokenStream::from(input);
    let block_input = quote! { { #input } };

    let mut block = parse2(block_input).expect("Failed to parse as block.");

    BespokeFutureWrappingVisitor.visit_block_mut(&mut block);

    quote! {
        {
            let output = async #block;
            unsafe { futures_compat::std_future_to_bespoke(output) }
        }
    }
    .into()
}

/// Determines if typed pattern contains a reference or dependency on a
/// lifetime (used for deciding between '_ and 'static ScopedFuture).
// fn has_lifetime_dependency(ty: &syn::Type) -> bool {
//     match ty {
//         syn::Type::Reference(_) => true,
//         syn::Type::Path(type_path) => {
//             type_path.path.segments.iter().any(|segment| {
//                 if let syn::PathArguments::AngleBracketed(args) =
//                     &segment.arguments
//                 {
//                     args.args.iter().any(|arg| match arg {
//                         GenericArgument::Type(ty) => {
//                             has_lifetime_dependency(ty)
//                         }
//                         syn::GenericArgument::Lifetime(_) => true,
//                         _ => false,
//                     })
//                 } else {
//                     false
//                 }
//             })
//         }
//         syn::Type::Tuple(tuple) => {
//             tuple.elems.iter().any(has_lifetime_dependency)
//         }
//         syn::Type::Slice(slice) => has_lifetime_dependency(&slice.elem),
//         syn::Type::Array(array) => has_lifetime_dependency(&array.elem),
//         syn::Type::Ptr(ptr) => has_lifetime_dependency(&ptr.elem),
//         syn::Type::Group(group) => has_lifetime_dependency(&group.elem),
//         syn::Type::Paren(paren) => has_lifetime_dependency(&paren.elem),
//         syn::Type::BareFn(bare_fn) => {
//             bare_fn
//                 .inputs
//                 .iter()
//                 .any(|input| has_lifetime_dependency(&input.ty))
//                 || match &bare_fn.output {
//                     ReturnType::Default => false,
//                     ReturnType::Type(_, ty) => has_lifetime_dependency(ty),
//                 }
//         }

//         _ => false,
//     }
// }

/// Uses the `syn::visit_mut` api to wrap every `.await` expression in
/// `ScopedFutureWrapper`.
struct BespokeFutureWrappingVisitor;

impl VisitMut for BespokeFutureWrappingVisitor {
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        if let Expr::Await(ExprAwait { attrs, base, .. }) = expr {
            *expr = syn::parse_quote! {
                unsafe { futures_compat::bespoke_future_to_std(#(#attrs)* #base) }.await
            };
        }

        syn::visit_mut::visit_expr_mut(self, expr);
    }
}
