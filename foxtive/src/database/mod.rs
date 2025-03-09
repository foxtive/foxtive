use diesel::r2d2::ConnectionManager;
use diesel::result::Error;
use diesel::{r2d2, PgConnection, QueryResult};
use r2d2::PooledConnection;
use serde::Serialize;

use crate::prelude::AppMessage::EntityNotFound;
use crate::prelude::AppResult;
use crate::results::AppOptionalResult;

pub mod pagination;

pub type DBPool = r2d2::Pool<ConnectionManager<PgConnection>>;

pub trait Model: Serialize {
    type Entity;

    fn into_shareable(self) -> Self::Entity;
}

pub trait OptionalResult<'a, T> {
    fn optional(self) -> AppOptionalResult<T>;
    fn required(self, entity: &'a str) -> AppResult<T>;
    fn exists(self) -> AppResult<bool>;
}

pub trait DatabaseConnectionHelper {
    fn connection(&self) -> AppResult<PooledConnection<ConnectionManager<PgConnection>>>;
}

impl DatabaseConnectionHelper for DBPool {
    fn connection(&self) -> AppResult<PooledConnection<ConnectionManager<PgConnection>>> {
        self.get().map_err(anyhow::Error::msg)
    }
}

impl<'a, T> OptionalResult<'a, T> for QueryResult<T> {
    fn optional(self) -> AppOptionalResult<T> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(Error::NotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn required(self, entity: &'a str) -> AppResult<T> {
        match self {
            Ok(value) => Ok(value),
            Err(Error::NotFound) => EntityNotFound(entity.to_string()).into_result(),
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
