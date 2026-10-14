use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Current UTC time in the exact shape of JavaScript's `Date#toISOString` (`2026-11-10T10:00:00.000Z`),
/// so manifests written by Rust and by the previous Electron build compare the same way as strings.
pub fn now_iso() -> String {
    format_iso(OffsetDateTime::now_utc())
}

pub fn format_iso(at: OffsetDateTime) -> String {
    let at = at.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second(),
        at.millisecond()
    )
}

pub fn is_valid_iso(value: &str) -> bool {
    OffsetDateTime::parse(value, &Rfc3339).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn matches_javascript_iso_format() {
        assert_eq!(
            format_iso(datetime!(2026-11-10 09:05:03.042 UTC)),
            "2026-11-10T09:05:03.042Z"
        );
        assert_eq!(
            format_iso(datetime!(2026-11-10 10:00:00 +02:00)),
            "2026-11-10T08:00:00.000Z"
        );
    }

    #[test]
    fn validates_rfc3339() {
        assert!(is_valid_iso(&now_iso()));
        assert!(is_valid_iso("2026-01-02T03:04:05.000Z"));
        assert!(!is_valid_iso("yesterday"));
        assert!(!is_valid_iso(""));
    }
}
