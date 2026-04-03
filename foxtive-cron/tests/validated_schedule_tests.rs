use foxtive_cron::contracts::ValidatedSchedule;

#[test]
fn accepts_valid_seven_field_expression() {
    assert!(ValidatedSchedule::parse("*/5 * * * * * *").is_ok());
}

#[test]
fn accepts_every_second_expression() {
    assert!(ValidatedSchedule::parse("* * * * * * *").is_ok());
}

#[test]
fn accepts_specific_time_expression() {
    // Every day at 03:30:00
    assert!(ValidatedSchedule::parse("0 30 3 * * * *").is_ok());
}

#[test]
fn rejects_empty_expression() {
    assert!(ValidatedSchedule::parse("").is_err());
}

#[test]
fn rejects_nonsense_expression() {
    assert!(ValidatedSchedule::parse("not a cron expression").is_err());
}

#[test]
fn rejects_out_of_range_field() {
    // Seconds field goes 0-59; 99 is invalid.
    assert!(ValidatedSchedule::parse("99 * * * * * *").is_err());
}

#[test]
fn error_message_includes_original_expression() {
    let err = ValidatedSchedule::parse("bad expr").unwrap_err();
    assert!(err.to_string().contains("bad expr"));
}
