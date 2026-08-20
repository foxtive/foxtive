mod common;

#[cfg(feature = "database")]
mod db_config {
    use foxtive::database::DbConfig;
    use std::time::Duration;

    #[test]
    fn valid_config_passes_validation() {
        let config = DbConfig::create("postgres://localhost/test");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn empty_dsn_is_rejected() {
        let config = DbConfig::create("");
        assert!(config.validate().is_err());
    }

    #[test]
    fn whitespace_only_dsn_is_rejected() {
        let config = DbConfig::create("   ");
        assert!(config.validate().is_err());
    }

    #[test]
    fn min_idle_exceeding_max_size_is_rejected() {
        let config = DbConfig::create("postgres://localhost/test")
            .max_size(5)
            .min_idle(Some(10));
        assert!(config.validate().is_err());
    }

    #[test]
    fn min_idle_equal_to_max_size_is_valid() {
        let config = DbConfig::create("postgres://localhost/test")
            .max_size(5)
            .min_idle(Some(5));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn full_builder_chain_validates() {
        let config = DbConfig::create("postgres://localhost/test")
            .max_size(20)
            .min_idle(Some(5))
            .connection_timeout(Duration::from_secs(10))
            .idle_timeout(Some(Duration::from_secs(300)))
            .max_lifetime(Some(Duration::from_secs(1800)));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn zero_max_size_fails_validation() {
        let config = DbConfig::create("postgres://localhost/test").max_size(0);
        assert!(config.validate().is_err());
    }
}

#[cfg(feature = "redis")]
mod redis_config {
    use foxtive::redis::config::RedisConfig;
    use std::time::Duration;

    #[test]
    fn valid_config_passes_validation() {
        let config = RedisConfig::create("redis://localhost:6379");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn empty_dsn_is_rejected() {
        let config = RedisConfig::create("");
        assert!(config.validate().is_err());
    }

    #[test]
    fn custom_timeouts_are_accepted() {
        let config = RedisConfig::create("redis://localhost:6379")
            .wait_timeout(Duration::from_secs(5))
            .recycle_timeout(Duration::from_millis(500));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn zero_timeout_is_accepted() {
        let config =
            RedisConfig::create("redis://localhost:6379").wait_timeout(Duration::from_secs(0));
        assert!(config.validate().is_ok());
    }
}

#[cfg(feature = "rabbitmq")]
mod rabbitmq_config {
    use foxtive::rabbitmq::config::RabbitmqConfig;
    use std::time::Duration;

    #[test]
    fn valid_config_passes_validation() {
        let config = RabbitmqConfig::create("amqp://guest:guest@localhost:5672");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn empty_dsn_is_rejected() {
        let config = RabbitmqConfig::create("");
        assert!(config.validate().is_err());
    }

    #[test]
    fn custom_timeouts_are_accepted() {
        let config = RabbitmqConfig::create("amqp://guest:guest@localhost:5672")
            .wait_timeout(Duration::from_secs(15))
            .recycle_timeout(Duration::from_secs(3));
        assert!(config.validate().is_ok());
    }
}
