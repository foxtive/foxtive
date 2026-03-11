/// Creates a `not_found` error with optional format arguments.
///
/// ```
/// use foxtive::not_found;
///
/// return Err(not_found!("User {} was not found", user_id));
/// ```
#[macro_export]
macro_rules! not_found {
    ($($arg:tt)*) => {
        anyhow::Error::from($crate::prelude::AppMessage::not_found(format!($($arg)*)))
    };
}

/// Creates an `unauthorized` error with optional format arguments.
///
/// ```
/// use foxtive::unauthorized;
///
/// return Err(unauthorized!("Token {} has expired", token_id));
/// ```
#[macro_export]
macro_rules! unauthorized {
    ($($arg:tt)*) => {
        anyhow::Error::from($crate::prelude::AppMessage::unauthorized(format!($($arg)*)))
    };
}

/// Creates a `forbidden` error with optional format arguments.
///
/// ```
/// use foxtive::forbidden;
///
/// return Err(forbidden!("User {} lacks permission", user_id));
/// ```
#[macro_export]
macro_rules! forbidden {
    ($($arg:tt)*) => {
        anyhow::Error::from($crate::prelude::AppMessage::forbidden(format!($($arg)*)))
    };
}

/// Creates a `bad_request` error with optional format arguments.
///
/// ```
/// use foxtive::bad_request;
///
/// return Err(bad_request!("Field '{}' is invalid", field));
/// ```
#[macro_export]
macro_rules! bad_request {
    ($($arg:tt)*) => {
        anyhow::Error::from($crate::prelude::AppMessage::invalid(format!($($arg)*)))
    };
}

/// Alias for [`bad_request!`].
///
/// ```
/// use foxtive::invalid;
///
/// return Err(invalid!("Field '{}' is invalid", field));
/// ```
#[macro_export]
macro_rules! invalid {
    ($($arg:tt)*) => {
        $crate::bad_request!($($arg)*)
    };
}

/// Creates a `conflict` error with optional format arguments.
///
/// ```
/// use foxtive::conflict;
///
/// return Err(conflict!("Email {} is already in use", email));
/// ```
#[macro_export]
macro_rules! conflict {
    ($($arg:tt)*) => {
        anyhow::Error::from($crate::prelude::AppMessage::conflict(format!($($arg)*)))
    };
}

/// Creates an `unprocessable_entity` error with optional format arguments.
///
/// ```
/// use foxtive::unprocessable_entity;
///
/// return Err(unprocessable_entity!("Payload missing field '{}'", field));
/// ```
#[macro_export]
macro_rules! unprocessable_entity {
    ($($arg:tt)*) => {
        anyhow::Error::from($crate::prelude::AppMessage::unprocessable_entity(format!($($arg)*)))
    };
}

/// Creates an `internal_server_error` error with optional format arguments.
///
/// ```
/// use foxtive::internal_server_error;
///
/// return Err(internal_server_error!("Unexpected failure at step {}", step));
/// ```
#[macro_export]
macro_rules! internal_server_error {
    ($($arg:tt)*) => {
        anyhow::Error::from($crate::prelude::AppMessage::internal_server_error(format!($($arg)*)))
    };
}

/// Creates a `validation_error` (422) with a message and per-field errors.
///
/// Accepts either a pre-built `ValidationErrors` map, or an inline list of
/// `"field" => ["error", ...]` pairs for convenience.
///
/// ```
/// use foxtive::validation_error;
///
/// // Pre-built map
/// return Err(validation_error!("Validation failed", errors));
///
/// // Inline
/// return Err(validation_error!("Validation failed", {
///     "email" => ["is required", "must be valid"],
///     "name"  => ["is too short"],
/// }));
/// ```
#[macro_export]
macro_rules! validation_error {
    // Inline form: validation_error!("msg", { "field" => ["e1", "e2"], ... })
    ($msg:expr, { $($field:expr => [$($err:expr),* $(,)?]),* $(,)? }) => {{
        let mut errors = $crate::ValidationErrors::new();
        $(
            errors.insert($field.to_string(), vec![$($err.to_string()),*]);
        )*
        anyhow::Error::from($crate::prelude::AppMessage::validation_error(format!($msg), errors))
    }};

    // Pre-built map form: validation_error!("msg", errors_map)
    ($msg:expr, $errors:expr) => {
        anyhow::Error::from($crate::prelude::AppMessage::validation_error(format!($msg), $errors))
    };
}

/// Asserts a condition is true, otherwise returns a `bad_request` error.
/// Useful for lightweight guard clauses at the top of service functions.
///
/// ```
/// use foxtive::ensure;
///
/// ensure!(age >= 18, "User must be at least 18, got {}", age);
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
/// ```
/// use foxtive::ensure_found;
///
/// let user = ensure_found!(db.find_user(id), "User {} not found", id);
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
    use http::StatusCode;
    use crate::enums::AppMessage;

    fn downcast(err: &anyhow::Error) -> &AppMessage {
        err.downcast_ref::<AppMessage>().unwrap()
    }

    #[test]
    fn test_basic_macros() {
        let err = invalid!("Pin must be 6 digits");
        assert_eq!(err.to_string(), "Pin must be 6 digits");
        assert_eq!(downcast(&err).status_code(), StatusCode::BAD_REQUEST);

        let err = not_found!("User {} was not found", 42);
        assert_eq!(err.to_string(), "User 42 was not found");
        assert_eq!(downcast(&err).status_code(), StatusCode::NOT_FOUND);

        let err = unauthorized!("Token {} expired", "abc");
        assert_eq!(err.to_string(), "Token abc expired");
        assert_eq!(downcast(&err).status_code(), StatusCode::UNAUTHORIZED);

        let err = forbidden!("Role {} not allowed", "guest");
        assert_eq!(err.to_string(), "Role guest not allowed");
        assert_eq!(downcast(&err).status_code(), StatusCode::FORBIDDEN);

        let err = conflict!("Email {} is already taken", "a@b.com");
        assert_eq!(err.to_string(), "Email a@b.com is already taken");
        assert_eq!(downcast(&err).status_code(), StatusCode::CONFLICT);

        let err = unprocessable_entity!("Cannot process request");
        assert_eq!(downcast(&err).status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let err = internal_server_error!("Crashed on line {}", 99);
        assert_eq!(err.to_string(), "Crashed on line 99");
        assert_eq!(downcast(&err).status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_bad_request_and_invalid_are_equivalent() {
        let a = bad_request!("bad input");
        let b = invalid!("bad input");
        assert_eq!(a.to_string(), b.to_string());
        assert_eq!(downcast(&a).status_code(), downcast(&b).status_code());
    }

    #[test]
    fn test_validation_error_inline() {
        let err = validation_error!("Validation failed", {
            "email" => ["is required", "must be a valid email"],
            "name"  => ["is too short"],
        });

        assert_eq!(err.to_string(), "Validation failed");
        let msg = downcast(&err);
        assert_eq!(msg.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let errors = msg.validation_errors().unwrap();
        assert_eq!(errors["email"], vec!["is required", "must be a valid email"]);
        assert_eq!(errors["name"], vec!["is too short"]);
    }

    #[test]
    fn test_validation_error_prebuilt_map() {
        let mut map = crate::ValidationErrors::new();
        map.insert("phone".into(), vec!["is invalid".into()]);

        let err = validation_error!("Validation failed", map);
        let msg = downcast(&err);
        assert_eq!(msg.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(msg.validation_errors().unwrap()["phone"], vec!["is invalid"]);
    }

    #[test]
    fn test_ensure_passes_and_fails() {
        fn check(age: u32) -> anyhow::Result<()> {
            ensure!(age >= 18, "Must be at least 18, got {}", age);
            Ok(())
        }

        assert!(check(18).is_ok());
        assert!(check(21).is_ok());

        let err = check(16).unwrap_err();
        assert_eq!(err.to_string(), "Must be at least 18, got 16");
        assert_eq!(downcast(&err).status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_ensure_found() {
        fn find(val: Option<u32>) -> anyhow::Result<u32> {
            let v = ensure_found!(val, "Item not found");
            Ok(v)
        }

        assert_eq!(find(Some(42)).unwrap(), 42);

        let err = find(None).unwrap_err();
        assert_eq!(err.to_string(), "Item not found");
        assert_eq!(downcast(&err).status_code(), StatusCode::NOT_FOUND);
    }
}