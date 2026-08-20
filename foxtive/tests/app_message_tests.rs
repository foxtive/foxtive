mod common;

use foxtive::enums::AppMessage;
use http::StatusCode;
use std::collections::HashMap;

#[test]
fn success_maps_to_200() {
    let msg = AppMessage::success("ok");
    assert_eq!(msg.status_code(), StatusCode::OK);
    assert!(msg.is_success());
    assert!(!msg.is_error());
    assert_eq!(msg.message(), "ok");
}

#[test]
fn redirect_maps_to_302() {
    let msg = AppMessage::redirect("https://example.com");
    assert_eq!(msg.status_code(), StatusCode::FOUND);
    assert!(msg.is_redirect());
    assert!(!msg.is_error());
    assert!(!msg.is_success());
}

#[test]
fn invalid_maps_to_400() {
    let msg = AppMessage::invalid("bad input");
    assert_eq!(msg.status_code(), StatusCode::BAD_REQUEST);
    assert!(msg.is_client_error());
    assert!(msg.is_error());
}

#[test]
fn unauthorized_maps_to_401() {
    let msg = AppMessage::unauthorized("no token");
    assert_eq!(msg.status_code(), StatusCode::UNAUTHORIZED);
    assert!(msg.is_client_error());
}

#[test]
fn forbidden_maps_to_403() {
    let msg = AppMessage::forbidden("no access");
    assert_eq!(msg.status_code(), StatusCode::FORBIDDEN);
    assert!(msg.is_client_error());
}

#[test]
fn not_found_maps_to_404() {
    let msg = AppMessage::not_found("missing");
    assert_eq!(msg.status_code(), StatusCode::NOT_FOUND);
    assert!(msg.is_client_error());
}

#[test]
fn conflict_maps_to_409() {
    let msg = AppMessage::conflict("duplicate");
    assert_eq!(msg.status_code(), StatusCode::CONFLICT);
    assert!(msg.is_client_error());
}

#[test]
fn unprocessable_entity_maps_to_422() {
    let msg = AppMessage::unprocessable_entity("invalid payload");
    assert_eq!(msg.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(msg.is_client_error());
}

#[test]
fn internal_server_error_maps_to_500() {
    let msg = AppMessage::internal_server_error("boom");
    assert_eq!(msg.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(msg.is_server_error());
    assert!(msg.is_error());
}

#[test]
fn error_message_uses_explicit_status() {
    let msg = AppMessage::error_message("gone", StatusCode::GONE);
    assert_eq!(msg.status_code(), StatusCode::GONE);
    assert_eq!(msg.message(), "gone");
}

#[test]
fn validation_error_carries_field_details() {
    let mut errors: HashMap<String, Vec<String>> = HashMap::new();
    errors.insert("email".into(), vec!["required".into()]);
    errors.insert("name".into(), vec!["too short".into()]);

    let msg = AppMessage::validation_error("Validation failed", errors);
    assert_eq!(msg.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(msg.is_client_error());
    assert_eq!(msg.message(), "Validation failed");

    let field_errors = msg.validation_errors().unwrap();
    assert_eq!(field_errors["email"], vec!["required"]);
    assert_eq!(field_errors["name"], vec!["too short"]);
}

#[test]
fn validation_errors_returns_none_for_non_validation_variants() {
    assert!(AppMessage::success("ok").validation_errors().is_none());
    assert!(AppMessage::not_found("x").validation_errors().is_none());
    assert!(
        AppMessage::internal_server_error("x")
            .validation_errors()
            .is_none()
    );
}

#[test]
fn infrastructure_variant_with_source() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let msg = AppMessage::Infrastructure {
        message: format!("IO error: {io_err}"),
        source: Some(Box::new(io_err)),
    };

    assert_eq!(msg.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(msg.is_server_error());
    assert_eq!(msg.kind_name(), "infrastructure");
    assert!(msg.message().contains("IO error"));
    assert!(msg.message().contains("file missing"));
}

#[test]
fn infrastructure_variant_without_source() {
    let msg = AppMessage::Infrastructure {
        message: "something broke".to_string(),
        source: None,
    };
    assert_eq!(msg.message(), "something broke");
    assert_eq!(msg.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn missing_environment_variable_maps_to_500() {
    let msg =
        AppMessage::missing_environment_variable("DATABASE_URL", std::env::VarError::NotPresent);
    assert_eq!(msg.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(msg.is_server_error());
    assert!(msg.message().contains("DATABASE_URL"));
}

#[test]
fn kind_name_returns_stable_identifiers() {
    assert_eq!(AppMessage::success("").kind_name(), "success");
    assert_eq!(AppMessage::redirect("").kind_name(), "redirect");
    assert_eq!(AppMessage::invalid("").kind_name(), "invalid");
    assert_eq!(AppMessage::unauthorized("").kind_name(), "unauthorized");
    assert_eq!(AppMessage::forbidden("").kind_name(), "forbidden");
    assert_eq!(AppMessage::not_found("").kind_name(), "not_found");
    assert_eq!(AppMessage::conflict("").kind_name(), "conflict");
    assert_eq!(
        AppMessage::unprocessable_entity("").kind_name(),
        "unprocessable_entity"
    );
    assert_eq!(
        AppMessage::internal_server_error("").kind_name(),
        "internal_server_error"
    );
}

#[test]
fn into_result_always_returns_err() {
    let result = AppMessage::not_found("gone").into_result::<String>();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().message(), "gone");
}

#[test]
fn display_trait_returns_message() {
    let msg = AppMessage::success("hello world");
    assert_eq!(format!("{msg}"), "hello world");
}

#[test]
fn is_error_covers_both_client_and_server() {
    assert!(AppMessage::not_found("x").is_error());
    assert!(AppMessage::internal_server_error("x").is_error());
    assert!(!AppMessage::success("x").is_error());
    assert!(!AppMessage::redirect("x").is_error());
}

#[test]
fn empty_messages_are_allowed() {
    let msg = AppMessage::success("");
    assert_eq!(msg.message(), "");
    assert!(msg.is_success());
}
