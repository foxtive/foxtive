// Example usage in handlers:
use super::{OrderingFormat, QueryParams};

#[test]
fn test_indexed_order_parsing() {
    let query_str = "order[0][column]=fms_id&order[0][direction]=desc&order[1][column]=vehicle_number&order[1][direction]=asc";
    let params: QueryParams = serde_urlencoded::from_str(query_str).unwrap();

    let orders = params.parse_indexed_ordering();
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].column, "fms_id");
    assert_eq!(orders[0].direction, "desc");
    assert_eq!(orders[1].column, "vehicle_number");
    assert_eq!(orders[1].direction, "asc");

    assert!(params.has_indexed_ordering());
    assert!(!params.has_compact_ordering());
    assert_eq!(params.ordering_format(), OrderingFormat::Indexed);
}

#[test]
fn test_compact_order_parsing() {
    let query_str = "order=name:desc,created_at:asc,status:desc";
    let params: QueryParams = serde_urlencoded::from_str(query_str).unwrap();

    let orders = params.parse_compact_ordering();
    assert_eq!(orders.len(), 3);
    assert_eq!(orders[0].column, "name");
    assert_eq!(orders[0].direction, "desc");
    assert_eq!(orders[1].column, "created_at");
    assert_eq!(orders[1].direction, "asc");
    assert_eq!(orders[2].column, "status");
    assert_eq!(orders[2].direction, "desc");

    assert!(!params.has_indexed_ordering());
    assert!(params.has_compact_ordering());
    assert_eq!(params.ordering_format(), OrderingFormat::Compact);
}

#[test]
fn test_indexed_takes_priority_over_compact() {
    let query_str = "order[0][column]=priority_col&order[0][direction]=asc&order=compact_col:desc";
    let params: QueryParams = serde_urlencoded::from_str(query_str).unwrap();

    // Both formats present
    assert!(params.has_indexed_ordering());
    assert!(params.has_compact_ordering());
    assert_eq!(params.ordering_format(), OrderingFormat::Indexed);

    // Auto-parsing should prioritize indexed
    let auto_orders = params.parse_ordering();
    assert_eq!(auto_orders.len(), 1);
    assert_eq!(auto_orders[0].column, "priority_col");
    assert_eq!(auto_orders[0].direction, "asc");

    // Manual parsing should work for both
    let indexed_orders = params.parse_indexed_ordering();
    assert_eq!(indexed_orders.len(), 1);
    assert_eq!(indexed_orders[0].column, "priority_col");

    let compact_orders = params.parse_compact_ordering();
    assert_eq!(compact_orders.len(), 1);
    assert_eq!(compact_orders[0].column, "compact_col");
}

#[test]
fn test_indexed_order_with_gaps() {
    let query_str = "order[2][column]=third&order[2][direction]=desc&order[0][column]=first&order[0][direction]=asc";
    let params: QueryParams = serde_urlencoded::from_str(query_str).unwrap();

    let orders = params.parse_indexed_ordering();
    assert_eq!(orders.len(), 2);
    // Should be sorted by index
    assert_eq!(orders[0].column, "first");
    assert_eq!(orders[0].direction, "asc");
    assert_eq!(orders[1].column, "third");
    assert_eq!(orders[1].direction, "desc");
}

#[test]
fn test_indexed_order_invalid_direction() {
    let query_str = "order[0][column]=test_col&order[0][direction]=invalid&order[1][column]=valid_col&order[1][direction]=asc";
    let params: QueryParams = serde_urlencoded::from_str(query_str).unwrap();

    let orders = params.parse_indexed_ordering();
    // Should only include valid directions
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].column, "valid_col");
    assert_eq!(orders[0].direction, "asc");
}

#[test]
fn test_compact_order_invalid_direction() {
    let query_str = "order=name:invalid,status:asc,date:unknown";
    let params: QueryParams = serde_urlencoded::from_str(query_str).unwrap();

    let orders = params.parse_compact_ordering();
    // Should only include valid directions
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].column, "status");
    assert_eq!(orders[0].direction, "asc");
}

#[test]
fn test_no_ordering_specified() {
    let query_str = "search=test&limit=10&status=active";
    let params: QueryParams = serde_urlencoded::from_str(query_str).unwrap();

    assert!(!params.has_ordering());
    assert!(!params.has_indexed_ordering());
    assert!(!params.has_compact_ordering());
    assert_eq!(params.ordering_format(), OrderingFormat::None);
    assert_eq!(params.ordering_description(), "No ordering specified");
    assert_eq!(params.parse_ordering().len(), 0);
    assert_eq!(params.parse_indexed_ordering().len(), 0);
    assert_eq!(params.parse_compact_ordering().len(), 0);
}

#[test]
fn test_ordering_descriptions() {
    // Indexed format description
    let indexed_query = "order[0][column]=fms_id&order[0][direction]=desc&order[1][column]=vehicle_number&order[1][direction]=asc";
    let indexed_params: QueryParams = serde_urlencoded::from_str(indexed_query).unwrap();
    assert_eq!(
        indexed_params.ordering_description(),
        "fms_id DESC, vehicle_number ASC"
    );

    // Compact format description
    let compact_query = "order=name:desc,created_at:asc";
    let compact_params: QueryParams = serde_urlencoded::from_str(compact_query).unwrap();
    assert_eq!(
        compact_params.ordering_description(),
        "name DESC, created_at ASC"
    );
}

#[test]
fn test_format_detection_methods() {
    // Only indexed
    let indexed_query = "order[0][column]=name&order[0][direction]=asc";
    let indexed_params: QueryParams = serde_urlencoded::from_str(indexed_query).unwrap();
    assert!(indexed_params.has_indexed_ordering());
    assert!(!indexed_params.has_compact_ordering());
    assert!(indexed_params.has_ordering());

    // Only compact
    let compact_query = "order=name:asc";
    let compact_params: QueryParams = serde_urlencoded::from_str(compact_query).unwrap();
    assert!(!compact_params.has_indexed_ordering());
    assert!(compact_params.has_compact_ordering());
    assert!(compact_params.has_ordering());

    // Both present
    let both_query = "order[0][column]=name&order[0][direction]=asc&order=date:desc";
    let both_params: QueryParams = serde_urlencoded::from_str(both_query).unwrap();
    assert!(both_params.has_indexed_ordering());
    assert!(both_params.has_compact_ordering());
    assert!(both_params.has_ordering());
    assert_eq!(both_params.ordering_format(), OrderingFormat::Indexed); // Indexed takes priority
}
