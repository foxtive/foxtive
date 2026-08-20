//! Application environment detection.
//!
//! Provides the [`Environment`] enum (`Local`, `Development`, `Staging`, `Production`)
//! with parsing from strings, serde support, and convenience predicates.

use crate::enums::AppMessage;
use crate::prelude::AppResult;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Environment {
    Local,
    Development,
    Staging,
    #[default]
    Production,
    Test,
}

impl Environment {
    /// Returns the string representation of the environment
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Development => "development",
            Environment::Staging => "staging",
            Environment::Production => "production",
            Environment::Test => "test",
        }
    }

    /// Returns the abbreviated form of the environment
    pub fn as_short_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Development => "dev",
            Environment::Staging => "staging",
            Environment::Production => "prod",
            Environment::Test => "test",
        }
    }

    /// Checks if the environment is production
    pub fn is_production(&self) -> bool {
        matches!(self, Environment::Production)
    }

    /// Checks if the environment is local development
    pub fn is_local(&self) -> bool {
        matches!(self, Environment::Local)
    }

    /// Checks if the environment is a development-like environment (local or dev)
    pub fn is_dev_like(&self) -> bool {
        matches!(
            self,
            Environment::Local | Environment::Development | Environment::Test
        )
    }

    /// Checks if the environment allows debug features
    pub fn allows_debug(&self) -> bool {
        !self.is_production()
    }

    /// Gets the environment from environment variable or returns default
    pub fn from_env(var_name: &str) -> AppResult<Environment> {
        std::env::var(var_name)
            .map_err(|e| AppMessage::MissingEnvironmentVariable(var_name.to_string(), e))
            .and_then(|val: String| {
                val.parse()
                    .map_err(|e: String| AppMessage::InternalServerError(e))
            })
    }

    /// Gets the environment from environment variable or returns default
    pub fn from_env_or_default(var_name: &str, default: Environment) -> Environment {
        std::env::var(var_name)
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(default)
    }

    /// Gets all possible environment variants
    pub fn all() -> &'static [Environment] {
        &[
            Environment::Local,
            Environment::Development,
            Environment::Staging,
            Environment::Production,
            Environment::Test,
        ]
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Environment {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("local") {
            return Ok(Environment::Local);
        }
        if s.eq_ignore_ascii_case("development") || s.eq_ignore_ascii_case("dev") {
            return Ok(Environment::Development);
        }
        if s.eq_ignore_ascii_case("staging") || s.eq_ignore_ascii_case("stage") {
            return Ok(Environment::Staging);
        }
        if s.eq_ignore_ascii_case("production") || s.eq_ignore_ascii_case("prod") {
            return Ok(Environment::Production);
        }
        if s.eq_ignore_ascii_case("test") {
            return Ok(Environment::Test);
        }
        Err(format!(
            "Invalid environment value: '{s}'. Valid values are: local, development (dev), staging (stage), production (prod), test"
        ))
    }
}

mod serde_impl {
    use super::Environment;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::str::FromStr;

    impl Serialize for Environment {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_str(self.as_str())
        }
    }

    impl<'de> Deserialize<'de> for Environment {
        fn deserialize<D>(deserializer: D) -> Result<Environment, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s = String::deserialize(deserializer)?;
            Environment::from_str(&s).map_err(serde::de::Error::custom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_str() {
        assert_eq!(Environment::Local.as_str(), "local");
        assert_eq!(Environment::Development.as_str(), "development");
        assert_eq!(Environment::Staging.as_str(), "staging");
        assert_eq!(Environment::Production.as_str(), "production");
    }

    #[test]
    fn test_as_short_str() {
        assert_eq!(Environment::Local.as_short_str(), "local");
        assert_eq!(Environment::Development.as_short_str(), "dev");
        assert_eq!(Environment::Staging.as_short_str(), "staging");
        assert_eq!(Environment::Production.as_short_str(), "prod");
    }

    #[test]
    fn test_is_production() {
        assert!(!Environment::Local.is_production());
        assert!(!Environment::Development.is_production());
        assert!(!Environment::Staging.is_production());
        assert!(Environment::Production.is_production());
    }

    #[test]
    fn test_is_dev_like() {
        assert!(Environment::Local.is_dev_like());
        assert!(Environment::Development.is_dev_like());
        assert!(!Environment::Staging.is_dev_like());
        assert!(!Environment::Production.is_dev_like());
    }
    #[test]
    fn test_from_str() {
        assert_eq!("local".parse::<Environment>().unwrap(), Environment::Local);
        assert_eq!(
            "development".parse::<Environment>().unwrap(),
            Environment::Development
        );
        assert_eq!(
            "dev".parse::<Environment>().unwrap(),
            Environment::Development
        );
        assert_eq!(
            "staging".parse::<Environment>().unwrap(),
            Environment::Staging
        );
        assert_eq!(
            "stage".parse::<Environment>().unwrap(),
            Environment::Staging
        );
        assert_eq!(
            "production".parse::<Environment>().unwrap(),
            Environment::Production
        );
        assert_eq!(
            "prod".parse::<Environment>().unwrap(),
            Environment::Production
        );

        // Case insensitive
        assert_eq!(
            "PRODUCTION".parse::<Environment>().unwrap(),
            Environment::Production
        );
        assert_eq!(
            "Dev".parse::<Environment>().unwrap(),
            Environment::Development
        );

        // Invalid value
        assert!("invalid".parse::<Environment>().is_err());
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Environment::Local), "local");
        assert_eq!(format!("{}", Environment::Production), "production");
    }

    #[test]
    fn test_default() {
        assert_eq!(Environment::default(), Environment::Production);
    }

    #[test]
    fn test_all() {
        let all = Environment::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&Environment::Local));
        assert!(all.contains(&Environment::Development));
        assert!(all.contains(&Environment::Staging));
        assert!(all.contains(&Environment::Production));
        assert!(all.contains(&Environment::Test));
    }
}
