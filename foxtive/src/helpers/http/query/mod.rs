mod indexed;
mod ordering;
mod compact;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use chrono::NaiveDate;
use serde::Deserialize;
use indexed::IndexedOrdering;
use compact::CompactOrdering;

pub use ordering::OrderBy;

/// Enum representing the type of ordering format detected
#[derive(Debug, Clone, PartialEq)]
pub enum OrderingFormat {
    /// No ordering specified
    None,
    /// Indexed format: order[0][column]=name&order[0][direction]=asc
    Indexed,
    /// Compact format: order=name:asc,date:desc
    Compact,
}

/// Represents common query parameters used for filtering, pagination, and sorting in API requests.
#[derive(Deserialize, Clone, Default)]
pub struct QueryParams {
    /// Search term for filtering results based on relevant text.
    ///
    /// Example: `?search=example`
    pub search: Option<String>,

    /// The maximum number of results to return.
    ///
    /// Example: `?limit=50`
    pub limit: Option<i64>,

    /// The current page number for paginated results.
    ///
    /// Example: `?page=2`
    pub page: Option<i64>,

    /// Number of results per page.
    ///
    /// Example: `?per_page=20`
    pub per_page: Option<i64>,

    /// Filter results based on their status.
    ///
    /// Example: `?status=active`
    pub status: Option<String>,

    /// Filter results based on their stage in a process or workflow.
    ///
    /// Example: `?stage=pending`
    pub stage: Option<String>,

    /// Compact multi-column ordering specification. Format: "column:direction,column:direction"
    ///
    /// Examples:
    /// - `?order=name:asc,created_at:desc`
    /// - `?order=fms_id:desc,updated_at:asc,status:asc`
    pub order: Option<String>,

    /// Capture all remaining query parameters to handle indexed orders
    #[serde(flatten)]
    pub extra: HashMap<String, String>,

    /// Filters results by a start date (inclusive). Expected format: `YYYY-MM-DD`.
    ///
    /// Example: `?start_date=2024-01-01`
    pub start_date: Option<NaiveDate>,

    /// Filters results by an end date (inclusive). Expected format: `YYYY-MM-DD`.
    ///
    /// Example: `?end_date=2024-12-31`
    pub end_date: Option<NaiveDate>,
}

impl QueryParams {
    pub fn search(&self) -> Option<String> {
        self.search.clone()
    }

    pub fn search_query(&self) -> String {
        self.search.clone().unwrap_or(String::from(""))
    }

    pub fn search_query_like(&self) -> String {
        format!("%{}%", self.search_query())
    }

    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(10).min(150)
    }

    pub fn curr_page(&self) -> i64 {
        self.page.unwrap_or(1)
    }

    pub fn per_page(&self) -> i64 {
        self.per_page.unwrap_or(10).min(150)
    }

    /// Parse indexed ordering parameters: `order[0][column]=fms_id&order[0][direction]=desc`
    /// Returns a vector of OrderBy structs sorted by index priority.
    pub fn parse_indexed_ordering(&self) -> Vec<OrderBy> {
        self.parse_indexed_orders()
    }

    /// Parse compact ordering parameters: `order=name:desc,created_at:asc`
    /// Returns a vector of OrderBy structs in the specified order.
    pub fn parse_compact_ordering(&self) -> Vec<OrderBy> {
        self.parse_compact_orders()
    }

    /// Parse ordering parameters with automatic format detection and priority:
    /// 1. Indexed order parameters (if present)
    /// 2. Compact colon format (if present)
    /// 3. Empty vector (if no ordering specified)
    pub fn parse_ordering(&self) -> Vec<OrderBy> {
        // Priority 1: Indexed order parameters
        let indexed_orders = self.parse_indexed_ordering();
        if !indexed_orders.is_empty() {
            return indexed_orders;
        }

        // Priority 2: Compact colon format
        let compact_orders = self.parse_compact_ordering();
        if !compact_orders.is_empty() {
            return compact_orders;
        }

        Vec::new()
    }

    /// Check if any ordering is specified (either indexed or compact format)
    pub fn has_ordering(&self) -> bool {
        self.has_indexed_ordering() || self.has_compact_ordering()
    }

    /// Check if indexed ordering is specified
    pub fn has_indexed_ordering(&self) -> bool {
        !self.parse_indexed_ordering().is_empty()
    }

    /// Check if compact ordering is specified
    pub fn has_compact_ordering(&self) -> bool {
        self.order.is_some() && !self.parse_compact_ordering().is_empty()
    }

    /// Get the ordering format currently being used
    pub fn ordering_format(&self) -> OrderingFormat {
        if self.has_indexed_ordering() {
            OrderingFormat::Indexed
        } else if self.has_compact_ordering() {
            OrderingFormat::Compact
        } else {
            OrderingFormat::None
        }
    }

    /// Get a human-readable description of the current ordering
    pub fn ordering_description(&self) -> String {
        let orders = self.parse_ordering();
        if orders.is_empty() {
            "No ordering specified".to_string()
        } else {
            orders
                .iter()
                .map(|o| format!("{} {}", o.column, o.direction.to_uppercase()))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
}
