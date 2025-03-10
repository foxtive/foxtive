use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemEnum};

pub fn generate_diesel_enum(input: TokenStream) -> TokenStream {
    let enum_ast = parse_macro_input!(input as ItemEnum);
    let enum_name = &enum_ast.ident;
    let variants = &enum_ast.variants;

    let expanded = quote! {
        #[derive(diesel::AsExpression, diesel::FromSqlRow, strum_macros::EnumString, strum_macros::Display, Clone, Eq, PartialEq)]
        #[diesel(sql_type = diesel::sql_types::Text)]
        #[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
        pub enum #enum_name {
            #variants
        }

        foxtive_macros::impl_enum_common_traits!(#enum_name);
        foxtive_macros::impl_enum_diesel_traits!(#enum_name);
    };

    expanded.into()
}

// pub fn generate_diesel_enum_with_optional_features(input: TokenStream) -> TokenStream {
//     let input_args = parse_macro_input!(input as syn::AttributeArgs);
//
//     // Extract feature name (first argument)
//     let feature_name = match input_args.first() {
//         Some(Meta::NameValue(MetaNameValue {
//             lit: Lit::Str(lit), ..
//         })) => lit.value(),
//         _ => panic!("Expected a string literal for the feature name"),
//     };
//
//     // Extract enum definition (remaining arguments)
//     let enum_def: ItemEnum =
//         syn::parse2(quote! { #(#input_args[1..])* }).expect("Expected a valid enum definition");
//
//     let enum_name = &enum_def.ident;
//     let variants = &enum_def.variants;
//
//     let expanded = quote! {
//         #[cfg_attr(feature = #feature_name, derive(diesel::AsExpression, diesel::FromSqlRow))]
//         #[cfg_attr(feature = #feature_name, diesel(sql_type = diesel::sql_types::Text))]
//         #[derive(strum_macros::EnumString, strum_macros::Display, Clone, Eq, PartialEq)]
//         #[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
//         pub enum #enum_name {
//             #variants
//         }
//
//         foxtive_macros::impl_enum_common_traits!(#enum_name);
//
//         #[cfg(feature = #feature_name)]
//         foxtive_macros::impl_enum_diesel_traits!(#enum_name);
//     };
//
//     expanded.into()
// }
