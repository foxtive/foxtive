use std::fmt::Debug;

use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::query_builder::*;
use diesel::query_dsl::methods::LoadQuery;
use diesel::sql_types::BigInt;
use serde::Serialize;

use crate::results::AppPaginationResult;

#[cfg(feature = "database-async")]
use diesel_async::AsyncPgConnection;

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

    #[cfg(feature = "database")]
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

    #[cfg(feature = "database-async")]
    pub async fn load_and_count_pages_async<'a, U>(
        self,
        conn: &mut AsyncPgConnection,
    ) -> AppPaginationResult<U>
    where
        U: Send,
        T: 'a,
        Self: diesel_async::methods::LoadQuery<'a, AsyncPgConnection, (U, i64)>,
    {
        let per_page = self.per_page;
        let results = <Self as diesel_async::RunQueryDsl<AsyncPgConnection>>::load(self, conn).await?;
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

#[cfg(any(feature = "database", feature = "database-async"))]
impl<T: Query> Query for Paginated<T> {
    type SqlType = (T::SqlType, BigInt);
}

#[cfg(feature = "database")]
impl<T> RunQueryDsl<PgConnection> for Paginated<T> {}

#[cfg(any(feature = "database", feature = "database-async"))]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_data_new_stores_fields() {
        let page = PageData::new(vec![1, 2, 3], 5, 42);
        assert_eq!(page.records, vec![1, 2, 3]);
        assert_eq!(page.total_pages, 5);
        assert_eq!(page.total_records, 42);
    }

    #[test]
    fn page_data_format_result_transforms_records() {
        let page = PageData::new(vec![1, 2, 3], 2, 6);
        let transformed = PageData::format_result(page, |x| x * 10);
        assert_eq!(transformed.records, vec![10, 20, 30]);
        assert_eq!(transformed.total_pages, 2);
        assert_eq!(transformed.total_records, 6);
    }

    #[test]
    fn page_data_format_method_transforms_records() {
        let page = PageData::new(vec!["a", "b"], 1, 2);
        let transformed = page.format(|s| s.to_uppercase());
        assert_eq!(transformed.records, vec!["A", "B"]);
        assert_eq!(transformed.total_pages, 1);
        assert_eq!(transformed.total_records, 2);
    }

    #[test]
    fn page_data_empty_records() {
        let page: PageData<i32> = PageData::new(vec![], 0, 0);
        assert!(page.records.is_empty());
        assert_eq!(page.total_pages, 0);
        assert_eq!(page.total_records, 0);
    }

    #[test]
    fn paginated_per_page_updates_offset() {
        let p = Paginated {
            query: (),
            page: 3,
            per_page: 10,
            offset: 20,
        };
        let updated = p.per_page(25);
        assert_eq!(updated.per_page, 25);
        assert_eq!(updated.offset, (3 - 1) * 25);
    }

    #[test]
    fn default_per_page_is_ten() {
        assert_eq!(DEFAULT_PER_PAGE, 10);
    }
}
