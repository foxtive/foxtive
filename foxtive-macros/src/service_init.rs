use quote::quote;
use syn::{Data, DeriveInput, Fields, Meta, Type};

/// Infrastructure types and their App accessor methods.
/// Maps type name suffix → (accessor_method, needs_clone)
const INFRASTRUCTURE_TYPES: &[(&str, &str, bool)] = &[
    ("DBPool", "db", true),
    ("Redis", "redis", true),
    ("RabbitMQ", "rabbitmq", false),
    ("Cache", "cache", false),
    ("Jwt", "jwt", true),
    ("Password", "password", true),
];

/// Information about a field initialization collected during parsing.
struct FieldInit {
    field_name: syn::Ident,
    init_expr: proc_macro2::TokenStream,
}

/// Information about a Lazy<T> field collected during parsing.
struct LazyFieldInfo {
    field_name: syn::Ident,
    inner_type: Type,
}

pub fn derive_service_init(input: &DeriveInput) -> proc_macro2::TokenStream {
    let name = &input.ident;
    let name_str = name.to_string();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Check for #[service(...)] struct-level attributes
    let is_mutable = has_service_mutable_attr(&input.attrs);
    let all_deps = has_service_all_attr(&input.attrs);
    let skip_hooks = has_service_skip_hooks_attr(&input.attrs);

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            Fields::Unit => {
                let mutable_method = if is_mutable {
                    quote! { fn is_mutable() -> bool { true } }
                } else {
                    quote! {}
                };

                let hooks_impl = if skip_hooks {
                    quote! {}
                } else {
                    quote! {
                        impl #impl_generics foxtive::lifecycle::ServiceHooks for #name #ty_generics #where_clause {}
                    }
                };

                let expanded = quote! {
                    #hooks_impl

                    impl #impl_generics foxtive::lifecycle::ServiceInit for #name #ty_generics #where_clause {
                        async fn init(_app: &foxtive::App) -> foxtive::prelude::AppResult<Self> {
                            Ok(Self)
                        }

                        fn after_init(&mut self, __app: &foxtive::App) -> foxtive::prelude::AppResult<()> {
                            <Self as foxtive::lifecycle::ServiceHooks>::after_init(self, __app)
                        }

                        fn on_ready(__app: &foxtive::App) -> foxtive::prelude::AppResult<()> {
                            <Self as foxtive::lifecycle::ServiceHooks>::on_ready(__app)
                        }

                        #mutable_method
                    }
                };
                return expanded;
            }
            _ => {
                return syn::Error::new_spanned(
                    input,
                    "ServiceInit can only be derived for structs with named fields or unit structs",
                )
                .to_compile_error();
            }
        },
        _ => {
            return syn::Error::new_spanned(input, "ServiceInit can only be derived for structs")
                .to_compile_error();
        }
    };

    let mut field_inits = Vec::new();
    let mut dependency_types = Vec::new();
    let mut lazy_fields: Vec<LazyFieldInfo> = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;
        let field_name_str = field_name.to_string();

        let has_dep_attr = field.attrs.iter().any(|attr| {
            if attr.path().is_ident("dependency") {
                return true;
            }
            if attr.path().is_ident("foxtive")
                && let Meta::List(meta_list) = &attr.meta {
                    return meta_list.tokens.to_string().contains("dependency");
                }
            false
        });

        let has_default_attr = field.attrs.iter().any(|attr| {
            if attr.path().is_ident("foxtive")
                && let Meta::List(meta_list) = &attr.meta {
                    return meta_list.tokens.to_string().contains("default");
                }
            false
        });

        let init_expr = extract_init_expr(&field.attrs);
        let has_init_attr = init_expr.is_some();

        // Conflict checks
        if has_dep_attr && has_default_attr {
            return syn::Error::new_spanned(
                field,
                "Field cannot have both #[dependency] and #[foxtive(default)]",
            )
            .to_compile_error();
        }
        if has_init_attr && has_dep_attr {
            return syn::Error::new_spanned(
                field,
                "Field cannot have both #[foxtive(init)] and #[dependency]",
            )
            .to_compile_error();
        }
        if has_init_attr && has_default_attr {
            return syn::Error::new_spanned(
                field,
                "Field cannot have both #[foxtive(init)] and #[foxtive(default)]",
            )
            .to_compile_error();
        }

        // Fields with #[foxtive(init = "expr")] use the custom expression
        if has_init_attr {
            field_inits.push(FieldInit {
                field_name: field_name.clone(),
                init_expr: init_expr.unwrap(),
            });
            continue;
        }

        // Determine if this field is a dependency:
        // - all_deps mode: all fields are deps unless #[default]
        // - default mode: only #[dependency] fields are deps
        let is_dependency = if all_deps {
            !has_default_attr
        } else {
            has_dep_attr
        };

        if !is_dependency {
            // Non-dependency field: use Default::default()
            field_inits.push(FieldInit {
                field_name: field_name.clone(),
                init_expr: quote! { ::std::default::Default::default() },
            });
            continue;
        }

        // 1. Check if outermost type is Lazy
        if let Some(lazy_inner) = extract_lazy_inner(field_type) {
            // 2. Reject Lazy<Arc<T>>
            if is_arc_type(&lazy_inner) {
                return syn::Error::new_spanned(
                    field_type,
                    "Use Lazy<T>, not Lazy<Arc<T>> - Lazy already stores Arc<T> internally",
                )
                .to_compile_error();
            }

            // 3. Reject Lazy<InfraType>
            if let Some(infra_name) = get_type_name(&lazy_inner)
                && INFRASTRUCTURE_TYPES.iter().any(|(n, _, _)| *n == infra_name)
            {
                return syn::Error::new_spanned(
                    field_type,
                    format!(
                        "Lazy<{infra_name}> is not supported - infrastructure types are leaf nodes and never form cycles"
                    ),
                )
                .to_compile_error();
            }

            field_inits.push(FieldInit {
                field_name: field_name.clone(),
                init_expr: quote! { foxtive::container::Lazy::new(#name_str, #field_name_str) },
            });

            lazy_fields.push(LazyFieldInfo {
                field_name: field_name.clone(),
                inner_type: lazy_inner,
            });

            // Lazy deps excluded from topo sort
            continue;
        }

        // 4. Check if this is an infrastructure type
        if let Some(infra_expr) = try_generate_infra_access(field_type) {
            field_inits.push(FieldInit {
                field_name: field_name.clone(),
                init_expr: infra_expr,
            });
        } else {
            // Regular service-to-service dependency
            let (inner_type, is_arc) = extract_arc_inner(field_type);

            if is_arc {
                field_inits.push(FieldInit {
                    field_name: field_name.clone(),
                    init_expr: quote! { app.require::<#inner_type>()? },
                });
            } else {
                field_inits.push(FieldInit {
                    field_name: field_name.clone(),
                    init_expr: quote! { app.require::<#field_type>()?.as_ref().clone() },
                });
            }

            dependency_types.push(inner_type);
        }
    }

    // Generate type_name::<T>() calls so dependency names match
    // ServiceFactory::type_name() (which uses std::any::type_name::<T>()).
    let dependency_name_exprs: Vec<proc_macro2::TokenStream> = dependency_types
        .iter()
        .map(|t| quote! { std::any::type_name::<#t>() })
        .collect();

    // Generate wire_lazy override only if there are Lazy<T> fields
    let wire_lazy_method = if lazy_fields.is_empty() {
        // No lazy fields - use default no-op (don't generate override)
        quote! {}
    } else {
        let wire_stmts: Vec<_> = lazy_fields
            .iter()
            .map(|info| {
                let field = &info.field_name;
                let inner_ty = &info.inner_type;
                quote! {
                    __app.require_lazy::<#inner_ty>(&__svc.#field)?;
                }
            })
            .collect();

        quote! {
            fn wire_lazy(__app: &foxtive::App) -> foxtive::prelude::AppResult<()> {
                let __svc = __app.require::<#name>()?;
                #(#wire_stmts)*
                Ok(())
            }
        }
    };

    // Build field initializers for direct struct construction
    let field_init_tokens: Vec<_> = field_inits
        .iter()
        .map(|fi| {
            let fname = &fi.field_name;
            let expr = &fi.init_expr;
            quote! { #fname: #expr }
        })
        .collect();

    // Generate is_mutable override if #[service(mutable)] is present
    let mutable_method = if is_mutable {
        quote! { fn is_mutable() -> bool { true } }
    } else {
        quote! {}
    };

    // Generate ServiceHooks impl (no-op) unless skip_hooks is set
    let hooks_impl = if skip_hooks {
        quote! {}
    } else {
        quote! {
            impl #impl_generics foxtive::lifecycle::ServiceHooks for #name #ty_generics #where_clause {}
        }
    };

    let expanded = quote! {
        #hooks_impl

        impl #impl_generics foxtive::lifecycle::ServiceInit for #name #ty_generics #where_clause {
            async fn init(app: &foxtive::App) -> foxtive::prelude::AppResult<Self> {
                Ok(Self {
                    #(#field_init_tokens),*
                })
            }

            fn dependencies() -> Vec<&'static str> {
                vec![#(#dependency_name_exprs),*]
            }

            fn after_init(&mut self, __app: &foxtive::App) -> foxtive::prelude::AppResult<()> {
                <Self as foxtive::lifecycle::ServiceHooks>::after_init(self, __app)
            }

            fn on_ready(__app: &foxtive::App) -> foxtive::prelude::AppResult<()> {
                <Self as foxtive::lifecycle::ServiceHooks>::on_ready(__app)
            }

            #mutable_method
            #wire_lazy_method
        }
    };

    expanded
}

fn extract_lazy_inner(ty: &Type) -> Option<Type> {
    if let Type::Path(type_path) = ty {
        let segments = &type_path.path.segments;
        if let Some(last) = segments.last()
            && last.ident == "Lazy"
            && let syn::PathArguments::AngleBracketed(args) = &last.arguments
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            return Some(inner.clone());
        }
    }
    None
}

fn is_arc_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty
        && let Some(last) = type_path.path.segments.last()
    {
        return last.ident == "Arc";
    }
    false
}

fn get_type_name(ty: &Type) -> Option<String> {
    if let Type::Path(type_path) = ty
        && let Some(last) = type_path.path.segments.last()
    {
        return Some(last.ident.to_string());
    }
    None
}

fn try_generate_infra_access(
    field_type: &Type,
) -> Option<proc_macro2::TokenStream> {
    let (inner_type_name, is_arc_wrapper) = extract_type_name(field_type);

    for (infra_name, accessor, needs_clone) in INFRASTRUCTURE_TYPES {
        if inner_type_name != *infra_name {
            continue;
        }

        let accessor_ident = syn::Ident::new(accessor, proc_macro2::Span::call_site());

        return if is_arc_wrapper {
            // Arc<InfraType>: unwrap Result, clone inner, wrap in Arc
            Some(quote! {
                std::sync::Arc::new(app.#accessor_ident()?.clone())
            })
        } else if *needs_clone {
            Some(quote! {
                app.#accessor_ident().clone()
            })
        } else {
            Some(quote! {
                app.#accessor_ident()
            })
        };
    }

    None
}

fn extract_type_name(ty: &Type) -> (String, bool) {
    if let Type::Path(type_path) = ty {
        let segments = &type_path.path.segments;
        if let Some(last) = segments.last() {
            if last.ident == "Arc"
                && let syn::PathArguments::AngleBracketed(args) = &last.arguments
                    && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
                        && let Type::Path(inner_path) = inner
                            && let Some(inner_last) = inner_path.path.segments.last() {
                                return (inner_last.ident.to_string(), true);
                            }
            return (last.ident.to_string(), false);
        }
    }
    (String::new(), false)
}

fn extract_arc_inner(ty: &Type) -> (Type, bool) {
    if let Type::Path(type_path) = ty {
        let segments = &type_path.path.segments;
        if let Some(last) = segments.last()
            && last.ident == "Arc"
                && let syn::PathArguments::AngleBracketed(args) = &last.arguments
                    && let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return (inner.clone(), true);
                    }
    }
    (ty.clone(), false)
}

/// Check if the struct has `#[service(mutable)]` attribute.
fn has_service_mutable_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("service")
            && let Meta::List(meta_list) = &attr.meta
        {
            return meta_list.tokens.to_string().contains("mutable");
        }
        false
    })
}

/// Check if the struct has `#[service(all)]` attribute.
/// When present, all fields are treated as dependencies by default.
/// Fields can opt out with `#[default]`.
fn has_service_all_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("service")
            && let Meta::List(meta_list) = &attr.meta
        {
            let tokens = meta_list.tokens.to_string();
            return tokens.split(',').any(|t| t.trim() == "all");
        }
        false
    })
}

/// Check if the struct has `#[service(skip_hooks)]` attribute.
/// When present, the derive will NOT auto-generate a `ServiceHooks` impl,
/// allowing the developer to provide their own.
fn has_service_skip_hooks_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if attr.path().is_ident("service")
            && let Meta::List(meta_list) = &attr.meta
        {
            let tokens = meta_list.tokens.to_string();
            return tokens.split(',').any(|t| t.trim() == "skip_hooks");
        }
        false
    })
}

/// Extract expression from `#[foxtive(init = "expr")]`.
fn extract_init_expr(attrs: &[syn::Attribute]) -> Option<proc_macro2::TokenStream> {
    for attr in attrs {
        if attr.path().is_ident("foxtive")
            && let Meta::List(meta_list) = &attr.meta
        {
            let tokens_str = meta_list.tokens.to_string();
            if let Some(pos) = tokens_str.find("init") {
                let after_init = &tokens_str[pos + 4..];
                if let Some(eq_pos) = after_init.find('=') {
                    let value_str = after_init[eq_pos + 1..].trim();
                    let value_str = value_str.trim_matches('"');
                    if let Ok(expr) = syn::parse_str::<syn::Expr>(value_str) {
                        return Some(quote! { #expr });
                    }
                }
            }
        }
    }
    None
}
