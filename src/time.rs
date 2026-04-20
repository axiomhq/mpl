//! Time relateded types for `MPL`
use std::{
    cmp,
    num::{NonZeroU32, NonZeroU64, TryFromIntError},
    ops::{self, Add, AddAssign, Div, Mul, Rem, Sub},
    time::Duration,
};

use chrono::{DateTime, Utc};

/// Resolution
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct Resolution(NonZeroU64);

impl Resolution {
    /// The largest value that can be represented
    pub const MAX: Self = Resolution(NonZeroU64::MAX);
    /// Translates the resolution to a `Timestamp`.
    #[must_use]
    pub fn as_timestamp(self) -> Timestamp {
        Timestamp(self.0.get())
    }

    /// trues to convert the secs into a u32 index
    pub fn as_idx(self) -> Result<u32, TryFromIntError> {
        self.try_into()
    }

    /// translates the resolution into a float
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn as_f64(self) -> f64 {
        self.0.get() as f64
    }

    /// Creates a new `Resolution` from a number of seconds.
    pub fn secs(s: u64) -> Result<Self, ResolutionError> {
        NonZeroU64::new(s)
            .map(Resolution)
            .ok_or(ResolutionError::ZeroResolution)
    }

    /// The resolution in seconds as u64.
    #[must_use]
    pub fn as_u64(self) -> u64 {
        self.0.get()
    }

    /// Aligns this resolution up to match the given `Resolution`.
    #[must_use]
    pub fn align_up_to(&self, align_up_to: Resolution) -> Self {
        Self(self.0.div_ceil(align_up_to.0).saturating_mul(align_up_to.0))
    }
}

impl PartialEq<u64> for Resolution {
    fn eq(&self, other: &u64) -> bool {
        self.0.get() == *other
    }
}
impl TryInto<u32> for Resolution {
    type Error = TryFromIntError;

    fn try_into(self) -> Result<u32, Self::Error> {
        self.0.get().try_into()
    }
}

impl Default for Resolution {
    fn default() -> Self {
        Self(NonZeroU64::MIN)
    }
}

impl Rem for Resolution {
    type Output = u64;

    fn rem(self, rhs: Self) -> Self::Output {
        self.0.get() % rhs.0.get()
    }
}

impl Div for Resolution {
    type Output = u64;

    fn div(self, rhs: Self) -> u64 {
        self.0.get() / rhs.0.get()
    }
}

impl Div<Resolution> for Timestamp {
    type Output = Timestamp;

    fn div(self, rhs: Resolution) -> Timestamp {
        Timestamp(self.0 / rhs.0.get())
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}s", self.0.get())
    }
}

/// Errors when dealing with resolutions.
#[derive(Debug, thiserror::Error, Clone, Copy)]
pub enum ResolutionError {
    /// The resolution cannot be zero.
    #[error("resolution cannot be zero")]
    ZeroResolution,
}

impl TryInto<i64> for Resolution {
    type Error = TryFromIntError;

    fn try_into(self) -> Result<i64, Self::Error> {
        self.0.get().try_into()
    }
}

impl PartialEq<Timestamp> for Resolution {
    fn eq(&self, other: &Timestamp) -> bool {
        self.0.get() == other.0
    }
}

impl PartialOrd<Timestamp> for Resolution {
    fn partial_cmp(&self, other: &Timestamp) -> Option<std::cmp::Ordering> {
        Some(self.0.get().cmp(&other.0))
    }
}

impl Mul<NonZeroU32> for Resolution {
    type Output = Self;

    fn mul(self, rhs: NonZeroU32) -> Self::Output {
        Resolution(self.0.saturating_mul(NonZeroU64::from(rhs)))
    }
}

impl PartialEq<Resolution> for Timestamp {
    fn eq(&self, other: &Resolution) -> bool {
        self.0 == other.0.get()
    }
}

impl PartialOrd<Resolution> for Timestamp {
    fn partial_cmp(&self, other: &Resolution) -> Option<std::cmp::Ordering> {
        Some(self.0.cmp(&other.0.get()))
    }
}

impl From<Resolution> for NonZeroU64 {
    fn from(value: Resolution) -> Self {
        value.0
    }
}

impl TryFrom<Resolution> for NonZeroU32 {
    type Error = TryFromIntError;

    fn try_from(value: Resolution) -> Result<Self, Self::Error> {
        value.0.try_into()
    }
}

/// Timestamp

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    serde::Deserialize,
    serde::Serialize,
)]
#[cfg_attr(feature = "bincode", derive(bincode::Encode, bincode::Decode))]
pub struct Timestamp(pub u64);

impl From<Timestamp> for DateTime<Utc> {
    #[allow(clippy::cast_possible_wrap)]
    fn from(value: Timestamp) -> Self {
        DateTime::<Utc>::from_timestamp(value.0 as i64, 0).unwrap_or_default()
    }
}

impl TryFrom<Timestamp> for i64 {
    type Error = TryFromIntError;

    #[allow(clippy::cast_possible_wrap)]
    fn try_from(value: Timestamp) -> Result<Self, Self::Error> {
        value.0.try_into()
    }
}

impl Add<usize> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: usize) -> Self::Output {
        Timestamp(self.0 + rhs as u64)
    }
}

impl Add<u64> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: u64) -> Self::Output {
        Timestamp(self.0 + rhs)
    }
}

impl Sub<Timestamp> for u64 {
    type Output = u64;

    fn sub(self, rhs: Timestamp) -> Self::Output {
        self - rhs.0
    }
}

impl Mul<Resolution> for Timestamp {
    type Output = Timestamp;

    fn mul(self, rhs: Resolution) -> Self::Output {
        Timestamp(self.0 * rhs.0.get())
    }
}

impl Rem<Resolution> for Timestamp {
    type Output = Timestamp;

    fn rem(self, rhs: Resolution) -> Self::Output {
        Timestamp(self.0 % rhs.0.get())
    }
}

/// Errors from converting a `Timestamp` to an index.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    /// Index is too large and cannot be converted to u32.
    #[error("idx too large: {0}")]
    IdxTooLarge(#[from] std::num::TryFromIntError),
}

impl Timestamp {
    /// The largest value that can be represented
    pub const MAX: Timestamp = Timestamp(u64::MAX);

    /// The smallest value that can be represented
    pub const MIN: Timestamp = Timestamp(u64::MIN);

    /// Checks if the timestamp is a multiple of the given resolution.
    #[must_use]
    pub fn is_multiple_of(self, other: Resolution) -> bool {
        self.0.is_multiple_of(other.0.get())
    }

    /// trues to convert the secs into a u32 index
    pub fn as_idx(self) -> Result<u32, IndexError> {
        self.try_into().map_err(IndexError::IdxTooLarge)
    }

    /// Creates a new `Timestamp` from a number of seconds.
    #[must_use]
    pub fn new(s: u64) -> Self {
        Self(s)
    }

    #[allow(clippy::self_named_constructors)] // we want to be explicit
    /// Creates a new `Timestamp` from a number of seconds.
    #[must_use]
    pub fn secs(s: u64) -> Self {
        Self(s)
    }

    /// Creates a new `Timestamp` from a number of minutes.
    #[must_use]
    pub fn mins(m: u64) -> Self {
        Self::secs(m * 60)
    }

    /// Creates a new `Timestamp` from a number of hours.
    #[must_use]
    pub fn hours(h: u64) -> Self {
        Self::mins(h * 60)
    }

    /// Creates a new `Timestamp` from a number of days.
    #[must_use]
    pub fn days(d: u64) -> Self {
        Self::hours(d * 24)
    }

    /// Creates a new `Timestamp` from a number of weeks.
    #[must_use]
    pub fn weeks(w: u64) -> Self {
        Self::days(w * 7)
    }

    /// Returns seconds as u64
    #[must_use]
    pub fn as_secs(&self) -> u64 {
        self.0
    }

    /// Returns the difference between two timestamps as a `Resolution`.
    /// Returns None if the timestamps are equal.
    #[must_use]
    pub fn diff(self, other: Self) -> Option<Resolution> {
        if self.0 < other.0 {
            Some(Resolution((other.0 - self.0).try_into().ok()?))
        } else {
            Some(Resolution((self.0 - other.0).try_into().ok()?))
        }
    }

    /// Aligns the timestamp down to a given resolution.
    #[must_use]
    pub fn align_down(self, resolution: Resolution) -> Self {
        Timestamp((self.0 / resolution.as_u64()) * resolution.as_u64())
    }

    /// Aligns the timestamp up to a given resolution.
    #[must_use]
    pub fn align_up(self, resolution: Resolution) -> Self {
        Timestamp(self.0.div_ceil(resolution.as_u64()) * resolution.as_u64())
    }

    /// Get a range between two timestamps (works like start..end).
    pub fn range(start: Self, end: Self) -> impl Iterator<Item = Self> {
        // TODO(arne): Once iter::step is stabilized, implement that and delete
        //             this function.
        (start.0..end.0).map(Timestamp)
    }
}

impl std::ops::Rem for Timestamp {
    type Output = Self;

    fn rem(self, rhs: Self) -> Self::Output {
        Timestamp(self.0 % rhs.0)
    }
}

impl From<DateTime<Utc>> for Timestamp {
    fn from(d: DateTime<Utc>) -> Self {
        Self(u64::try_from(d.timestamp()).unwrap_or_default())
    }
}
impl PartialEq<u64> for Timestamp {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}s", self.0)
    }
}

impl Add for Timestamp {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Timestamp(self.0 + rhs.0)
    }
}

impl Add<Resolution> for Timestamp {
    type Output = Self;

    fn add(self, rhs: Resolution) -> Self::Output {
        Timestamp(self.0 + rhs.0.get())
    }
}

impl Add<u32> for Timestamp {
    type Output = Self;

    fn add(self, rhs: u32) -> Self::Output {
        Timestamp(self.0 + u64::from(rhs))
    }
}

impl AddAssign<Resolution> for Timestamp {
    fn add_assign(&mut self, rhs: Resolution) {
        self.0 += rhs.0.get();
    }
}

impl TryFrom<Timestamp> for u32 {
    type Error = TryFromIntError;

    fn try_from(value: Timestamp) -> Result<Self, Self::Error> {
        value.0.try_into()
    }
}
impl TryFrom<i64> for Timestamp {
    type Error = TryFromIntError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Ok(Timestamp(value.try_into()?))
    }
}
impl From<u32> for Timestamp {
    fn from(value: u32) -> Self {
        Timestamp(u64::from(value))
    }
}

impl Mul<usize> for Timestamp {
    type Output = Self;

    fn mul(self, rhs: usize) -> Self::Output {
        Timestamp(self.0 * rhs as u64)
    }
}

impl Mul<u32> for Timestamp {
    type Output = Self;

    fn mul(self, rhs: u32) -> Self::Output {
        Timestamp(self.0 * u64::from(rhs))
    }
}

impl Sub for Timestamp {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Timestamp(self.0 - rhs.0)
    }
}
impl Timestamp {
    /// Saturating subtraction of two timestamps.
    #[must_use]
    pub fn saturating_sub(self, rhs: Self) -> Self {
        Timestamp(self.0.saturating_sub(rhs.0))
    }
}

impl Div for Timestamp {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Timestamp(self.0 / rhs.0)
    }
}

impl AddAssign for Timestamp {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl AddAssign<u32> for Timestamp {
    fn add_assign(&mut self, rhs: u32) {
        self.0 += u64::from(rhs);
    }
}

impl From<Duration> for Timestamp {
    fn from(d: Duration) -> Self {
        Self(d.as_secs())
    }
}

/// Returned from methods of `Timerange`.
#[derive(Debug, thiserror::Error)]
pub enum TimerangeError {
    /// Returned from `Timerange::new` if end is before start.
    #[error("end is before start, start: {start}, end: {end}")]
    EndBeforeStart {
        /// The given start.
        start: Timestamp,
        /// The given end.
        end: Timestamp,
    },
}

/// A (half-open) date range  bounded inclusively below and exclusively above.
///
/// We cannot use `std::ops::Range` because the chunking logic is experimental.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Timerange {
    /// The lower bound of the range (inclusive).
    start: Timestamp,
    /// The upper bound of the range (exclusive).
    end: Timestamp,
}

/// divides a time range giving a resolution as a result
/// the minimum resolution is 1 second
impl ops::Div<u128> for Timerange {
    type Output = Resolution;

    fn div(self, rhs: u128) -> Self::Output {
        Resolution::secs(
            // we can never overflow as end-start is a u64 (Timestamp is a u64)
            u64::try_from(u128::from(self.end.0 - self.start.0) / rhs).unwrap_or(u64::MAX),
        )
        .unwrap_or_default()
    }
}

impl Timerange {
    /// Create a new `Timerange`.
    pub fn new(start: Timestamp, end: Timestamp) -> Result<Timerange, TimerangeError> {
        if start > end {
            return Err(TimerangeError::EndBeforeStart { start, end });
        }

        Ok(Self { start, end })
    }

    /// Return the start value.
    #[must_use]
    pub fn start(&self) -> Timestamp {
        self.start
    }

    /// Return the end value.
    #[must_use]
    pub fn end(&self) -> Timestamp {
        self.end
    }

    /// Return the duration of the range.
    #[must_use]
    pub fn duration(&self) -> u64 {
        self.end.0 - self.start.0
    }

    /// Iterate over chunks of at most `chunk_size`.
    pub fn split_by_resolution(&self, resolution: Resolution) -> impl Iterator<Item = Timerange> {
        TimerangeIterator {
            rolling_start: self.start,
            end: self.end,
            step: resolution,
        }
    }

    /// Are the ranges overlapping?
    #[must_use]
    pub fn is_overlapping(&self, other: &Timerange) -> bool {
        // TODO(arne): switch to `std::ops::Range::is_overlapping`,
        // when it's stable
        //
        // the general idea is that there are two conditions where
        // ranges _don't_ overlap:
        //
        // condition a: [ other ] [ self ]
        // true if self.start > other.end
        //
        // condition b: [ self ] [ other]
        // true if other.start > self.end
        //
        // this means it overlaps if:
        // !(self.start > other.end) && !(other.start > self.end)
        //
        // if we inverse this boolean logic, we get the same logic
        // that `std::ops::Range::is_overlapping` does:
        (self.start < other.end) && (other.start < self.end)
    }
}

struct TimerangeIterator {
    rolling_start: Timestamp,
    end: Timestamp,
    step: Resolution,
}

impl Iterator for TimerangeIterator {
    type Item = Timerange;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rolling_start >= self.end {
            return None; // we're done iterating
        }

        let start = self.rolling_start;
        let end = cmp::min(self.rolling_start + self.step.as_u64(), self.end);

        // move rolling start to the next window, but add one second so there's
        // no overlap
        self.rolling_start = Timestamp(self.rolling_start.0 + self.step.as_u64());

        Timerange::new(start, end).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test]
    fn test_timestamp_add() {
        let t1 = Timestamp::from(10);
        let t2 = Timestamp::from(5);
        let result = t1 + t2;
        assert_eq!(result, Timestamp::from(15));
    }

    #[test]
    fn test_timestamp_sub() {
        let t1 = Timestamp::from(10);
        let t2 = Timestamp::from(5);
        let result = t1 - t2;
        assert_eq!(result, Timestamp::from(5));
    }

    #[test]
    fn test_timestamp_mul() {
        let t = Timestamp::from(10);
        let result = t * Resolution::secs(2).expect("2 is zero");
        assert_eq!(result, Timestamp::from(20));
    }

    #[test]
    fn test_timestamp_div() {
        let t1 = Timestamp::from(10);
        let t2 = Timestamp::from(5);
        let result = t1 / t2;
        assert_eq!(result, Timestamp::from(2));
    }

    #[test]
    fn test_timestamp_add_assign() {
        let mut t = Timestamp::from(10);
        t += Timestamp::from(5);
        assert_eq!(t, Timestamp::from(15));
    }

    #[test]
    fn test_timestamp_add_assign_u32() {
        let mut t = Timestamp::from(10);
        t += 5;
        assert_eq!(t, Timestamp::from(15));
    }

    #[test]
    fn test_timestamp_from_duration() {
        let duration = Duration::from_secs(10);
        let timestamp = Timestamp::from(duration);
        assert_eq!(timestamp, Timestamp::from(10));
    }

    #[test]
    fn test_align_down() {
        let t = Timestamp::from(10);
        let result = t.align_down(Resolution::secs(3).expect("3 is zero"));
        assert_eq!(result, Timestamp::from(9));
    }

    #[test]
    fn test_align_up() {
        let t = Timestamp::from(10);
        let result = t.align_up(Resolution::secs(3).expect("3 is zero"));
        assert_eq!(result, Timestamp::from(12));
    }

    #[test_case("already aligned", 6, 3, 6)]
    #[test_case("other > self", 3, 5, 5)]
    #[test_case("align up", 11, 4, 12)]
    #[test_case("million", 1_230_978, 10, 1_230_980)]
    fn align_up_to(
        name: &str,
        res: u64,
        align_up_to: u64,
        expected: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let expected = Resolution::secs(expected)?;
        let res = Resolution::secs(res)?;
        let align_up_to = Resolution::secs(align_up_to)?;

        assert_eq!(expected, res.align_up_to(align_up_to), "{name}");
        Ok(())
    }
}
