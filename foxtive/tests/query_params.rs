//! Integration tests for `QueryParams` deserialization and ordering.
//!
//! These tests verify end-to-end URL-encoded query string parsing,
//! pagination defaults, and search helpers.

#[cfg(feature = "http")]
mod query_params_tests {
    use foxtive::http::query::{OrderingFormat, QueryParams};

    fn parse(query: &str) -> QueryParams {
        serde_urlencoded::from_str(query).expect("valid query string")
    }

    #[test]
    fn default_limit_is_10() {
        let params = parse("search=hello");
        assert_eq!(params.limit(), 10);
    }

    #[test]
    fn limit_is_capped_at_150() {
        let params = parse("limit=9999");
        assert_eq!(params.limit(), 150);
    }

    #[test]
    fn custom_limit_respected() {
        let params = parse("limit=50");
        assert_eq!(params.limit(), 50);
    }

    #[test]
    fn default_page_is_1() {
        let params = parse("");
        assert_eq!(params.curr_page(), 1);
    }

    #[test]
    fn per_page_defaults_and_cap() {
        let p1 = parse("");
        assert_eq!(p1.per_page(), 10);

        let p2 = parse("per_page=500");
        assert_eq!(p2.per_page(), 150);
    }

    #[test]
    fn search_returns_none_when_absent() {
        let params = parse("limit=5");
        assert!(params.search().is_none());
    }

    #[test]
    fn search_query_returns_empty_string_when_absent() {
        let params = parse("");
        assert_eq!(params.search_query(), "");
    }

    #[test]
    fn search_query_like_wraps_with_percent() {
        let params = parse("search=foo");
        assert_eq!(params.search_query_like(), "%foo%");
    }

    #[test]
    fn compact_order_single_column() {
        let params = parse("order=name:asc");
        let orders = params.parse_compact_ordering();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].column, "name");
        assert_eq!(orders[0].direction, "asc");
    }

    #[test]
    fn compact_order_multiple_columns() {
        let params = parse("order=name:asc,created_at:desc,id:asc");
        let orders = params.parse_compact_ordering();
        assert_eq!(orders.len(), 3);
        assert_eq!(orders[1].column, "created_at");
        assert_eq!(orders[1].direction, "desc");
    }

    #[test]
    fn compact_order_invalid_direction_skipped() {
        let params = parse("order=name:up,date:desc");
        let orders = params.parse_compact_ordering();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].column, "date");
    }

    #[test]
    fn indexed_order_basic() {
        let params = parse("order[0][column]=id&order[0][direction]=desc");
        let orders = params.parse_indexed_ordering();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].column, "id");
        assert_eq!(orders[0].direction, "desc");
    }

    #[test]
    fn indexed_order_sorted_by_index() {
        let params = parse(
            "order[1][column]=name&order[1][direction]=asc&order[0][column]=id&order[0][direction]=desc",
        );
        let orders = params.parse_indexed_ordering();
        assert_eq!(orders.len(), 2);
        assert_eq!(orders[0].column, "id");
        assert_eq!(orders[1].column, "name");
    }

    #[test]
    fn auto_detect_indexed_priority() {
        let params = parse("order=compact:asc&order[0][column]=indexed&order[0][direction]=desc");
        assert_eq!(params.ordering_format(), OrderingFormat::Indexed);
        let orders = params.parse_ordering();
        assert_eq!(orders[0].column, "indexed");
    }

    #[test]
    fn auto_detect_compact_fallback() {
        let params = parse("order=name:desc");
        assert_eq!(params.ordering_format(), OrderingFormat::Compact);
    }

    #[test]
    fn no_ordering_detected() {
        let params = parse("search=test");
        assert_eq!(params.ordering_format(), OrderingFormat::None);
        assert!(!params.has_ordering());
        assert_eq!(params.ordering_description(), "No ordering specified");
    }

    #[test]
    fn date_filters_parsed() {
        let params = parse("start_date=2024-01-01&end_date=2024-12-31");
        assert!(params.start_date.is_some());
        assert!(params.end_date.is_some());
    }

    #[test]
    fn datetime_filters_parsed() {
        let params = parse("start_datetime=2024-01-01T09:30:00&end_datetime=2024-12-31T18:00:00");
        assert!(params.start_datetime.is_some());
        assert!(params.end_datetime.is_some());
    }

    #[test]
    fn status_and_stage_parsed() {
        let params = parse("status=active&stage=pending");
        assert_eq!(params.status.as_deref(), Some("active"));
        assert_eq!(params.stage.as_deref(), Some("pending"));
    }

    #[test]
    fn ordering_description_compact() {
        let params = parse("order=name:asc,age:desc");
        assert_eq!(params.ordering_description(), "name ASC, age DESC");
    }

    #[test]
    fn ordering_description_indexed() {
        let params = parse("order[0][column]=id&order[0][direction]=desc");
        assert_eq!(params.ordering_description(), "id DESC");
    }
}
