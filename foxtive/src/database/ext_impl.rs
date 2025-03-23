use crate::database::ext::{DatabaseConnectionExt, OptionalResultExt, PaginationResultExt, ShareablePaginationResultExt, ShareableResultExt};
use crate::database::{DBPool, Model};
use crate::prelude::AppMessage::EntityNotFound;
use crate::prelude::AppResult;
use crate::results::{AppOptionalResult, AppPaginationResult};
use diesel::r2d2::{ConnectionManager, PooledConnection};
use diesel::result::Error;
use diesel::{PgConnection, QueryResult};
use serde::Serialize;

impl<Sha, Ent> ShareableResultExt<Sha, Ent> for AppResult<Ent>
where
    Sha: Serialize,
    Ent: Serialize + Model<Entity = Sha>,
{
    fn into_shareable_result(self) -> AppResult<Sha> {
        self.map(|entity| entity.into_shareable())
    }
}

impl<Sha, Ent> ShareablePaginationResultExt<Sha, Ent> for AppPaginationResult<Ent>
where
    Sha: Serialize,
    Ent: Serialize + Model<Entity = Sha>,
{
    fn into_shareable_result(self) -> AppPaginationResult<Sha> {
        self.map(|paged| paged.format(|entity| entity.into_shareable()))
    }
}

impl DatabaseConnectionExt for DBPool {
    fn connection(&self) -> AppResult<PooledConnection<ConnectionManager<PgConnection>>> {
        self.get().map_err(anyhow::Error::msg)
    }
}

impl<'a, T> OptionalResultExt<'a, T> for QueryResult<T> {
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

impl<T> PaginationResultExt<T> for AppPaginationResult<T> {
    fn map_page_data<U>(self, mapper: fn(T) -> U) -> AppPaginationResult<U> {
        self.map(|paged| paged.format(mapper))
    }
}
