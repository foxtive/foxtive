use diesel::PgConnection;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use serde::Serialize;
use crate::database::Model;
use crate::prelude::AppResult;
use crate::results::{AppOptionalResult, AppPaginationResult};

pub trait ShareableResultExt<S: Serialize, T: Serialize + Model> {
    fn into_shareable_result(self) -> AppResult<S>;
}

pub trait ShareablePaginationResultExt<S: Serialize, T: Serialize + Model> {
    fn into_shareable_result(self) -> AppPaginationResult<S>;
}

pub trait OptionalResultExt<'a, T> {
    fn optional(self) -> AppOptionalResult<T>;
    fn required(self, entity: &'a str) -> AppResult<T>;
    fn exists(self) -> AppResult<bool>;
}

pub trait DatabaseConnectionExt {
    fn connection(&self) -> AppResult<PooledConnection<ConnectionManager<PgConnection>>>;
}
