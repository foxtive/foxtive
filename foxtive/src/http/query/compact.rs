use super::ordering::OrderBy;
use crate::helpers::http::query::QueryParams;

pub trait CompactOrdering {
    /// Parse compact colon-separated order format
    fn parse_compact_orders(&self) -> Vec<OrderBy>;
}

impl CompactOrdering for QueryParams {
    fn parse_compact_orders(&self) -> Vec<OrderBy> {
        if let Some(order_str) = &self.order {
            let mut orders = Vec::new();
            for order_part in order_str.split(',') {
                let parts: Vec<&str> = order_part.trim().split(':').collect();
                if parts.len() == 2 {
                    let direction = parts[1].trim().to_lowercase();
                    // Validate direction
                    if direction == "asc" || direction == "desc" {
                        orders.push(OrderBy {
                            column: parts[0].trim().to_string(),
                            direction,
                        });
                    }
                }
            }
            return orders;
        }
        Vec::new()
    }
}
