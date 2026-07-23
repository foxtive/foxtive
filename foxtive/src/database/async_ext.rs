//! Async extension traits for diesel-async + deadpool.

use crate::database::Model;
use crate::prelude::AppResult;
use crate::results::{AppOptionalResult, AppPaginationResult};
use diesel::result::Error;
use diesel_async::pooled_connection::deadpool::{Object, Pool};
use diesel_async::AsyncPgConnection;
use serde::Serialize;

/// Async counterpart to [`DatabaseConnectionExt`](crate::database::ext::DatabaseConnectionExt).
///
/// Provides async connection checkout from the deadpool.
pub trait AsyncDatabaseConnectionExt {
    fn connection(
        &self,
    ) -> impl std::future::Future<Output = AppResult<Object<AsyncPgConnection>>> + Send;
}

impl AsyncDatabaseConnectionExt for Pool<AsyncPgConnection> {
    async fn connection(&self) -> AppResult<Object<AsyncPgConnection>> {
        self.get()
            .await
            .map_err(|e| crate::enums::AppMessage::Infrastructure {
                message: format!("Async connection pool error: {e}"),
                source: None,
            })
    }
}

/// Async counterpart to [`OptionalResultExt`](crate::database::ext::OptionalResultExt).
///
/// Works with `QueryResult<T>` from diesel-async operations.
///
/// **Note:** Unlike the blocking `OptionalResultExt<'a, T>`, this trait does not
/// carry a lifetime parameter. The `required()` method takes `&str` without
/// lifetime linkage, which is a deliberate API divergence for async ergonomics.
pub trait AsyncOptionalResultExt<T> {
    fn optional(self) -> AppOptionalResult<T>;
    fn required(self, entity: &str) -> AppResult<T>;
    fn exists(self) -> AppResult<bool>;
}

impl<T> AsyncOptionalResultExt<T> for diesel::QueryResult<T> {
    fn optional(self) -> AppOptionalResult<T> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(Error::NotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn required(self, entity: &str) -> AppResult<T> {
        match self {
            Ok(value) => Ok(value),
            Err(Error::NotFound) => {
                crate::enums::AppMessage::not_found(format!("Such {entity} does not exist"))
                    .into_result()
            }
            Err(e) => Err(e.into()),
        }
    }

    fn exists(self) -> AppResult<bool> {
        match self {
            Ok(_) => Ok(true),
            Err(Error::NotFound) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

/// Async counterpart to [`ShareableResultExt`](crate::database::ext::ShareableResultExt).
pub trait AsyncShareableResultExt<S: Serialize, T: Serialize + Model> {
    fn into_shareable_result(self) -> AppResult<S>;
}

impl<S, T> AsyncShareableResultExt<S, T> for AppResult<T>
where
    S: Serialize,
    T: Serialize + Model<Entity = S>,
{
    fn into_shareable_result(self) -> AppResult<S> {
        self.map(|entity| entity.into_shareable())
    }
}

/// Async counterpart to [`ShareablePaginationResultExt`](crate::database::ext::ShareablePaginationResultExt).
pub trait AsyncShareablePaginationResultExt<S: Serialize, T: Serialize + Model> {
    fn into_shareable_result(self) -> AppPaginationResult<S>;
}

impl<S, T> AsyncShareablePaginationResultExt<S, T> for AppPaginationResult<T>
where
    S: Serialize,
    T: Serialize + Model<Entity = S>,
{
    fn into_shareable_result(self) -> AppPaginationResult<S> {
        self.map(|paged| paged.format(|entity| entity.into_shareable()))
    }
}

/// Async counterpart to [`PaginationResultExt`](crate::database::ext::PaginationResultExt).
pub trait AsyncPaginationResultExt<T> {
    fn map_page_data<U, F>(self, mapper: F) -> AppPaginationResult<U>
    where
        F: Fn(T) -> U;
}

impl<T> AsyncPaginationResultExt<T> for AppPaginationResult<T> {
    fn map_page_data<U, F>(self, mapper: F) -> AppPaginationResult<U>
    where
        F: Fn(T) -> U,
    {
        self.map(|paged| paged.format(mapper))
    }
}
