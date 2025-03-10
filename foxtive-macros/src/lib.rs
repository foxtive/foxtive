use proc_macro::TokenStream;

mod enum_common;
#[cfg(feature = "database")]
mod enum_diesel;
#[cfg(feature = "database")]
mod enum_diesel_generate;
mod enum_generate;

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

#[cfg(feature = "database")]
#[proc_macro]
pub fn generate_diesel_enum_with_optional_features(input: TokenStream) -> TokenStream {
    enum_diesel_generate::generate_diesel_enum_with_optional_features(input)
}
