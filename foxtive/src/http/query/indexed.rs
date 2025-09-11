use std::collections::HashMap;
use crate::http::query::{ordering, QueryParams};
use super::ordering::OrderBy;

pub trait IndexedOrdering {
    /// Parse indexed order parameters from extra fields
    fn parse_indexed_orders(&self) -> Vec<OrderBy>;
}

impl IndexedOrdering for QueryParams {
    fn parse_indexed_orders(&self) -> Vec<OrderBy> {
        let mut order_map: HashMap<usize, (Option<String>, Option<String>)> = HashMap::new();

        // Parse flattened parameters like "order[0][column]" and "order[0][direction]"
        for (key, value) in &self.extra {
            if let Some((index, field)) = ordering::parse_indexed_key(key) {
                let entry = order_map.entry(index).or_insert((None, None));

                match field.as_str() {
                    "column" => entry.0 = Some(value.clone()),
                    "direction" => entry.1 = Some(value.clone()),
                    _ => {} // Ignore unknown fields
                }
            }
        }

        // Convert to OrderBy structs, maintaining index order
        let mut indices: Vec<usize> = order_map.keys().copied().collect();
        indices.sort();

        let mut orders = Vec::new();
        for index in indices {
            if let Some((Some(column), Some(direction))) = order_map.get(&index) {
                let direction_lower = direction.to_lowercase();
                if direction_lower == "asc" || direction_lower == "desc" {
                    orders.push(OrderBy {
                        column: column.clone(),
                        direction: direction_lower,
                    });
                }
            }
        }

        orders
    }
}