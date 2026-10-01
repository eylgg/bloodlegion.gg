use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Meta, parse_macro_input};

/// True when a `status = ...` value names the `INTERNAL_SERVER_ERROR` constant.
fn is_internal_server_error(expr: &syn::Expr) -> bool {
    matches!(
        expr,
        syn::Expr::Path(path)
            if path.path.segments.last().is_some_and(|s| s.ident == "INTERNAL_SERVER_ERROR")
    )
}

/// The compile error emitted when an External problem claims a 500 status
/// without the `internal` opt-out. A 500 means the server failed internally,
/// the cause must be logged and hidden, not described to the client, so it
/// belongs in `crate::Error::Internal`, reached via `.context(...)?`.
fn reject_internal_status(expr: &syn::Expr) -> proc_macro2::TokenStream {
    syn::Error::new_spanned(
        expr,
        "`INTERNAL_SERVER_ERROR` is not a valid status for an External problem: a 500 means the \
         server failed internally, so the cause must be logged and hidden rather than described \
         to the client. Route it through `crate::Error::Internal` instead (e.g. `.context(\"...\")?`). \
         Gateway failures (502/503/504) are allowed as External problems. If this is the one \
         canonical internal renderer, mark it `#[problem(internal)]`.",
    )
    .to_compile_error()
}

/// The parsed contents of the `#[problem(...)]` attribute(s) on one item (an
/// enum variant or a struct). The struct and enum code paths both build the
/// same `Problem` accessors, so they share this parser rather than duplicating
/// the attribute match.
#[derive(Default)]
struct ProblemAttrs {
    status: Option<proc_macro2::TokenStream>,
    title: Option<proc_macro2::TokenStream>,
    detail: Option<proc_macro2::TokenStream>,
    r#type: Option<proc_macro2::TokenStream>,
    instance: Option<proc_macro2::TokenStream>,
    extensions: Option<proc_macro2::TokenStream>,
    /// `#[problem(internal)]`: opts this item out of the 500-status ban.
    is_internal: bool,
    /// `#[problem(transparent)]`: delegate every accessor to the wrapped
    /// inner problem (enum variants only).
    is_transparent: bool,
    /// The `status = INTERNAL_SERVER_ERROR` expression, captured so the ban
    /// can point its compile error at the offending span.
    status_500: Option<syn::Expr>,
}

impl ProblemAttrs {
    /// Collects every `#[problem(...)]` attribute in `attrs` into one value.
    fn parse(attrs: &[syn::Attribute]) -> Self {
        let mut out = Self::default();
        for attr in attrs {
            if !attr.path().is_ident("problem") {
                continue;
            }
            let nested = attr
                .parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )
                .unwrap();
            for meta in nested {
                match meta {
                    Meta::Path(path) if path.is_ident("transparent") => out.is_transparent = true,
                    Meta::Path(path) if path.is_ident("internal") => out.is_internal = true,
                    Meta::NameValue(nv) => {
                        let ident = nv.path.get_ident().unwrap().to_string();
                        let expr = nv.value;
                        match ident.as_str() {
                            "status" => {
                                if is_internal_server_error(&expr) {
                                    out.status_500 = Some(expr.clone());
                                }
                                out.status = Some(quote! { axum::http::StatusCode::#expr });
                            }
                            "title" => out.title = Some(quote! { #expr }),
                            "detail" => out.detail = Some(quote! { #expr.into() }),
                            "type" => out.r#type = Some(quote! { #expr }),
                            "instance" => out.instance = Some(quote! { #expr.into() }),
                            "extensions" => out.extensions = Some(quote! { #expr }),
                            _ => panic!("Unknown problem attribute: {}", ident),
                        }
                    }
                    _ => panic!("Unsupported problem attribute format"),
                }
            }
        }
        out
    }

    /// The compile error to emit when this item is an External problem (no
    /// `internal` opt-out) that nonetheless claims a 500 status, or `None` when
    /// the status is allowed.
    fn internal_status_violation(&self) -> Option<proc_macro2::TokenStream> {
        match &self.status_500 {
            Some(expr) if !self.is_internal => Some(reject_internal_status(expr)),
            _ => None,
        }
    }
}

#[proc_macro_derive(Problem, attributes(problem))]
pub fn derive_problem(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let item_name = &ast.ident;

    match &ast.data {
        Data::Enum(data_enum) => {
            let mut status_arms = Vec::new();
            let mut title_arms = Vec::new();
            let mut detail_arms = Vec::new();
            let mut type_arms = Vec::new();
            let mut instance_arms = Vec::new();
            let mut extensions_arms = Vec::new();

            for variant in &data_enum.variants {
                let variant_name = &variant.ident;
                let attrs = ProblemAttrs::parse(&variant.attrs);

                let fields_match = match &variant.fields {
                    syn::Fields::Unit => quote! {},
                    syn::Fields::Unnamed(f) => {
                        let idents = (0..f.unnamed.len()).map(|i| {
                            syn::Ident::new(&format!("_{}", i), proc_macro2::Span::call_site())
                        });
                        quote! { (#(#idents),*) }
                    }
                    syn::Fields::Named(f) => {
                        let idents = f.named.iter().map(|f| &f.ident);
                        quote! { { #(#idents),* } }
                    }
                };

                if attrs.is_transparent {
                    status_arms.push(quote! { Self::#variant_name(inner) => inner.status(), });
                    title_arms.push(quote! { Self::#variant_name(inner) => inner.title(), });
                    detail_arms.push(quote! { Self::#variant_name(inner) => inner.detail(), });
                    type_arms.push(quote! { Self::#variant_name(inner) => inner.r#type(), });
                    instance_arms.push(quote! { Self::#variant_name(inner) => inner.instance(), });
                    extensions_arms
                        .push(quote! { Self::#variant_name(inner) => inner.extensions(), });
                    continue;
                }

                if let Some(err) = attrs.internal_status_violation() {
                    return TokenStream::from(err);
                }

                let status = attrs.status.unwrap_or_else(|| {
                    panic!(
                        "Missing 'status' in #[problem(...)] on variant: {}",
                        variant_name
                    )
                });
                let title = attrs.title.unwrap_or_else(|| {
                    panic!(
                        "Missing 'title' in #[problem(...)] on variant: {}",
                        variant_name
                    )
                });
                let detail = attrs.detail.unwrap_or_else(|| {
                    panic!(
                        "Missing 'detail' in #[problem(...)] on variant: {}",
                        variant_name
                    )
                });

                status_arms.push(quote! { Self::#variant_name #fields_match => #status, });
                title_arms.push(quote! { Self::#variant_name #fields_match => #title, });
                detail_arms.push(quote! { Self::#variant_name #fields_match => #detail, });

                if let Some(r#type) = attrs.r#type {
                    type_arms.push(quote! { Self::#variant_name #fields_match => #r#type, });
                }
                if let Some(inst) = attrs.instance {
                    instance_arms
                        .push(quote! { Self::#variant_name #fields_match => Some(#inst), });
                }
                if let Some(ext) = attrs.extensions {
                    extensions_arms.push(quote! {
                        Self::#variant_name #fields_match => match #ext {
                            serde_json::Value::Object(map) => Some(map),
                            _ => None,
                        },
                    });
                }
            }

            let expanded = quote! {
                impl Problem for #item_name {
                    fn status(&self) -> axum::http::StatusCode {
                        match self { #(#status_arms)* }
                    }
                    fn title(&self) -> &'static str {
                        match self { #(#title_arms)* }
                    }
                    fn detail(&self) -> String {
                        match self { #(#detail_arms)* }
                    }
                    fn r#type(&self) -> &'static str {
                        #[allow(unreachable_patterns)]
                        match self {
                            #(#type_arms)*
                            _ => "about:blank",
                        }
                    }
                    fn instance(&self) -> Option<String> {
                        #[allow(unreachable_patterns)]
                        match self {
                            #(#instance_arms)*
                            _ => None,
                        }
                    }
                    fn extensions(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
                        #[allow(unreachable_patterns)]
                        match self {
                            #(#extensions_arms)*
                            _ => None,
                        }
                    }
                }
            };

            TokenStream::from(expanded)
        }
        Data::Struct(_) => {
            let attrs = ProblemAttrs::parse(&ast.attrs);

            if let Some(err) = attrs.internal_status_violation() {
                return TokenStream::from(err);
            }

            let status = attrs.status.unwrap_or_else(|| {
                panic!(
                    "Missing 'status' in #[problem(...)] on struct: {}",
                    item_name
                )
            });
            let title = attrs.title.unwrap_or_else(|| {
                panic!(
                    "Missing 'title' in #[problem(...)] on struct: {}",
                    item_name
                )
            });
            let detail = attrs.detail.unwrap_or_else(|| {
                panic!(
                    "Missing 'detail' in #[problem(...)] on struct: {}",
                    item_name
                )
            });

            let type_method = attrs
                .r#type
                .map(|t| quote! { fn r#type(&self) -> &'static str { #t } });
            let instance_method = attrs
                .instance
                .map(|i| quote! { fn instance(&self) -> Option<String> { Some(#i) } });
            let extensions_method = attrs.extensions.map(|e| {
                quote! {
                    fn extensions(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
                        match #e {
                            serde_json::Value::Object(map) => Some(map),
                            _ => None,
                        }
                    }
                }
            });

            let expanded = quote! {
                impl Problem for #item_name {
                    fn status(&self) -> axum::http::StatusCode { #status }
                    fn title(&self) -> &'static str { #title }
                    fn detail(&self) -> String { #detail }
                    #type_method
                    #instance_method
                    #extensions_method
                }
            };

            TokenStream::from(expanded)
        }
        Data::Union(_) => panic!("Problem cannot be derived on Unions"),
    }
}
