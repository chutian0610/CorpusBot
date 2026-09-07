use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::{Date, OffsetDateTime};

pub type DateTimeUtc = OffsetDateTime;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IsoDate(Date);

impl IsoDate {
    pub fn parse(value: impl AsRef<str>) -> crate::Result<Self> {
        let date = Date::parse(
            value.as_ref(),
            &time::format_description::well_known::Iso8601::DEFAULT,
        )
        .map_err(|error| crate::CoreError::Frontmatter(error.to_string()))?;
        Ok(Self(date))
    }

    pub fn date(self) -> Date {
        self.0
    }
}

impl std::fmt::Display for IsoDate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl Serialize for IsoDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for IsoDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

pub mod datetime_utc {
    use super::*;

    pub fn serialize<S>(value: &DateTimeUtc, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let formatted = value
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&formatted)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTimeUtc, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        OffsetDateTime::parse(&value, &time::format_description::well_known::Rfc3339)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_dates_only() {
        assert!(IsoDate::parse("2026-09-07").is_ok());
        assert!(IsoDate::parse("2026-9-7").is_err());
    }
}
