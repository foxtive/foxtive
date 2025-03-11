use diesel::r2d2::ConnectionManager;
use diesel::{r2d2, PgConnection};
use serde::Serialize;

pub mod ext;
mod ext_impl;
pub mod pagination;

pub type DBPool = r2d2::Pool<ConnectionManager<PgConnection>>;

pub trait Model: Serialize {
    type Entity;

    fn into_shareable(self) -> Self::Entity;
}
