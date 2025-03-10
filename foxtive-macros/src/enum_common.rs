use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident};

pub fn impl_enum_common_traits(input: TokenStream) -> TokenStream {
    let variant_name = parse_macro_input!(input as Ident);

    let expanded = quote! {
        impl std::fmt::Debug for #variant_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.to_string())
            }
        }

        impl serde::Serialize for #variant_name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> serde::Deserialize<'de> for #variant_name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct EnumVisitor;

                impl<'de> serde::de::Visitor<'de> for EnumVisitor {
                    type Value = #variant_name;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str("a valid variant string")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<#variant_name, E>
                    where
                        E: serde::de::Error,
                    {
                        use std::str::FromStr;

                        #variant_name::from_str(value).map_err(|_| {
                            serde::de::Error::invalid_value(
                                serde::de::Unexpected::Str(value),
                                &self,
                            )
                        })
                    }
                }

                deserializer.deserialize_str(EnumVisitor)
            }
        }
    };

    expanded.into()
}

pub fn impl_enum_display_trait(input: TokenStream) -> TokenStream {
    let variant_name = parse_macro_input!(input as Ident);

    let expanded = quote! {
        impl std::fmt::Display for #variant_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };

    expanded.into()
}
