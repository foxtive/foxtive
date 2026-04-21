use crate::contracts::{Schedule, ValidatedSchedule};
use crate::{CronError, CronResult};
use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Represents the months of the year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Month {
    January = 1,
    February = 2,
    March = 3,
    April = 4,
    May = 5,
    June = 6,
    July = 7,
    August = 8,
    September = 9,
    October = 10,
    November = 11,
    December = 12,
}

/// Represents the days of the week.
/// Note: These values map to cron crate format where Sunday=1, Monday=2, ..., Saturday=7
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weekday {
    Sunday = 0,
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
}

/// Represents the different ways a cron field can be specified.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum CronField {
    /// All values (*)
    All,
    /// A single specific value (e.g., 5)
    Value(u32),
    /// A range of values (e.g., 1-5)
    Range(u32, u32),
    /// An interval/step (e.g., */15 or 1-30/5)
    Step(Box<CronField>, u32),
    /// A list of specific components (e.g., 1,3,5-10)
    List(Vec<CronField>),
}

impl fmt::Display for CronField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CronField::All => write!(f, "*"),
            CronField::Value(v) => write!(f, "{}", v),
            CronField::Range(start, end) => write!(f, "{}-{}", start, end),
            CronField::Step(base, step) => write!(f, "{}/{}", base, step),
            CronField::List(values) => {
                let s: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                write!(f, "{}", s.join(","))
            }
        }
    }
}

/// A structure representing a cron expression with advanced features like
/// timezones, exclusion rules, and jitter.
///
/// Use `CronExpression::builder()` to create a new instance fluently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronExpression {
    seconds: CronField,
    minutes: CronField,
    hours: CronField,
    day_of_month: CronField,
    month: CronField,
    day_of_week: CronField,
    year: CronField,

    timezone: Option<Tz>,
    jitter: Option<Duration>,
    blackout_dates: Vec<NaiveDate>,

    #[serde(skip)]
    error: Option<String>,
    #[serde(skip)]
    validated: Option<ValidatedSchedule>,
}

impl Default for CronExpression {
    fn default() -> Self {
        Self {
            seconds: CronField::Value(0),
            minutes: CronField::All,
            hours: CronField::All,
            day_of_month: CronField::All,
            month: CronField::All,
            day_of_week: CronField::All,
            year: CronField::All,
            timezone: None,
            jitter: None,
            blackout_dates: Vec::new(),
            error: None,
            validated: None,
        }
    }
}

impl Schedule for CronExpression {
    fn next_after(&self, after: &DateTime<Utc>, tz: Tz) -> Option<DateTime<Utc>> {
        let active_tz = self.timezone.unwrap_or(tz);

        // Loop to handle blackout dates (if next time is blacked out, find the one after)
        let mut current_after = *after;

        for _ in 0..100 {
            // Safety limit to prevent infinite loops
            let mut next = if let Some(v) = &self.validated {
                v.next_after(&current_after, active_tz)
            } else if let Ok(v) = self.to_validated() {
                v.next_after(&current_after, active_tz)
            } else {
                return None;
            }?;

            // Apply Jitter if specified
            if let Some(jitter_dur) = self.jitter {
                let mut rng = rand::rng();
                let millis = jitter_dur.as_millis();
                if millis > 0 {
                    let offset = rng.random_range(0..millis);
                    next += Duration::from_millis(offset as u64);
                }
            }

            // Check Blackout Dates
            let date = next.date_naive();
            if self.blackout_dates.contains(&date) {
                current_after = next;
                continue;
            }

            return Some(next);
        }

        None
    }
}

impl CronExpression {
    /// Create a new builder for a `CronExpression`.
    pub fn builder() -> Self {
        Self::default()
    }

    /// Set a fixed timezone for this schedule.
    pub fn with_timezone(mut self, tz: Tz) -> Self {
        self.timezone = Some(tz);
        self
    }

    /// Adds a random jitter to the execution time.
    ///
    /// The job will run at the scheduled time plus a random duration
    /// between 0 and `max_jitter`.
    pub fn with_jitter(mut self, max_jitter: Duration) -> Self {
        self.jitter = Some(max_jitter);
        self
    }

    /// Exclude specific dates from the schedule.
    pub fn exclude_date(mut self, date: NaiveDate) -> Self {
        self.blackout_dates.push(date);
        self
    }

    /// Exclude a list of dates.
    pub fn exclude_dates(mut self, dates: Vec<NaiveDate>) -> Self {
        self.blackout_dates.extend(dates);
        self
    }

    // --- Presets ---

    pub fn hourly(mut self) -> Self {
        self.seconds = CronField::Value(0);
        self.minutes = CronField::Value(0);
        self.hours = CronField::All;
        self
    }

    pub fn daily(mut self) -> Self {
        self.seconds = CronField::Value(0);
        self.minutes = CronField::Value(0);
        self.hours = CronField::Value(0);
        self
    }

    pub fn weekly(mut self) -> Self {
        self = self.daily();
        self.day_of_week = CronField::Value(1);
        self
    }

    pub fn monthly(mut self) -> Self {
        self = self.daily();
        self.day_of_month = CronField::Value(1);
        self
    }

    // --- Field Methods ---

    fn validate_range(&mut self, val: u32, min: u32, max: u32, field: &str) {
        if val < min || val > max {
            self.error = Some(format!(
                "Invalid value {} for field {}: must be between {} and {}",
                val, field, min, max
            ));
        }
    }

    pub fn second(mut self, second: u32) -> Self {
        self.validate_range(second, 0, 59, "seconds");
        self.seconds = CronField::Value(second);
        self
    }

    pub fn every_second(mut self) -> Self {
        self.seconds = CronField::All;
        self
    }

    pub fn seconds_interval(mut self, interval: u32) -> Self {
        self.validate_range(interval, 1, 59, "seconds interval");
        self.seconds = CronField::Step(Box::new(CronField::All), interval);
        self
    }

    pub fn minute(mut self, minute: u32) -> Self {
        self.validate_range(minute, 0, 59, "minutes");
        self.minutes = CronField::Value(minute);
        self
    }

    pub fn every_minute(mut self) -> Self {
        self.minutes = CronField::All;
        self
    }

    pub fn minutes_interval(mut self, interval: u32) -> Self {
        self.validate_range(interval, 1, 59, "minutes interval");
        self.minutes = CronField::Step(Box::new(CronField::All), interval);
        self
    }

    pub fn hour(mut self, hour: u32) -> Self {
        self.validate_range(hour, 0, 23, "hours");
        self.hours = CronField::Value(hour);
        self
    }

    pub fn hours_list(mut self, hours: &[u32]) -> Self {
        let mut fields = Vec::new();
        for &h in hours {
            self.validate_range(h, 0, 23, "hours list");
            fields.push(CronField::Value(h));
        }
        self.hours = CronField::List(fields);
        self
    }

    pub fn every_hour(mut self) -> Self {
        self.hours = CronField::All;
        self
    }

    pub fn hours_range(mut self, start: u32, end: u32) -> Self {
        self.validate_range(start, 0, 23, "hours range start");
        self.validate_range(end, 0, 23, "hours range end");
        if start >= end {
            self.error = Some(format!(
                "Hours range start ({}) must be less than end ({})",
                start, end
            ));
        }
        self.hours = CronField::Range(start, end);
        self
    }

    pub fn day_of_month(mut self, day: u32) -> Self {
        self.validate_range(day, 1, 31, "day of month");
        self.day_of_month = CronField::Value(day);
        self
    }

    pub fn month(mut self, month: Month) -> Self {
        self.month = CronField::Value(month as u32);
        self
    }

    pub fn day_of_week(mut self, day: Weekday) -> Self {
        // Cron crate format: Sunday=1, Monday=2, ..., Saturday=7
        // Our enum: Sunday=0, Monday=1, ..., Saturday=6
        let cron_value = (day as u32) + 1;
        self.day_of_week = CronField::Value(cron_value);
        self
    }

    pub fn weekdays_only(mut self) -> Self {
        // Cron crate: Monday=2 through Friday=6
        self.day_of_week = CronField::Range(2, 6);
        self
    }

    pub fn weekends_only(mut self) -> Self {
        // Cron crate: Saturday=7, Sunday=1
        self.day_of_week = CronField::List(vec![CronField::Value(7), CronField::Value(1)]);
        self
    }

    pub fn sundays_only(mut self) -> Self {
        // Cron crate: Sunday=1
        self.day_of_week = CronField::Value(1);
        self
    }

    pub fn mondays_only(mut self) -> Self {
        self.day_of_week = CronField::Value(2);
        self
    }

    pub fn tuesdays_only(mut self) -> Self {
        self.day_of_week = CronField::Value(3);
        self
    }

    pub fn wednesdays_only(mut self) -> Self {
        self.day_of_week = CronField::Value(4);
        self
    }

    pub fn thursdays_only(mut self) -> Self {
        self.day_of_week = CronField::Value(5);
        self
    }

    pub fn fridays_only(mut self) -> Self {
        self.day_of_week = CronField::Value(6);
        self
    }

    pub fn saturdays_only(mut self) -> Self {
        self.day_of_week = CronField::Value(7);
        self
    }

    pub fn quarterly(mut self) -> Self {
        self.month = CronField::List(vec![
            CronField::Value(1),  // January
            CronField::Value(4),  // April
            CronField::Value(7),  // July
            CronField::Value(10), // October
        ]);
        self.day_of_month = CronField::Value(1);
        self.seconds = CronField::Value(0);
        self.minutes = CronField::Value(0);
        self.hours = CronField::Value(0);
        self
    }

    pub fn year(mut self, year: u32) -> Self {
        self.year = CronField::Value(year);
        self
    }

    pub fn build(&self) -> String {
        format!(
            "{} {} {} {} {} {} {}",
            self.seconds,
            self.minutes,
            self.hours,
            self.day_of_month,
            self.month,
            self.day_of_week,
            self.year
        )
    }

    pub fn to_validated(&self) -> CronResult<ValidatedSchedule> {
        if let Some(err) = &self.error {
            return Err(CronError::InvalidSchedule(err.clone()));
        }
        ValidatedSchedule::parse(&self.build())
    }
}

impl fmt::Display for CronExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.build())
    }
}
