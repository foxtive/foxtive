use diesel::r2d2::ConnectionManager;
use diesel::{PgConnection, r2d2};
use serde::Serialize;

mod config;
mod conn;
pub mod ext;
mod ext_impl;
pub mod pagination;

pub use config::DbConfig;
pub use conn::create_db_pool;

pub type DBPool = r2d2::Pool<ConnectionManager<PgConnection>>;

pub trait Model: Serialize {
    type Entity;

    fn into_shareable(self) -> Self::Entity;
}
