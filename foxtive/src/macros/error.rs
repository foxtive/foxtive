/// Generates `From<ErrorType> for AppMessage` impls that wrap into Infrastructure.
///
/// Usage:
///   impl_from_infra!(std::io::Error => "IO error");
///   impl_from_infra!(serde_json::Error => "Serialization error");
#[macro_export]
macro_rules! impl_from_infra {
    ($($ty:ty => $msg:expr),* $(,)?) => {
        $(
            impl From<$ty> for $crate::prelude::AppMessage {
                fn from(e: $ty) -> Self {
                    $crate::prelude::AppMessage::Infrastructure {
                        message: format!("{}: {}", $msg, e),
                        source: Some(Box::new(e)),
                    }
                }
            }
        )*
    };
}

/// Creates a `not_found` error with optional format arguments.
///
/// ```no_run
/// use foxtive::not_found;
/// use foxtive::prelude::AppResult;
///
/// fn example(user_id: u64) -> AppResult<()> {
///     return Err(not_found!("User {} was not found", user_id));
/// }
/// ```
#[macro_export]
macro_rules! not_found {
    ($($arg:tt)*) => {
        $crate::prelude::AppMessage::not_found(format!($($arg)*))
    };
}

/// Creates an `unauthorized` error with optional format arguments.
///
/// ```no_run
/// use foxtive::unauthorized;
/// use foxtive::prelude::AppResult;
///
/// fn example(token_id: &str) -> AppResult<()> {
///     return Err(unauthorized!("Token {} has expired", token_id));
/// }
/// ```
#[macro_export]
macro_rules! unauthorized {
    ($($arg:tt)*) => {
        $crate::prelude::AppMessage::unauthorized(format!($($arg)*))
    };
}

/// Creates a `forbidden` error with optional format arguments.
///
/// ```no_run
/// use foxtive::forbidden;
/// use foxtive::prelude::AppResult;
///
/// fn example(user_id: u64) -> AppResult<()> {
///     return Err(forbidden!("User {} lacks permission", user_id));
/// }
/// ```
#[macro_export]
macro_rules! forbidden {
    ($($arg:tt)*) => {
        $crate::prelude::AppMessage::forbidden(format!($($arg)*))
    };
}

/// Creates a `bad_request` error with optional format arguments.
///
/// ```no_run
/// use foxtive::bad_request;
/// use foxtive::prelude::AppResult;
///
/// fn example(field: &str) -> AppResult<()> {
///     return Err(bad_request!("Field '{}' is invalid", field));
/// }
/// ```
#[macro_export]
macro_rules! bad_request {
    ($($arg:tt)*) => {
        $crate::prelude::AppMessage::invalid(format!($($arg)*))
    };
}

/// Alias for [`bad_request!`].
///
/// ```no_run
/// use foxtive::invalid;
/// use foxtive::prelude::AppResult;
///
/// fn example(field: &str) -> AppResult<()> {
///     return Err(invalid!("Field '{}' is invalid", field));
/// }
/// ```
#[macro_export]
macro_rules! invalid {
    ($($arg:tt)*) => {
        $crate::bad_request!($($arg)*)
    };
}

/// Creates a `conflict` error with optional format arguments.
///
/// ```no_run
/// use foxtive::conflict;
/// use foxtive::prelude::AppResult;
///
/// fn example(email: &str) -> AppResult<()> {
///     return Err(conflict!("Email {} is already in use", email));
/// }
/// ```
#[macro_export]
macro_rules! conflict {
    ($($arg:tt)*) => {
        $crate::prelude::AppMessage::conflict(format!($($arg)*))
    };
}

/// Creates an `unprocessable_entity` error with optional format arguments.
///
/// ```no_run
/// use foxtive::unprocessable_entity;
/// use foxtive::prelude::AppResult;
///
/// fn example(field: &str) -> AppResult<()> {
///     return Err(unprocessable_entity!("Payload missing field '{}'", field));
/// }
/// ```
#[macro_export]
macro_rules! unprocessable_entity {
    ($($arg:tt)*) => {
        $crate::prelude::AppMessage::unprocessable_entity(format!($($arg)*))
    };
}

/// Creates an `internal_server_error` error with optional format arguments.
///
/// ```no_run
/// use foxtive::internal_server_error;
/// use foxtive::prelude::AppResult;
///
/// fn example(step: u32) -> AppResult<()> {
///     return Err(internal_server_error!("Unexpected failure at step {}", step));
/// }
/// ```
#[macro_export]
macro_rules! internal_server_error {
    ($($arg:tt)*) => {
        $crate::prelude::AppMessage::internal_server_error(format!($($arg)*))
    };
}

/// Creates a `validation_error` (422) with a message and per-field errors.
///
/// Accepts either a pre-built `ValidationErrors` map, or an inline list of
/// `"field" => ["error", ...]` pairs for convenience.
///
/// ```no_run
/// use foxtive::{validation_error, ValidationErrors};
/// use foxtive::prelude::AppResult;
///
/// fn example() -> AppResult<()> {
///     return Err(validation_error!("Validation failed", {
///         "email" => ["is required", "must be valid"],
///         "name"  => ["is too short"],
///     }));
/// }
///
/// fn example_prebuilt(errors: ValidationErrors) -> AppResult<()> {
///     return Err(validation_error!("Validation failed", errors));
/// }
/// ```
#[macro_export]
macro_rules! validation_error {
    // Inline form: validation_error!("msg", { "field" => ["e1", "e2"], ... })
    ($msg:expr, { $($field:expr => [$($err:expr),* $(,)?]),* $(,)? }) => {{
        let mut errors = std::collections::HashMap::<String, Vec<String>>::new();
        $(
            errors.insert($field.to_string(), vec![$($err.to_string()),*]);
        )*
        $crate::prelude::AppMessage::validation_error(format!($msg), errors)
    }};

    // Pre-built map form: validation_error!("msg", errors_map)
    ($msg:expr, $errors:expr) => {
        $crate::prelude::AppMessage::validation_error(format!($msg), $errors)
    };
}

/// Asserts a condition is true, otherwise returns a `bad_request` error.
/// Useful for lightweight guard clauses at the top of service functions.
///
/// ```no_run
/// use foxtive::ensure;
/// use foxtive::prelude::AppResult;
///
/// fn example(age: u32) -> AppResult<()> {
///     ensure!(age >= 18, "User must be at least 18, got {}", age);
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond {
            return Err($crate::bad_request!($($arg)*));
        }
    };
}

/// Unwraps an `Option`, returning a `not_found` error if `None`.
///
/// ```no_run
/// use foxtive::ensure_found;
/// use foxtive::prelude::AppResult;
///
/// fn example(value: Option<u64>) -> AppResult<u64> {
///     let v = ensure_found!(value, "Item not found");
///     Ok(v)
/// }
/// ```
#[macro_export]
macro_rules! ensure_found {
    ($option:expr, $($arg:tt)*) => {
        match $option {
            Some(val) => val,
            None => return Err($crate::not_found!($($arg)*)),
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::prelude::AppResult;
    use http::StatusCode;

    #[test]
    fn test_basic_macros() {
        let err = invalid!("Pin must be 6 digits");
        assert_eq!(err.message(), "Pin must be 6 digits");
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);

        let err = not_found!("User {} was not found", 42);
        assert_eq!(err.message(), "User 42 was not found");
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);

        let err = unauthorized!("Token {} expired", "abc");
        assert_eq!(err.message(), "Token abc expired");
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);

        let err = forbidden!("Role {} not allowed", "guest");
        assert_eq!(err.message(), "Role guest not allowed");
        assert_eq!(err.status_code(), StatusCode::FORBIDDEN);

        let err = conflict!("Email {} is already taken", "a@b.com");
        assert_eq!(err.message(), "Email a@b.com is already taken");
        assert_eq!(err.status_code(), StatusCode::CONFLICT);

        let err = unprocessable_entity!("Cannot process request");
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let err = internal_server_error!("Crashed on line {}", 99);
        assert_eq!(err.message(), "Crashed on line 99");
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_bad_request_and_invalid_are_equivalent() {
        let a = bad_request!("bad input");
        let b = invalid!("bad input");
        assert_eq!(a.message(), b.message());
        assert_eq!(a.status_code(), b.status_code());
    }

    #[test]
    fn test_validation_error_inline() {
        let err = validation_error!("Validation failed", {
            "email" => ["is required", "must be a valid email"],
            "name"  => ["is too short"],
        });

        assert_eq!(err.message(), "Validation failed");
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let errors = err.validation_errors().unwrap();
        assert_eq!(
            errors["email"],
            vec!["is required", "must be a valid email"]
        );
        assert_eq!(errors["name"], vec!["is too short"]);
    }

    #[test]
    fn test_validation_error_prebuilt_map() {
        let mut map = std::collections::HashMap::<String, Vec<String>>::new();
        map.insert("phone".into(), vec!["is invalid".into()]);

        let err = validation_error!("Validation failed", map);
        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            err.validation_errors().unwrap()["phone"],
            vec!["is invalid"]
        );
    }

    #[test]
    fn test_ensure_passes_and_fails() {
        fn check(age: u32) -> AppResult<()> {
            ensure!(age >= 18, "Must be at least 18, got {}", age);
            Ok(())
        }

        assert!(check(18).is_ok());
        assert!(check(21).is_ok());

        let err = check(16).unwrap_err();
        assert_eq!(err.message(), "Must be at least 18, got 16");
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_ensure_found() {
        fn find(val: Option<u32>) -> AppResult<u32> {
            let v = ensure_found!(val, "Item not found");
            Ok(v)
        }

        assert_eq!(find(Some(42)).unwrap(), 42);

        let err = find(None).unwrap_err();
        assert_eq!(err.message(), "Item not found");
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }
}
