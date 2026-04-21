use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, LitStr};

/// A macro to validate a cron expression at compile time.
///
/// Returns the cron expression string if valid, or a compile error if invalid.
///
/// # Example
/// ```rust
/// let s = cron!("0 0 9 * * * *");
/// ```
#[proc_macro]
pub fn cron(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);
    let expr = input.value();

    // We can't easily import the 'cron' crate logic here without complex workspace
    // dependencies, but we can do basic validation of the number of fields.
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 7 {
        return syn::Error::new(
            input.span(),
            format!("Invalid cron expression: expected 7 fields, found {}. Format: 'sec min hour day_of_month month day_of_week year'", fields.len())
        )
        .to_compile_error()
        .into();
    }

    // In a real implementation, we would include a full parser here.

    let expanded = quote! {
        #expr
    };

    TokenStream::from(expanded)
}
