//! Tests for deferred trait binding with `register_trait_with`.
//!
//! Shows how to register trait objects that depend on services constructed
//! during `freeze()`.

#[cfg(test)]
mod tests {
    use foxtive::lifecycle::ServiceInit;
    use foxtive::prelude::*;
    use std::sync::Arc;

    trait Resolver: Send + Sync {
        fn resolve(&self) -> String;
    }

    struct ConcreteResolver {
        prefix: String,
    }

    impl Resolver for ConcreteResolver {
        fn resolve(&self) -> String {
            format!("{}resolved", self.prefix)
        }
    }

    struct PrefixService {
        value: String,
    }

    impl ServiceInit for PrefixService {
        async fn init(_app: &App) -> AppResult<Self> {
            Ok(Self {
                value: "test-".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_trait_factory_basic() {
        let mut init = App::builder("Test", "TST")
            .register_service::<PrefixService>()
            .build_init()
            .await
            .unwrap();

        init.register_trait_with::<dyn Resolver, _>(|app| {
            let prefix_svc = app.require::<PrefixService>()?;
            Ok(Arc::new(ConcreteResolver {
                prefix: prefix_svc.value.clone(),
            }) as Arc<dyn Resolver>)
        });

        let app = init.freeze().await.unwrap();
        let resolver = app.require_trait::<dyn Resolver>().unwrap();
        assert_eq!(resolver.resolve(), "test-resolved");
    }

    #[tokio::test]
    async fn test_trait_factory_with_multiple_dependencies() {
        struct SuffixService {
            value: String,
        }

        impl ServiceInit for SuffixService {
            async fn init(_app: &App) -> AppResult<Self> {
                Ok(Self {
                    value: "-suffix".to_string(),
                })
            }
        }

        struct CombinedResolver {
            prefix: String,
            suffix: String,
        }

        impl Resolver for CombinedResolver {
            fn resolve(&self) -> String {
                format!("{}combined{}", self.prefix, self.suffix)
            }
        }

        let app = App::builder("Test", "TST")
            .register_service::<PrefixService>()
            .register_with::<SuffixService, _, _>(|_app| async {
                Ok(SuffixService {
                    value: "-suffix".to_string(),
                })
            })
            .register_trait_with::<dyn Resolver, _>(|app| {
                let prefix_svc = app.require::<PrefixService>()?;
                let suffix_svc = app.require::<SuffixService>()?;
                Ok(Arc::new(CombinedResolver {
                    prefix: prefix_svc.value.clone(),
                    suffix: suffix_svc.value.clone(),
                }) as Arc<dyn Resolver>)
            })
            .build()
            .await
            .unwrap();

        let resolver = app.require_trait::<dyn Resolver>().unwrap();
        assert_eq!(resolver.resolve(), "test-combined-suffix");
    }

    #[tokio::test]
    async fn test_trait_factory_error_propagation() {
        struct MissingDepResolver;
        impl Resolver for MissingDepResolver {
            fn resolve(&self) -> String {
                "resolved".to_string()
            }
        }

        let result = App::builder("Test", "TST")
            .register_trait_with::<dyn Resolver, _>(|app| {
                // This service doesn't exist - should fail
                let _missing = app.require::<PrefixService>()?;
                Ok(Arc::new(MissingDepResolver) as Arc<dyn Resolver>)
            })
            .build()
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_trait_factory_replacement() {
        struct FirstResolver;
        impl Resolver for FirstResolver {
            fn resolve(&self) -> String {
                "first".to_string()
            }
        }

        struct SecondResolver;
        impl Resolver for SecondResolver {
            fn resolve(&self) -> String {
                "second".to_string()
            }
        }

        let mut init = App::builder("Test", "TST")
            .register_trait_with::<dyn Resolver, _>(|_app| {
                Ok(Arc::new(FirstResolver) as Arc<dyn Resolver>)
            })
            .build_init()
            .await
            .unwrap();

        // Replace with second factory
        init.register_trait_with::<dyn Resolver, _>(|_app| {
            Ok(Arc::new(SecondResolver) as Arc<dyn Resolver>)
        });

        let app = init.freeze().await.unwrap();
        let resolver = app.require_trait::<dyn Resolver>().unwrap();
        assert_eq!(resolver.resolve(), "second");
    }

    #[tokio::test]
    async fn test_trait_factory_with_complex_setup() {
        // Test that factories can perform complex setup
        struct ComplexResolver {
            value: String,
        }
        impl Resolver for ComplexResolver {
            fn resolve(&self) -> String {
                format!("complex-{}", self.value)
            }
        }

        let app = App::builder("Test", "TST")
            .register_trait_with::<dyn Resolver, _>(|_app| {
                // Simulate complex setup
                let value = "setup-complete".to_string();
                Ok(Arc::new(ComplexResolver { value }) as Arc<dyn Resolver>)
            })
            .build()
            .await
            .unwrap();

        let resolver = app.require_trait::<dyn Resolver>().unwrap();
        assert_eq!(resolver.resolve(), "complex-setup-complete");
    }

    #[tokio::test]
    async fn test_trait_factory_with_service_dependencies() {
        // Test the exact scenario from the problem statement
        struct UserService {
            name: String,
        }

        trait UserResolver: Send + Sync {
            fn get_user(&self) -> String;
        }

        struct UtilityUserResolver {
            service: Arc<UserService>,
        }

        impl UserResolver for UtilityUserResolver {
            fn get_user(&self) -> String {
                format!("user-from-{}", self.service.name)
            }
        }

        let app = App::builder("Test", "TST")
            .register(UserService {
                name: "UserService".to_string(),
            })
            .register(UtilityUserResolver {
                service: Arc::new(UserService {
                    name: "UserService".to_string(),
                }),
            })
            .register_trait_with::<dyn UserResolver, _>(|app| {
                let resolver = app.require::<UtilityUserResolver>()?;
                Ok(resolver as Arc<dyn UserResolver>)
            })
            .build()
            .await
            .unwrap();

        // Both concrete and trait access should work
        assert!(app.get::<UtilityUserResolver>().is_some());
        assert!(app.get_trait::<dyn UserResolver>().is_some());

        let user_resolver = app.require_trait::<dyn UserResolver>().unwrap();
        assert_eq!(user_resolver.get_user(), "user-from-UserService");
    }
}
