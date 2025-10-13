use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Ident, LitStr, Token, Variant, braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// Struct to parse macro input for `generate_diesel_enum`
struct DieselEnumInput {
    enum_name: Ident,
    variants: Punctuated<Variant, Token![,]>,
}

/// Struct to parse macro input for `generate_diesel_enum_with_optional_features`
struct DieselEnumWithFeatureInput {
    feature: LitStr,
    enum_name: Ident,
    variants: Punctuated<Variant, Token![,]>,
}

impl Parse for DieselEnumInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let enum_name: Ident = input.parse()?; // Parse the enum name
        let content;
        braced!(content in input); // Parse the variants inside `{}`

        let variants = Punctuated::<Variant, Token![,]>::parse_terminated(&content)?;
        Ok(DieselEnumInput {
            enum_name,
            variants,
        })
    }
}

impl Parse for DieselEnumWithFeatureInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let feature: LitStr = input.parse()?; // Parse the feature flag
        input.parse::<Token![,]>()?;

        let enum_name: Ident = input.parse()?; // Parse the enum name
        let content;
        braced!(content in input); // Parse the variants inside `{}`

        let variants = Punctuated::<Variant, Token![,]>::parse_terminated(&content)?;
        Ok(DieselEnumWithFeatureInput {
            feature,
            enum_name,
            variants,
        })
    }
}

/// Procedural macro to generate a Diesel-compatible enum **without** feature flags
pub fn generate_diesel_enum(input: TokenStream) -> TokenStream {
    let DieselEnumInput {
        enum_name,
        variants,
    } = syn::parse_macro_input!(input as DieselEnumInput);

    let expanded = quote! {
        #[derive(diesel::AsExpression, diesel::FromSqlRow, strum_macros::EnumString, strum_macros::Display, Copy, Clone, Eq, PartialEq)]
        #[diesel(sql_type = diesel::sql_types::Text)]
        #[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
        pub enum #enum_name {
            #variants
        }

        foxtive_macros::impl_enum_common_traits!(#enum_name);
        foxtive_macros::impl_enum_diesel_traits!(#enum_name);
    };

    TokenStream::from(expanded)
}

/// Procedural macro to generate a Diesel-compatible enum **with optional feature flags**
pub fn generate_diesel_enum_with_optional_features(input: TokenStream) -> TokenStream {
    let DieselEnumWithFeatureInput {
        feature,
        enum_name,
        variants,
    } = syn::parse_macro_input!(input as DieselEnumWithFeatureInput);

    let enum_definition = quote! {
        #[cfg_attr(feature = #feature, derive(diesel::AsExpression, diesel::FromSqlRow))]
        #[cfg_attr(feature = #feature, diesel(sql_type = diesel::sql_types::Text))]
        #[derive(strum_macros::EnumString, strum_macros::Display, Copy, Clone, Eq, PartialEq)]
        #[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
        pub enum #enum_name {
            #variants
        }
    };

    let common_traits = quote! {
        foxtive_macros::impl_enum_common_traits!(#enum_name);
    };

    let diesel_traits = quote! {
        #[cfg(feature = #feature)]
        foxtive_macros::impl_enum_diesel_traits!(#enum_name);
    };

    let expanded = quote! {
        #enum_definition
        #common_traits
        #diesel_traits
    };

    TokenStream::from(expanded)
}
