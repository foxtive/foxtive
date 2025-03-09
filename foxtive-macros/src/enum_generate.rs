use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemEnum};

pub fn generate_enum(input: TokenStream) -> TokenStream {
    let enum_def = parse_macro_input!(input as ItemEnum);
    let enum_name = &enum_def.ident;
    let variants = &enum_def.variants;

    let expanded = quote! {
        #[derive(strum_macros::EnumString, strum_macros::Display, Clone, Eq, PartialEq)]
        #[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
        pub enum #enum_name {
            #variants
        }

        foxtive::impl_enum_common_traits!(#enum_name);
    };

    expanded.into()
}
