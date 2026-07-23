use proc_macro::TokenStream;

mod enum_common;
#[cfg(feature = "database")]
mod enum_diesel;
mod enum_diesel_generate;
mod enum_generate;
mod service_init;

#[proc_macro]
pub fn generate_enum(input: TokenStream) -> TokenStream {
    enum_generate::generate_enum(input)
}

#[proc_macro]
pub fn impl_enum_common_traits(input: TokenStream) -> TokenStream {
    enum_common::impl_enum_common_traits(input)
}

#[proc_macro]
pub fn impl_enum_display_trait(input: TokenStream) -> TokenStream {
    enum_common::impl_enum_display_trait(input)
}

#[cfg(feature = "database")]
#[proc_macro]
pub fn impl_enum_diesel_traits(input: TokenStream) -> TokenStream {
    enum_diesel::impl_enum_diesel_traits(input)
}

#[cfg(feature = "database")]
#[proc_macro]
pub fn generate_diesel_enum(input: TokenStream) -> TokenStream {
    enum_diesel_generate::generate_diesel_enum(input)
}

#[proc_macro]
/// Generate Diesel enum with optional features
pub fn generate_diesel_enum_with_optional_features(input: TokenStream) -> TokenStream {
    enum_diesel_generate::generate_diesel_enum_with_optional_features(input)
}

/// Derive macro for the `Event` marker trait.
///
/// Generates an empty `impl Event for T {}` - the trait bounds
/// (`Clone + Send + Sync + 'static`) are checked by the compiler
/// on the struct's own derives.
///
/// # Example
///
/// ```ignore
/// use foxtive::events::Event;
///
/// #[derive(Event, Clone, Debug)]
/// struct UserCreated {
///     user_id: i64,
/// }
/// ```
#[proc_macro_derive(Event)]
pub fn derive_event(input: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let expanded = quote::quote! {
        impl #impl_generics foxtive::events::Event for #name #ty_generics #where_clause {}
    };

    TokenStream::from(expanded)
}

/// Derive macro for automatic dependency injection.
///
/// Generates a `ServiceInit` implementation with automatic dependency resolution
/// and lazy field wiring. Dependencies marked with `#[dependency]` are
/// automatically resolved from the app container. Non-dependency fields are
/// initialized via `Default::default()`.
///
/// - `Arc<T>` dependencies are resolved eagerly during construction.
/// - `Lazy<T>` dependencies are deferred and wired after all services are constructed.
///
/// # Mutable Services
///
/// Add `#[service(mutable)]` to register the service as `Mutable<T>`, enabling
/// shared interior mutability. Retrieve with `app.require_mutable::<T>()`.
///
/// # Example
///
/// ```ignore
/// use foxtive::lifecycle::Service;
/// use foxtive::container::Lazy;
/// use std::sync::Arc;
///
/// #[derive(Service)]
/// struct UserService {
///     #[dependency]
///     cache: Arc<CacheService>,
///
///     #[dependency]
///     payment: Lazy<PaymentService>,  // deferred - breaks cycles
/// }
///
/// // Mutable service - stored as Mutable<CounterService>
/// #[derive(Service)]
/// #[service(mutable)]
/// struct CounterService {
///     count: u64,
/// }
/// ```
#[proc_macro_derive(Service, attributes(dependency, foxtive, service))]
pub fn derive_service(input: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    TokenStream::from(service_init::derive_service_init(&ast))
}

/// Derive macro for the `FromApp` trait.
///
/// Generates a `FromApp` implementation that calls `app.require::<Self>()`
/// and clones the inner value. The type must be registered in the app container
/// and must implement `Clone`.
///
/// # Example
///
/// ```ignore
/// use foxtive::lifecycle::FromApp;
///
/// #[derive(Clone, FromApp)]
/// struct MyService {
///     // fields...
/// }
///
/// // Now MyService can be extracted automatically:
/// let svc: MyService = FromApp::from_app(&app)?;
/// ```
#[proc_macro_derive(FromApp)]
pub fn derive_from_app(input: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    let expanded = quote::quote! {
        impl #impl_generics foxtive::lifecycle::FromApp for #name #ty_generics #where_clause {
            fn from_app(app: &foxtive::App) -> foxtive::prelude::AppResult<Self> {
                Ok(app.require::<Self>()?.as_ref().clone())
            }
        }
    };

    TokenStream::from(expanded)
}
