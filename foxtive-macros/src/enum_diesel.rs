use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident};

pub fn impl_enum_diesel_traits(input: TokenStream) -> TokenStream {
    let variant_name = parse_macro_input!(input as Ident);

    let expanded = quote! {
        impl diesel::deserialize::FromSql<diesel::sql_types::Text, diesel::pg::Pg> for #variant_name {
            fn from_sql(bytes: diesel::pg::PgValue) -> diesel::deserialize::Result<Self> {
                use std::str::FromStr;

                let value = <String as diesel::deserialize::FromSql<
                    diesel::sql_types::Text,
                    diesel::pg::Pg,
                >>::from_sql(bytes)?;
                match #variant_name::from_str(value.as_str()) {
                    Ok(variant) => Ok(variant),
                    Err(err) => Err(err.to_string().into()),
                }
            }
        }

        impl diesel::serialize::ToSql<diesel::sql_types::Text, diesel::pg::Pg> for #variant_name {
            fn to_sql(
                &self,
                out: &mut diesel::serialize::Output<diesel::pg::Pg>,
            ) -> diesel::serialize::Result {
                use std::io::Write;

                let value = self.to_string();
                out.write_all(value.as_bytes())?;
                Ok(diesel::serialize::IsNull::No)
            }
        }
    };

    expanded.into()
}
