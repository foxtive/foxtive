mod common;

use common::Environment;

#[test]
fn serde_round_trip_all_variants() {
    let envs = [
        Environment::Local,
        Environment::Development,
        Environment::Staging,
        Environment::Production,
    ];
    for env in &envs {
        let json = serde_json::to_string(env).unwrap();
        let deserialized: Environment = serde_json::from_str(&json).unwrap();
        assert_eq!(*env, deserialized);
    }
}

#[test]
fn from_str_accepts_full_names() {
    assert_eq!("local".parse::<Environment>().unwrap(), Environment::Local);
    assert_eq!("development".parse::<Environment>().unwrap(), Environment::Development);
    assert_eq!("staging".parse::<Environment>().unwrap(), Environment::Staging);
    assert_eq!("production".parse::<Environment>().unwrap(), Environment::Production);
}

#[test]
fn from_str_accepts_common_aliases() {
    assert_eq!("dev".parse::<Environment>().unwrap(), Environment::Development);
    assert_eq!("stage".parse::<Environment>().unwrap(), Environment::Staging);
    assert_eq!("prod".parse::<Environment>().unwrap(), Environment::Production);
}

#[test]
fn from_str_rejects_garbage() {
    assert!("invalid".parse::<Environment>().is_err());
    assert!("".parse::<Environment>().is_err());
    assert!("dev ".parse::<Environment>().is_err());
    assert!("123".parse::<Environment>().is_err());
}

#[test]
fn production_is_production() {
    let env = Environment::Production;
    assert!(env.is_production());
    assert!(!env.is_dev_like());
    assert!(!env.is_local());
    assert!(!env.allows_debug());
}

#[test]
fn local_allows_debug() {
    let env = Environment::Local;
    assert!(env.is_dev_like());
    assert!(env.is_local());
    assert!(env.allows_debug());
    assert!(!env.is_production());
}

#[test]
fn development_is_dev_like_but_not_local() {
    let env = Environment::Development;
    assert!(env.is_dev_like());
    assert!(!env.is_local());
    assert!(env.allows_debug());
}

#[test]
fn staging_is_neither_dev_nor_local() {
    let env = Environment::Staging;
    assert!(!env.is_dev_like());
    assert!(!env.is_local());
    assert!(!env.is_production());
}

#[test]
fn default_environment_is_production() {
    assert_eq!(Environment::default(), Environment::Production);
}

#[test]
fn display_formatting() {
    let env = Environment::Production;
    let display = format!("{env}");
    assert!(!display.is_empty());
}
