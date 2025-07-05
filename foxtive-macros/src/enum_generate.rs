use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Ident, Token, Variant, braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// Struct to parse macro input
struct EnumInput {
    enum_name: Ident,
    variants: Punctuated<Variant, Token![,]>,
}

impl Parse for EnumInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let enum_name: Ident = input.parse()?; // Parse the enum name
        let content;
        braced!(content in input); // Parse variants inside `{}`

        let variants = Punctuated::<Variant, Token![,]>::parse_terminated(&content)?;
        Ok(EnumInput {
            enum_name,
            variants,
        })
    }
}

/// Procedural macro to generate a simple Rust enum with Strum traits
pub fn generate_enum(input: TokenStream) -> TokenStream {
    let EnumInput {
        enum_name,
        variants,
    } = syn::parse_macro_input!(input as EnumInput);

    let expanded = quote! {
        #[derive(strum_macros::EnumString, strum_macros::Display, Clone, Eq, PartialEq)]
        #[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
        pub enum #enum_name {
            #variants
        }

        foxtive_macros::impl_enum_common_traits!(#enum_name);
    };

    TokenStream::from(expanded)
}
