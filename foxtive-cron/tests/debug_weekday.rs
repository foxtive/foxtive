use chrono::{Datelike, TimeZone, Timelike, Utc};
use chrono_tz::UTC;

#[test]
fn debug_sunday_schedule() {
    // Test ISO 8601: Sunday=7
    let schedule = foxtive_cron::contracts::ValidatedSchedule::parse("0 0 10 * * 7 *").unwrap();
    println!("Parsed ISO 8601 schedule successfully (Sunday=7)");

    // Start from Saturday Jan 6, 2024 at noon
    let saturday = Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap();
    println!("Starting from: {} ({})", saturday, saturday.weekday());

    let next = schedule.next_after(&saturday, UTC);
    match next {
        Some(dt) => {
            println!("Next execution: {} ({})", dt, dt.weekday());
            println!(
                "Day: {}, Hour: {}, Minute: {}",
                dt.day(),
                dt.hour(),
                dt.minute()
            );
        }
        None => {
            println!("No next execution found!");
        }
    }
}
