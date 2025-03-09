use std::fmt::Debug;

use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::query_builder::*;
use diesel::query_dsl::methods::LoadQuery;
use diesel::sql_types::BigInt;
use serde::Serialize;

use crate::results::AppPaginationResult;

pub trait Paginate: Sized {
    fn paginate(self, page: i64) -> Paginated<Self>;
}

#[derive(Serialize)]
pub struct PageData<U> {
    pub total_pages: i64,
    pub total_records: i64,
    pub records: Vec<U>,
}

impl<M> PageData<M> {
    pub fn new(records: Vec<M>, total_pages: i64, total_records: i64) -> PageData<M> {
        PageData {
            records,
            total_pages,
            total_records,
        }
    }

    pub fn format_result<T, F>(result: PageData<M>, func: F) -> PageData<T>
    where
        F: Fn(M) -> T,
    {
        let mut records = vec![];
        for model in result.records {
            records.push(func(model));
        }

        PageData::new(records, result.total_pages, result.total_records)
    }

    pub fn format<T, F>(self, func: F) -> PageData<T>
    where
        F: Fn(M) -> T,
    {
        PageData::format_result(self, func)
    }
}

impl<T> Paginate for T {
    fn paginate(self, page: i64) -> Paginated<Self> {
        Paginated {
            query: self,
            per_page: DEFAULT_PER_PAGE,
            page,
            offset: (page - 1) * DEFAULT_PER_PAGE,
        }
    }
}

const DEFAULT_PER_PAGE: i64 = 10;

#[derive(Debug, Clone, Copy, QueryId)]
pub struct Paginated<T> {
    query: T,
    page: i64,
    per_page: i64,
    offset: i64,
}

impl<T> Paginated<T> {
    pub fn per_page(self, per_page: i64) -> Self {
        Paginated {
            per_page,
            offset: (self.page - 1) * per_page,
            ..self
        }
    }

    pub fn load_and_count_pages<'a, U>(self, conn: &mut PgConnection) -> AppPaginationResult<U>
    where
        Self: LoadQuery<'a, PgConnection, (U, i64)>,
    {
        let per_page = self.per_page;
        let results = self.load::<(U, i64)>(conn)?;
        let total = results.first().map(|x| x.1).unwrap_or(0);
        let records = results.into_iter().map(|x| x.0).collect();
        let total_pages = (total as f64 / per_page as f64).ceil() as i64;

        Ok(PageData {
            records,
            total_pages,
            total_records: total,
        })
    }
}

impl<T: Query> Query for Paginated<T> {
    type SqlType = (T::SqlType, BigInt);
}

impl<T> RunQueryDsl<PgConnection> for Paginated<T> {}

impl<T> QueryFragment<Pg> for Paginated<T>
where
    T: QueryFragment<Pg>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Pg>) -> QueryResult<()> {
        out.push_sql("SELECT *, COUNT(*) OVER () FROM (");
        self.query.walk_ast(out.reborrow())?;
        out.push_sql(") t LIMIT ");
        out.push_bind_param::<BigInt, _>(&self.per_page)?;
        out.push_sql(" OFFSET ");
        out.push_bind_param::<BigInt, _>(&self.offset)?;
        Ok(())
    }
}
