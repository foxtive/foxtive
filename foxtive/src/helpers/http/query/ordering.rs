#[derive(Debug, Clone)]
pub struct OrderBy {
    pub column: String,
    pub direction: String,
}

/// Parse indexed parameter keys like "order[0][column]" into (index, field)
pub fn parse_indexed_key(key: &str) -> Option<(usize, String)> {
    // Simple regex-like parsing for "order[N][field]"
    if !key.starts_with("order[") {
        return None;
    }

    let rest = &key[6..]; // Remove "order["
    let close_bracket = rest.find(']')?;
    let index_str = &rest[..close_bracket];
    let index = index_str.parse::<usize>().ok()?;

    let remaining = &rest[close_bracket + 1..];
    if !remaining.starts_with('[') || !remaining.ends_with(']') {
        return None;
    }

    let field = remaining[1..remaining.len() - 1].to_string();
    Some((index, field))
}