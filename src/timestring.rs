//! Flexible time string parsing.
//!
//! Parses human-readable time strings into timestamps or time ranges.
//! The caller provides the timezone for interpreting ambiguous times.
//!
//! # Supported Formats
//!
//! - **Relative durations**: `"2 weeks ago"`, `"1 year ago"`, `"a month ago"`
//! - **Named periods**: `"last month"`, `"this year"`, `"yesterday"`, `"today"`, `"last week"`, `"this week"`
//! - **Month names**: `"January"`, `"Jan"` (resolves to most recent completed/ongoing instance)
//! - **Year only**: `"2025"` (entire year range)
//! - **American dates**: `"1/15/2025"`, `"01/15/2025"` (MM/DD/YYYY)
//! - **Human dates**: `"Jan 15, 2025"`, `"January 15th, 2025"`
//! - **ISO dates**: `"2025-01-15"` (date only, treated as full day)
//! - **ISO8601**: `"2024-01-01T00:00:00Z"`
//! - **Unix milliseconds**: `"1704067200000"` (must be > 4 digits)

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone};
use chrono_tz::Tz;
use regex::Regex;

/// Represents a parsed time specification - either a specific moment or a range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeSpec {
    /// A specific moment in time (milliseconds since epoch)
    Moment(i64),
    /// A range of time (start and end in milliseconds since epoch, inclusive)
    Range { start: i64, end: i64 },
}

impl TimeSpec {
    /// Get the timestamp to use for "after" comparisons.
    /// For a moment, returns that moment. For a range, returns the end.
    pub fn for_after(&self) -> i64 {
        match self {
            TimeSpec::Moment(ts) => *ts,
            TimeSpec::Range { end, .. } => *end,
        }
    }

    /// Get the timestamp to use for "before" comparisons.
    /// For a moment, returns that moment. For a range, returns the start.
    pub fn for_before(&self) -> i64 {
        match self {
            TimeSpec::Moment(ts) => *ts,
            TimeSpec::Range { start, .. } => *start,
        }
    }

    /// Check if this is a range (required for "during")
    pub fn is_range(&self) -> bool {
        matches!(self, TimeSpec::Range { .. })
    }

    /// Check if a timestamp (in millis) falls within this time spec.
    /// For a moment, checks equality. For a range, checks inclusive bounds.
    pub fn contains(&self, timestamp_ms: i64) -> bool {
        match self {
            TimeSpec::Moment(ts) => timestamp_ms == *ts,
            TimeSpec::Range { start, end } => timestamp_ms >= *start && timestamp_ms <= *end,
        }
    }
}

/// Parse a flexible time string into a TimeSpec.
///
/// # Arguments
/// * `input` - The time string to parse
/// * `now` - The current time to use for relative calculations
/// * `tz` - The timezone to use for interpreting ambiguous times
pub fn parse(input: &str, now: DateTime<Tz>, tz: Tz) -> Result<TimeSpec, String> {
    let input = input.trim();
    let input_lower = input.to_lowercase();

    // Try ISO8601/RFC3339 first (timezone-aware, doesn't need tz parameter)
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Ok(TimeSpec::Moment(dt.timestamp_millis()));
    }

    // Relative durations: "N weeks ago", "N days ago", etc.
    if let Some(spec) = try_parse_relative_duration(&input_lower, now) {
        return Ok(spec);
    }

    // Named periods: "last month", "last year", "this month", "this year"
    if let Some(spec) = try_parse_named_period(&input_lower, now, tz) {
        return Ok(spec);
    }

    // Month names: "January", "February", etc.
    if let Some(spec) = try_parse_month_name(&input_lower, now, tz) {
        return Ok(spec);
    }

    // Year only: "2025" (must come before unix timestamp check)
    if let Some(spec) = try_parse_year_only(input, tz) {
        return Ok(spec);
    }

    // Try Unix timestamp in milliseconds (must be > 4 digits to avoid matching years)
    if input.len() > 4 {
        if let Ok(ts) = input.parse::<i64>() {
            return Ok(TimeSpec::Moment(ts));
        }
    }

    // American date format: "1/15/2025", "01/15/2025"
    if let Some(spec) = try_parse_american_date(input, tz) {
        return Ok(spec);
    }

    // Human-readable date: "Jan 15, 2025", "Jan 15th, 2025", "January 15, 2025"
    if let Some(spec) = try_parse_human_date(input, tz) {
        return Ok(spec);
    }

    // ISO date without time: "2025-01-15"
    if let Some(spec) = try_parse_iso_date(input, tz) {
        return Ok(spec);
    }

    Err(format!("Could not parse time string: {}", input))
}

fn try_parse_relative_duration(input: &str, now: DateTime<Tz>) -> Option<TimeSpec> {
    // Patterns: "N week(s) ago", "N day(s) ago", "N month(s) ago", "N year(s) ago"
    let re = Regex::new(r"^(\d+)\s+(second|minute|hour|day|week|month|year)s?\s+ago$").ok()?;

    if let Some(caps) = re.captures(input) {
        let n: i64 = caps.get(1)?.as_str().parse().ok()?;
        let unit = caps.get(2)?.as_str();

        let target = match unit {
            "second" => now - Duration::seconds(n),
            "minute" => now - Duration::minutes(n),
            "hour" => now - Duration::hours(n),
            "day" => now - Duration::days(n),
            "week" => now - Duration::weeks(n),
            "month" => {
                // Approximate: go back n months
                let mut year = now.year();
                let mut month = now.month() as i32 - n as i32;
                while month <= 0 {
                    month += 12;
                    year -= 1;
                }
                now.with_year(year)?.with_month(month as u32)?
            }
            "year" => now.with_year(now.year() - n as i32)?,
            _ => return None,
        };

        return Some(TimeSpec::Moment(target.timestamp_millis()));
    }

    // Also support "a week ago", "a month ago", etc.
    let re_single = Regex::new(r"^a\s+(second|minute|hour|day|week|month|year)\s+ago$").ok()?;
    if let Some(caps) = re_single.captures(input) {
        let unit = caps.get(1)?.as_str();
        let target = match unit {
            "second" => now - Duration::seconds(1),
            "minute" => now - Duration::minutes(1),
            "hour" => now - Duration::hours(1),
            "day" => now - Duration::days(1),
            "week" => now - Duration::weeks(1),
            "month" => {
                let mut year = now.year();
                let mut month = now.month() as i32 - 1;
                if month <= 0 {
                    month += 12;
                    year -= 1;
                }
                now.with_year(year)?.with_month(month as u32)?
            }
            "year" => now.with_year(now.year() - 1)?,
            _ => return None,
        };
        return Some(TimeSpec::Moment(target.timestamp_millis()));
    }

    None
}

fn try_parse_named_period(input: &str, now: DateTime<Tz>, tz: Tz) -> Option<TimeSpec> {
    match input {
        "last month" => {
            let mut year = now.year();
            let mut month = now.month() as i32 - 1;
            if month <= 0 {
                month = 12;
                year -= 1;
            }
            let start = tz
                .with_ymd_and_hms(year, month as u32, 1, 0, 0, 0)
                .single()?;
            let end_month = if month == 12 { 1 } else { month + 1 };
            let end_year = if month == 12 { year + 1 } else { year };
            let end = tz
                .with_ymd_and_hms(end_year, end_month as u32, 1, 0, 0, 0)
                .single()?
                - Duration::milliseconds(1);
            Some(TimeSpec::Range {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
            })
        }
        "this month" => {
            let start = tz
                .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
                .single()?;
            let next_month = if now.month() == 12 {
                1
            } else {
                now.month() + 1
            };
            let next_year = if now.month() == 12 {
                now.year() + 1
            } else {
                now.year()
            };
            let end = tz
                .with_ymd_and_hms(next_year, next_month, 1, 0, 0, 0)
                .single()?
                - Duration::milliseconds(1);
            Some(TimeSpec::Range {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
            })
        }
        "last year" => {
            let year = now.year() - 1;
            let start = tz.with_ymd_and_hms(year, 1, 1, 0, 0, 0).single()?;
            let end =
                tz.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).single()? - Duration::milliseconds(1);
            Some(TimeSpec::Range {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
            })
        }
        "this year" => {
            let year = now.year();
            let start = tz.with_ymd_and_hms(year, 1, 1, 0, 0, 0).single()?;
            let end =
                tz.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).single()? - Duration::milliseconds(1);
            Some(TimeSpec::Range {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
            })
        }
        "today" => {
            let start = tz
                .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
                .single()?;
            let end = start + Duration::days(1) - Duration::milliseconds(1);
            Some(TimeSpec::Range {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
            })
        }
        "yesterday" => {
            let yesterday = now - Duration::days(1);
            let start = tz
                .with_ymd_and_hms(
                    yesterday.year(),
                    yesterday.month(),
                    yesterday.day(),
                    0,
                    0,
                    0,
                )
                .single()?;
            let end = start + Duration::days(1) - Duration::milliseconds(1);
            Some(TimeSpec::Range {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
            })
        }
        "last week" => {
            // Last week = Sunday through Saturday before the current week
            let days_since_sunday = now.weekday().num_days_from_sunday() as i64;
            let this_sunday = now - Duration::days(days_since_sunday);
            let last_sunday = this_sunday - Duration::days(7);
            let start = tz
                .with_ymd_and_hms(
                    last_sunday.year(),
                    last_sunday.month(),
                    last_sunday.day(),
                    0,
                    0,
                    0,
                )
                .single()?;
            // End is Saturday (this_sunday - 1 day, end of day)
            let end = tz
                .with_ymd_and_hms(
                    this_sunday.year(),
                    this_sunday.month(),
                    this_sunday.day(),
                    0,
                    0,
                    0,
                )
                .single()?
                - Duration::milliseconds(1);
            Some(TimeSpec::Range {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
            })
        }
        "this week" => {
            // This week = Sunday through Saturday
            let days_since_sunday = now.weekday().num_days_from_sunday() as i64;
            let sunday = now - Duration::days(days_since_sunday);
            let start = tz
                .with_ymd_and_hms(sunday.year(), sunday.month(), sunday.day(), 0, 0, 0)
                .single()?;
            let next_sunday = sunday + Duration::days(7);
            let end = tz
                .with_ymd_and_hms(
                    next_sunday.year(),
                    next_sunday.month(),
                    next_sunday.day(),
                    0,
                    0,
                    0,
                )
                .single()?
                - Duration::milliseconds(1);
            Some(TimeSpec::Range {
                start: start.timestamp_millis(),
                end: end.timestamp_millis(),
            })
        }
        _ => None,
    }
}

fn month_name_to_num(name: &str) -> Option<u32> {
    match name {
        "january" | "jan" => Some(1),
        "february" | "feb" => Some(2),
        "march" | "mar" => Some(3),
        "april" | "apr" => Some(4),
        "may" => Some(5),
        "june" | "jun" => Some(6),
        "july" | "jul" => Some(7),
        "august" | "aug" => Some(8),
        "september" | "sep" | "sept" => Some(9),
        "october" | "oct" => Some(10),
        "november" | "nov" => Some(11),
        "december" | "dec" => Some(12),
        _ => None,
    }
}

fn try_parse_month_name(input: &str, now: DateTime<Tz>, tz: Tz) -> Option<TimeSpec> {
    // Just a month name like "January" or "jan"
    let month_num = month_name_to_num(input)?;

    // Find the most recent completed or ongoing instance of this month
    let mut year = now.year();
    if month_num > now.month() {
        // This month hasn't happened yet this year, use last year
        year -= 1;
    }

    let start = tz.with_ymd_and_hms(year, month_num, 1, 0, 0, 0).single()?;
    let next_month = if month_num == 12 { 1 } else { month_num + 1 };
    let next_year = if month_num == 12 { year + 1 } else { year };
    let end = tz
        .with_ymd_and_hms(next_year, next_month, 1, 0, 0, 0)
        .single()?
        - Duration::milliseconds(1);

    Some(TimeSpec::Range {
        start: start.timestamp_millis(),
        end: end.timestamp_millis(),
    })
}

fn try_parse_year_only(input: &str, tz: Tz) -> Option<TimeSpec> {
    // Just a 4-digit year like "2025"
    let re = Regex::new(r"^(\d{4})$").ok()?;
    let caps = re.captures(input)?;
    let year: i32 = caps.get(1)?.as_str().parse().ok()?;

    let start = tz.with_ymd_and_hms(year, 1, 1, 0, 0, 0).single()?;
    let end = tz.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).single()? - Duration::milliseconds(1);

    Some(TimeSpec::Range {
        start: start.timestamp_millis(),
        end: end.timestamp_millis(),
    })
}

fn try_parse_american_date(input: &str, tz: Tz) -> Option<TimeSpec> {
    // MM/DD/YYYY or M/D/YYYY
    let re = Regex::new(r"^(\d{1,2})/(\d{1,2})/(\d{4})$").ok()?;
    let caps = re.captures(input)?;

    let month: u32 = caps.get(1)?.as_str().parse().ok()?;
    let day: u32 = caps.get(2)?.as_str().parse().ok()?;
    let year: i32 = caps.get(3)?.as_str().parse().ok()?;

    if month < 1 || month > 12 || day < 1 || day > 31 {
        return None;
    }

    let start = tz.with_ymd_and_hms(year, month, day, 0, 0, 0).single()?;
    let end = start + Duration::days(1) - Duration::milliseconds(1);

    Some(TimeSpec::Range {
        start: start.timestamp_millis(),
        end: end.timestamp_millis(),
    })
}

fn try_parse_human_date(input: &str, tz: Tz) -> Option<TimeSpec> {
    // "Jan 15, 2025", "Jan 15th, 2025", "January 15, 2025", "January 15th 2025"
    let re = Regex::new(
        r"(?i)^(january|february|march|april|may|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sep|sept|oct|nov|dec)\s+(\d{1,2})(?:st|nd|rd|th)?,?\s+(\d{4})$"
    ).ok()?;

    let caps = re.captures(input)?;
    let month_name = caps.get(1)?.as_str().to_lowercase();
    let day: u32 = caps.get(2)?.as_str().parse().ok()?;
    let year: i32 = caps.get(3)?.as_str().parse().ok()?;

    let month = month_name_to_num(&month_name)?;

    if day < 1 || day > 31 {
        return None;
    }

    let start = tz.with_ymd_and_hms(year, month, day, 0, 0, 0).single()?;
    let end = start + Duration::days(1) - Duration::milliseconds(1);

    Some(TimeSpec::Range {
        start: start.timestamp_millis(),
        end: end.timestamp_millis(),
    })
}

fn try_parse_iso_date(input: &str, tz: Tz) -> Option<TimeSpec> {
    // "2025-01-15" (date only, no time)
    let date = NaiveDate::parse_from_str(input, "%Y-%m-%d").ok()?;
    let start = tz
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()?;
    let end = start + Duration::days(1) - Duration::milliseconds(1);

    Some(TimeSpec::Range {
        start: start.timestamp_millis(),
        end: end.timestamp_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::America::New_York;

    const TEST_TZ: Tz = New_York;

    // Helper: create a fixed "now" for deterministic tests
    // Wednesday, January 15, 2025, 12:00:00 EST
    fn test_now() -> DateTime<Tz> {
        TEST_TZ
            .with_ymd_and_hms(2025, 1, 15, 12, 0, 0)
            .single()
            .unwrap()
    }

    // Helper: get timestamp for a specific date at midnight in test timezone
    fn ts(year: i32, month: u32, day: u32) -> i64 {
        TEST_TZ
            .with_ymd_and_hms(year, month, day, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis()
    }

    // Helper: get end-of-day timestamp (23:59:59.999)
    fn ts_end(year: i32, month: u32, day: u32) -> i64 {
        ts(year, month, day) + 24 * 60 * 60 * 1000 - 1
    }

    #[test]
    fn test_parse_year_2025() {
        let now = test_now();
        let result = parse("2025", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 1),
                end: ts_end(2025, 12, 31),
            }
        );
    }

    #[test]
    fn test_parse_year_2024() {
        let now = test_now();
        let result = parse("2024", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2024, 1, 1),
                end: ts_end(2024, 12, 31),
            }
        );
    }

    #[test]
    fn test_parse_american_date() {
        let now = test_now();
        let result = parse("1/15/2025", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 15),
                end: ts_end(2025, 1, 15),
            }
        );

        let result = parse("12/31/2024", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2024, 12, 31),
                end: ts_end(2024, 12, 31),
            }
        );
    }

    #[test]
    fn test_parse_human_date() {
        let now = test_now();
        let result = parse("Jan 15, 2025", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 15),
                end: ts_end(2025, 1, 15),
            }
        );

        let result = parse("January 15th 2025", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 15),
                end: ts_end(2025, 1, 15),
            }
        );
    }

    #[test]
    fn test_parse_iso_date() {
        let now = test_now();
        let result = parse("2025-01-15", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 15),
                end: ts_end(2025, 1, 15),
            }
        );
    }

    #[test]
    fn test_parse_unix_timestamp() {
        let now = test_now();
        let result = parse("1704067200000", now, TEST_TZ).unwrap();
        assert_eq!(result, TimeSpec::Moment(1704067200000));
    }

    #[test]
    fn test_parse_iso8601() {
        let now = test_now();
        // 2024-01-01T00:00:00Z = midnight UTC (timezone in string, tz param ignored)
        let result = parse("2024-01-01T00:00:00Z", now, TEST_TZ).unwrap();
        let expected = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(result, TimeSpec::Moment(expected));
    }

    #[test]
    fn test_parse_today() {
        let now = test_now();
        let result = parse("today", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 15),
                end: ts_end(2025, 1, 15),
            }
        );
    }

    #[test]
    fn test_parse_yesterday() {
        let now = test_now();
        let result = parse("yesterday", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 14),
                end: ts_end(2025, 1, 14),
            }
        );
    }

    #[test]
    fn test_parse_this_month() {
        let now = test_now();
        let result = parse("this month", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 1),
                end: ts_end(2025, 1, 31),
            }
        );
    }

    #[test]
    fn test_parse_last_month() {
        let now = test_now();
        let result = parse("last month", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2024, 12, 1),
                end: ts_end(2024, 12, 31),
            }
        );
    }

    #[test]
    fn test_parse_this_year() {
        let now = test_now();
        let result = parse("this year", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 1),
                end: ts_end(2025, 12, 31),
            }
        );
    }

    #[test]
    fn test_parse_last_year() {
        let now = test_now();
        let result = parse("last year", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2024, 1, 1),
                end: ts_end(2024, 12, 31),
            }
        );
    }

    #[test]
    fn test_parse_this_week() {
        // "now" is Wed Jan 15, 2025
        // This week = Sun Jan 12 through Sat Jan 18
        let now = test_now();
        let result = parse("this week", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 12),
                end: ts_end(2025, 1, 18),
            }
        );
    }

    #[test]
    fn test_parse_last_week() {
        // "now" is Wed Jan 15, 2025
        // Last week = Sun Jan 5 through Sat Jan 11
        let now = test_now();
        let result = parse("last week", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 5),
                end: ts_end(2025, 1, 11),
            }
        );
    }

    #[test]
    fn test_parse_month_name_january_in_january() {
        // "now" is Jan 15, 2025 -> "january" = Jan 2025 (current/ongoing)
        let now = test_now();
        let result = parse("january", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2025, 1, 1),
                end: ts_end(2025, 1, 31),
            }
        );
    }

    #[test]
    fn test_parse_month_name_december_in_january() {
        // "now" is Jan 15, 2025 -> "december" = Dec 2024 (most recent completed)
        let now = test_now();
        let result = parse("december", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2024, 12, 1),
                end: ts_end(2024, 12, 31),
            }
        );
    }

    #[test]
    fn test_parse_month_name_march_in_january() {
        // "now" is Jan 15, 2025 -> "march" = March 2024 (hasn't happened yet in 2025)
        let now = test_now();
        let result = parse("march", now, TEST_TZ).unwrap();
        assert_eq!(
            result,
            TimeSpec::Range {
                start: ts(2024, 3, 1),
                end: ts_end(2024, 3, 31),
            }
        );
    }

    #[test]
    fn test_parse_relative_1_week_ago() {
        // "now" is Wed Jan 15, 2025 12:00 -> 1 week ago = Wed Jan 8, 2025 12:00
        let now = test_now();
        let result = parse("1 week ago", now, TEST_TZ).unwrap();
        let expected = (now - Duration::weeks(1)).timestamp_millis();
        assert_eq!(result, TimeSpec::Moment(expected));
    }

    #[test]
    fn test_parse_relative_2_days_ago() {
        let now = test_now();
        let result = parse("2 days ago", now, TEST_TZ).unwrap();
        let expected = (now - Duration::days(2)).timestamp_millis();
        assert_eq!(result, TimeSpec::Moment(expected));
    }

    #[test]
    fn test_parse_relative_a_month_ago() {
        // "now" is Jan 15, 2025 -> a month ago = Dec 15, 2024
        let now = test_now();
        let result = parse("a month ago", now, TEST_TZ).unwrap();
        let expected = TEST_TZ
            .with_ymd_and_hms(2024, 12, 15, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        assert_eq!(result, TimeSpec::Moment(expected));
    }

    #[test]
    fn test_timespec_for_after() {
        let moment = TimeSpec::Moment(1000);
        assert_eq!(moment.for_after(), 1000);

        let range = TimeSpec::Range {
            start: 1000,
            end: 2000,
        };
        assert_eq!(range.for_after(), 2000);
    }

    #[test]
    fn test_timespec_for_before() {
        let moment = TimeSpec::Moment(1000);
        assert_eq!(moment.for_before(), 1000);

        let range = TimeSpec::Range {
            start: 1000,
            end: 2000,
        };
        assert_eq!(range.for_before(), 1000);
    }

    #[test]
    fn test_timespec_contains() {
        let range = TimeSpec::Range {
            start: 1000,
            end: 2000,
        };
        assert!(range.contains(1000));
        assert!(range.contains(1500));
        assert!(range.contains(2000));
        assert!(!range.contains(999));
        assert!(!range.contains(2001));
    }

    #[test]
    fn test_invalid_inputs() {
        let now = test_now();
        assert!(parse("not a date", now, TEST_TZ).is_err());
        assert!(parse("", now, TEST_TZ).is_err());
        assert!(parse("13/45/2025", now, TEST_TZ).is_err()); // invalid month/day
    }
}
