/// A GitHub timestamp (`…Z`, `….000Z`, `…+02:00`) as UTC `YYYY-MM-DDTHH:MM:SSZ`;
/// `None` when the value is not a full timestamp.
pub fn utc_timestamp(value: &str) -> Option<String> {
    crate::civil_date::parse_rfc3339(value)
        .map(|(secs, _)| crate::civil_date::iso8601_secs(secs.max(0)))
}

#[cfg(test)]
mod tests;
