// Copyright (c) Meta Platforms, Inc. and affiliates.

//! Timestamp utilities for Unigraph.
//!
//! Provides a [`Timestamp`] wrapper around `chrono::DateTime<Utc>` with
//! convenient constructors, formatting, arithmetic, and date boundary helpers.

use anyhow::Context;
use anyhow::Error;
use anyhow::Result;
use anyhow::format_err;
use chrono::DateTime;
use chrono::Datelike;
use chrono::Duration;
use chrono::Local;
use chrono::NaiveDateTime;
use chrono::TimeZone;
use chrono::Timelike;
use chrono::Utc;
use chrono::Weekday;

pub type TimestampRFC3339 = String;

#[derive(
    Clone,
    Copy,
    Eq,
    Hash,
    Ord,
    serde::Deserialize,
    serde::Serialize,
    PartialEq,
    PartialOrd
)]
#[serde(transparent)]
pub struct Timestamp(DateTime<Utc>);

impl Timestamp {
    #[inline]
    pub fn new(dt: DateTime<Utc>) -> Self {
        Self(dt)
    }

    #[inline]
    pub fn now() -> Self {
        Self(Utc::now())
    }

    #[inline]
    pub fn from_naive(dt: NaiveDateTime) -> Self {
        Self(dt.and_utc())
    }

    pub fn from_rfc3339(src: &str) -> Result<Self> {
        let dt = DateTime::parse_from_rfc3339(src)
            .with_context(|| {
                format!(
                    "Failed to parse timestamp.\nExpected rfc3339 format, e.g. '1996-12-19T16:39:57-08:00'\nInput: '{}'",
                    src
                )
            })?;
        Ok(Self(dt.into()))
    }

    #[inline]
    pub fn to_rfc3339(&self) -> String {
        self.0.to_rfc3339()
    }

    #[inline]
    pub fn to_rfc3339_local(&self) -> String {
        self.into_chrono_local().to_rfc3339()
    }

    /// Produces an rfc3339 string that can be used for ordering timestamps.
    /// This is here to mitigate MySQL SEV S307719.
    /// MySQL can not deal with time. Timestamp and DateTime MySQL types should
    /// never be used. They just don't work.
    /// Since MySQL client treats Timestamp column as `bytes` one of the possible
    /// solution for migration away from Timestamps is hotswapping the existing
    /// timestamp columns with a String column that contains an rfc3339 string.
    /// This will still work, since these strings can be compared to each other
    /// which will result in the correct time ordering.
    pub fn to_comparable_rfc3339_str(&self) -> String {
        let dt: &DateTime<Utc> = &self.0;
        // include fixed number of nanos. 3 should be enough? technically it can
        // be more granular, but AFAIK nothing upstream uses fraction so likely
        // they're all be .000
        // always use Z to signify UTC. +/- offset can technically break formatting
        format!("{}", dt.format("%Y-%m-%dT%H:%M:%S.%3fZ"))
    }

    #[inline]
    pub fn into_chrono(self) -> DateTime<Utc> {
        self.0
    }

    #[inline]
    pub fn from_chrono_local(dt: DateTime<Local>) -> Self {
        Self(dt.into())
    }

    #[inline]
    pub fn into_chrono_local(self) -> DateTime<Local> {
        self.0.into()
    }

    pub fn to_unix_timestamp(&self) -> i64 {
        self.0.timestamp()
    }

    pub fn from_unix_timestamp(ts: i64) -> Self {
        Self(Utc.timestamp_opt(ts, 0).unwrap())
    }

    #[inline]
    pub fn signed_duration_since(self, rhs: Self) -> Duration {
        self.0.signed_duration_since(rhs.0)
    }

    #[inline]
    pub fn checked_add_signed(self, rhs: Duration) -> Option<Self> {
        self.0.checked_add_signed(rhs).map(Self)
    }

    #[inline]
    pub fn checked_sub_signed(self, rhs: Duration) -> Option<Self> {
        self.0.checked_sub_signed(rhs).map(Self)
    }

    pub fn subtract_days(&self, days: usize) -> Result<Self> {
        let duration = chrono::Duration::days(days.try_into()?);
        let ts = self
            .0
            .checked_sub_signed(duration)
            .context("date subtraction failed")?;
        Ok(Self(ts))
    }

    pub fn add_days(&self, days: usize) -> Result<Self> {
        let duration = chrono::Duration::days(days.try_into()?);
        let ts = self
            .0
            .checked_add_signed(duration)
            .context("date addition failed")?;
        Ok(Self(ts))
    }

    pub fn add_minutes(&self, minutes: usize) -> Result<Self> {
        let duration = chrono::Duration::minutes(minutes.try_into()?);
        let ts = self
            .0
            .checked_add_signed(duration)
            .context("date addition failed")?;
        Ok(Self(ts))
    }

    /// Add an arbitrary [`std::time::Duration`] to this timestamp.
    pub fn add_duration(&self, duration: std::time::Duration) -> Result<Self> {
        let chrono_duration = chrono::Duration::from_std(duration)
            .context("duration too large for chrono conversion")?;
        self.0
            .checked_add_signed(chrono_duration)
            .map(Self)
            .context("timestamp overflow when adding duration")
    }

    pub fn day_start(&self) -> Result<Self> {
        let ts = self
            .0
            .with_hour(0)
            .and_then(|ts| ts.with_minute(0))
            .and_then(|ts| ts.with_second(0))
            .and_then(|ts| ts.with_nanosecond(0))
            .ok_or_else(|| format_err!("Failed to get the day start timestamp for {}", self))?;
        Ok(Self(ts))
    }

    pub fn day_end(&self) -> Result<Self> {
        let ts = self
            .0
            .with_hour(23)
            .and_then(|ts| ts.with_minute(59))
            .and_then(|ts| ts.with_second(59))
            .and_then(|ts| ts.with_nanosecond(999_999_999))
            .ok_or_else(|| format_err!("Failed to get the day end timestamp for {}", self))?;
        Ok(Self(ts))
    }

    pub fn weekday(&self) -> Weekday {
        self.0.weekday()
    }

    /// ISO 8601 week number (1–53).
    #[inline]
    pub fn iso_week(&self) -> u32 {
        self.0.iso_week().week()
    }

    /// ISO week-numbering year. Can differ from the calendar year at year
    /// boundaries (e.g. 2024-12-31 may belong to ISO year 2025).
    #[inline]
    pub fn iso_week_year(&self) -> i32 {
        self.0.iso_week().year()
    }

    /// Useful for aggregating dates by week.
    /// Calling this on multiple Timestamps within one week will always return
    /// 00:00:00 of the preceding Monday which can later be used as a map key.
    pub fn week_start(&self) -> Result<Self> {
        let weekday = self.0.weekday();
        let days_to_subtract = match weekday {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        };
        self.subtract_days(days_to_subtract)?.day_start()
    }

    /// MySQL has a special datetime format that it accepts and returns. These are different
    /// from rfc3339 and these functions cast to and from those formats.
    pub fn to_mysql_format(&self) -> String {
        let local = self.into_chrono_local();
        local.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    pub fn from_mysql_format(s: &str) -> Result<Self> {
        let naive_date_time = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
            .with_context(|| format!("Failed to parse timestamp. input: `{}`", s))?;
        chrono::Local
            .from_local_datetime(&naive_date_time)
            .earliest()
            .ok_or_else(|| anyhow::anyhow!("Failed to convert NaiveDateTime to Local DateTime"))
            .map(Timestamp::from_chrono_local)
    }
}

impl From<Timestamp> for DateTime<Utc> {
    #[inline]
    fn from(ts: Timestamp) -> Self {
        ts.into_chrono()
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{}", self.0)
    }
}

impl std::fmt::Debug for Timestamp {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(fmt, "{:?}", self.0)
    }
}

impl std::ops::Add<Duration> for Timestamp {
    type Output = Timestamp;

    #[inline]
    fn add(self, rhs: Duration) -> Timestamp {
        Timestamp(self.into_chrono() + rhs)
    }
}

impl std::ops::Sub<Duration> for Timestamp {
    type Output = Timestamp;

    #[inline]
    fn sub(self, rhs: Duration) -> Timestamp {
        Timestamp(self.into_chrono() - rhs)
    }
}

impl std::str::FromStr for Timestamp {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let dt = DateTime::from_str(s).with_context(|| "Failed to parse timestamp from string")?;
        Ok(Self(dt))
    }
}

#[cfg(test)]
mod tests {
    use k9::*;

    use super::*;

    #[test]
    fn test_comparable_rfc3339() {
        let timestamp_strs = [
            "2000-11-12T00:24:20-00:00",
            "2000-11-12T00:24:20-05:29", // earlier timezone. sorts later
            "2022-11-12T00:24:20-00:00", // specified offset
            "2022-11-12T00:24:20Z",      // Z offset that signifies UTC
            "2022-12-12T00:24:20Z",
            "2022-12-12T00:24:20.001Z", // handles fractions
            "2022-12-12T00:24:20.002Z",
            "2022-12-12T00:24:20.003Z",
            "2022-12-12T00:24:20.004+00:00",
            "2022-12-12T01:24:20.004+01:00",
        ];

        let timestamps = timestamp_strs
            .iter()
            .map(|s| Timestamp::from_rfc3339(s).unwrap())
            .collect::<Vec<_>>();

        let mut strs = timestamps
            .iter()
            .map(|ts| ts.to_comparable_rfc3339_str())
            .collect::<Vec<_>>();

        strs.sort_unstable();

        snapshot!(
            &strs,
            r#"
[
    "2000-11-12T00:24:20.000Z",
    "2000-11-12T05:53:20.000Z",
    "2022-11-12T00:24:20.000Z",
    "2022-11-12T00:24:20.000Z",
    "2022-12-12T00:24:20.000Z",
    "2022-12-12T00:24:20.001Z",
    "2022-12-12T00:24:20.002Z",
    "2022-12-12T00:24:20.003Z",
    "2022-12-12T00:24:20.004Z",
    "2022-12-12T00:24:20.004Z",
]
"#
        );

        let parsed = strs
            .iter()
            .map(|s| Timestamp::from_rfc3339(s).unwrap())
            .collect::<Vec<_>>();

        // They should be properly ordered
        assert_equal!(&timestamps, &parsed);
    }

    #[test]
    fn test_week_start() -> Result<()> {
        #[derive(Debug)]
        struct D {
            ts: &'static str,
            expected_weekday: Weekday,
            expected_week_start: &'static str,
        }
        let data = vec![
            D {
                ts: "2024-07-02T00:00:20Z",
                expected_weekday: Weekday::Tue,
                expected_week_start: "2024-07-01T00:00:00.000Z",
            },
            D {
                ts: "2024-07-02T10:00:20Z",
                expected_weekday: Weekday::Tue,
                expected_week_start: "2024-07-01T00:00:00.000Z",
            },
            D {
                ts: "2024-07-04T10:00:20Z",
                expected_weekday: Weekday::Thu,
                expected_week_start: "2024-07-01T00:00:00.000Z",
            },
            D {
                ts: "2024-06-30T10:00:20Z",
                expected_weekday: Weekday::Sun,
                expected_week_start: "2024-06-24T00:00:00.000Z",
            },
        ];

        for d in data {
            let D {
                ts,
                expected_weekday,
                expected_week_start,
            } = d;
            let d_debug = format!("{:#?}", &d);

            let ts = Timestamp::from_rfc3339(ts)?;
            assert_equal!(
                ts.weekday(),
                expected_weekday,
                "Incorrect week day {}",
                &d_debug
            );
            let week_start = ts.week_start()?;
            assert_equal!(
                week_start.weekday(),
                Weekday::Mon,
                "Incorrect week start. {}",
                &d_debug
            );
            assert_equal!(
                week_start.to_comparable_rfc3339_str(),
                expected_week_start,
                "{}",
                &d_debug
            );
        }
        Ok(())
    }

    #[test]
    fn test_iso_week() -> Result<()> {
        // Mid-year: straightforward
        let ts = Timestamp::from_rfc3339("2024-07-02T10:00:00Z")?;
        assert_equal!(ts.iso_week(), 27);
        assert_equal!(ts.iso_week_year(), 2024);

        // Year boundary: Dec 31, 2024 is a Tuesday → ISO week 1 of 2025
        let ts = Timestamp::from_rfc3339("2024-12-31T00:00:00Z")?;
        assert_equal!(ts.iso_week(), 1);
        assert_equal!(ts.iso_week_year(), 2025);

        // Year boundary: Jan 1, 2025 is a Wednesday → also ISO week 1 of 2025
        let ts = Timestamp::from_rfc3339("2025-01-01T00:00:00Z")?;
        assert_equal!(ts.iso_week(), 1);
        assert_equal!(ts.iso_week_year(), 2025);

        // Dec 28, 2024 is a Saturday → still ISO week 52 of 2024
        let ts = Timestamp::from_rfc3339("2024-12-28T00:00:00Z")?;
        assert_equal!(ts.iso_week(), 52);
        assert_equal!(ts.iso_week_year(), 2024);

        // W53: 2020-12-31 is a Thursday → ISO week 53 of 2020
        let ts = Timestamp::from_rfc3339("2020-12-31T00:00:00Z")?;
        assert_equal!(ts.iso_week(), 53);
        assert_equal!(ts.iso_week_year(), 2020);

        Ok(())
    }

    #[test]
    fn unix_timestamp() -> Result<()> {
        let ts = Timestamp::from_rfc3339("2024-06-24T00:00:00.000Z")?;
        let unix = ts.to_unix_timestamp();
        assert_equal!(unix, 1719187200);

        let roundrip = Timestamp::from_unix_timestamp(unix);
        assert_equal!(ts, roundrip);
        Ok(())
    }

    #[test]
    fn add_duration() -> Result<()> {
        let ts = Timestamp::from_rfc3339("2024-06-24T00:00:00.000Z")?;

        let plus_1s = ts.add_duration(std::time::Duration::from_secs(1))?;
        assert_equal!(
            plus_1s.to_comparable_rfc3339_str(),
            "2024-06-24T00:00:01.000Z"
        );

        let plus_1h = ts.add_duration(std::time::Duration::from_secs(3600))?;
        assert_equal!(
            plus_1h.to_comparable_rfc3339_str(),
            "2024-06-24T01:00:00.000Z"
        );

        let plus_500ms = ts.add_duration(std::time::Duration::from_millis(500))?;
        assert_equal!(
            plus_500ms.to_comparable_rfc3339_str(),
            "2024-06-24T00:00:00.500Z"
        );

        Ok(())
    }
}
