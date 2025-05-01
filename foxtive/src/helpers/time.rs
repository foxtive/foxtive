use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, NaiveDateTime, TimeDelta};
use serde::{Deserialize, Deserializer, Serializer};

pub fn now_plus_seconds(sec: i64) -> NaiveDateTime {
    (Local::now() + TimeDelta::try_seconds(sec).unwrap()).naive_local()
}

pub fn now_plus_minutes(min: i64) -> NaiveDateTime {
    now_plus_seconds(min * 60)
}

pub fn current_datetime() -> NaiveDateTime {
    Local::now().naive_local()
}

pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_secs()
}

pub fn serde_de_datetime<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S").map_err(serde::de::Error::custom)
}

pub fn serde_se_datetime<S>(date: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let formatted_date = date.format("%Y-%m-%d %H:%M:%S").to_string();
    serializer.serialize_str(&formatted_date)
}
