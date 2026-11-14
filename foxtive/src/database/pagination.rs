use std::fmt::Debug;

use diesel::prelude::*;
use diesel::query_builder::*;
use diesel::query_dsl::methods::LoadQuery;
use diesel::sql_types::BigInt;
use serde::Serialize;

#[cfg(feature = "database-async")]
use crate::database::backend::AsyncDbConnection;
use crate::database::backend::DbConnection;
use crate::results::AppPaginationResult;

#[cfg(any(
    feature = "database-postgres",
    all(
        feature = "database",
        not(any(feature = "database-postgres", feature = "database-mysql"))
    )
))]
use diesel::pg::Pg;

#[cfg(all(feature = "database-mysql", not(feature = "database-postgres")))]
use diesel::mysql::Mysql;

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
    pub fn load_and_count_pages<'a, U>(self, conn: &mut DbConnection) -> AppPaginationResult<U>
    where
        Self: LoadQuery<'a, DbConnection, (U, i64)>,
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
    pub fn load_and_count_pages_async<'conn, 'query, U>(
        self,
        conn: &'conn mut AsyncDbConnection,
    ) -> impl Future<Output = AppPaginationResult<U>> + Send + 'conn
    where
        U: Send + 'static,
        'query: 'conn,
        Self: diesel_async::methods::LoadQuery<'query, AsyncDbConnection, (U, i64)> + Send + 'query,
    {
        let per_page = self.per_page;
        let load = <Self as diesel_async::RunQueryDsl<AsyncDbConnection>>::load(self, conn);
        async move {
            let results = load.await?;
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
}

#[cfg(any(feature = "database", feature = "database-async"))]
impl<T: Query> Query for Paginated<T> {
    type SqlType = (T::SqlType, BigInt);
}

// Diesel 2.4 stopped providing `RunQueryDsl` through unconditional blanket impls: diesel
// and diesel-async now both derive it from the `RunQueryDslSupport` marker, which only
// covers `QueryRelation` types by default. `Paginated` is a custom query type rather than
// a table, so it opts in through the marker once and picks up the sync and async
// `RunQueryDsl` impls from the respective blanket impls.
#[cfg(any(feature = "database", feature = "database-async"))]
impl<T> diesel::query_dsl::RunQueryDslSupport for Paginated<T> {}

// --- PostgreSQL backend: QueryFragment<Pg> ---
#[cfg(any(
    feature = "database-postgres",
    all(
        feature = "database",
        not(any(feature = "database-postgres", feature = "database-mysql"))
    )
))]
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

// --- MySQL backend: QueryFragment<Mysql> (requires MySQL 8.0+ for window functions) ---
#[cfg(all(feature = "database-mysql", not(feature = "database-postgres")))]
impl<T> QueryFragment<Mysql> for Paginated<T>
where
    T: QueryFragment<Mysql>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, Mysql>) -> QueryResult<()> {
        // MySQL 8.0+ supports COUNT(*) OVER() window functions
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

    // Diesel 2.4 wiring coverage. `Paginated` is a custom query type, so neither the
    // blocking nor the async `RunQueryDsl` impl exists on its own - both come from the
    // `RunQueryDslSupport` marker. These tests pin that marker (and the SQL it renders)
    // so a future diesel or diesel-async bump cannot silently drop it again.
    #[cfg(any(feature = "database", feature = "database-async"))]
    mod dsl_wiring {
        use super::*;

        diesel::table! {
            users (id) {
                id -> BigInt,
                name -> Varchar,
            }
        }

        fn paginated<Q>(query: Q) -> Paginated<Q> {
            query.paginate(3).per_page(25)
        }

        #[test]
        fn marker_provides_blocking_dsl_and_load_query() {
            fn assert_blocking<Q>(page: Paginated<Q>)
            where
                for<'a> Paginated<Q>:
                    RunQueryDsl<DbConnection> + LoadQuery<'a, DbConnection, ((i64, String), i64)>,
            {
                let _ = page;
            }

            assert_blocking(paginated(users::table.select((users::id, users::name))));
        }

        #[cfg(feature = "database-async")]
        #[test]
        fn marker_provides_async_dsl_and_load_query() {
            // This is the bound set `load_and_count_pages_async` relies on, and exactly the
            // one that stopped resolving once diesel-async 0.10 gated its blanket impl.
            fn assert_async<Q>(page: Paginated<Q>)
            where
                for<'a> Paginated<Q>: diesel_async::RunQueryDsl<AsyncDbConnection>
                    + diesel_async::methods::LoadQuery<'a, AsyncDbConnection, ((i64, String), i64)>
                    + Send
                    + 'a,
            {
                let _ = page;
            }

            assert_async(paginated(users::table.select((users::id, users::name))));
        }

        #[cfg(any(
            feature = "database-postgres",
            all(feature = "database", not(feature = "database-mysql"))
        ))]
        fn render<Q: QueryFragment<Pg>>(page: &Paginated<Q>) -> String {
            diesel::debug_query::<Pg, _>(page).to_string()
        }

        #[cfg(all(feature = "database-mysql", not(feature = "database-postgres")))]
        fn render<Q: QueryFragment<Mysql>>(page: &Paginated<Q>) -> String {
            diesel::debug_query::<Mysql, _>(page).to_string()
        }

        #[test]
        fn paginated_sql_keeps_count_window_and_page_bounds() {
            let sql = render(&paginated(users::table.select((users::id, users::name))));
            // page 3 at 25 per page -> LIMIT 25 OFFSET 50, wrapped so COUNT(*) OVER ()
            // reports the total of the unpaged result set
            assert!(sql.contains("FROM (SELECT"), "{sql}");
            assert!(sql.contains("COUNT(*) OVER ()"), "{sql}");
            assert!(sql.contains(") t LIMIT"), "{sql}");
            assert!(sql.contains("OFFSET"), "{sql}");
            assert!(sql.contains("[25, 50]"), "{sql}");
        }
    }
}
