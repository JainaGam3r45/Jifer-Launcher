//! Field rules shared by instance manifests and launcher settings.

use serde::{Deserialize, Deserializer, Serialize};

pub const WINDOW_WIDTH: (u32, u32) = (320, 7680);
pub const WINDOW_HEIGHT: (u32, u32) = (240, 4320);
pub const MAX_PATH_LEN: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
}

impl WindowConfig {
    pub fn is_valid(&self) -> bool {
        (WINDOW_WIDTH.0..=WINDOW_WIDTH.1).contains(&self.width)
            && (WINDOW_HEIGHT.0..=WINDOW_HEIGHT.1).contains(&self.height)
    }

    pub fn validated(self) -> Result<Self, String> {
        if self.is_valid() {
            Ok(self)
        } else {
            Err(format!(
                "Window size must be between {}x{} and {}x{}.",
                WINDOW_WIDTH.0, WINDOW_HEIGHT.0, WINDOW_WIDTH.1, WINDOW_HEIGHT.1
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub min: String,
    pub max: String,
}

impl MemoryConfig {
    pub fn new(min: &str, max: &str) -> Self {
        Self {
            min: min.to_owned(),
            max: max.to_owned(),
        }
    }

    pub fn validated(self) -> Result<Self, String> {
        match (to_megabytes(&self.min), to_megabytes(&self.max)) {
            (Some(min), Some(max)) if min <= max => Ok(self),
            _ => Err("Memory values must look like 512M or 2G, and min cannot exceed max.".to_owned()),
        }
    }

    pub fn max_megabytes(&self) -> Option<u64> {
        to_megabytes(&self.max)
    }

    pub fn min_megabytes(&self) -> Option<u64> {
        to_megabytes(&self.min)
    }
}

/// Parses `^([1-9]\d*)([MG])$` into megabytes.
pub fn to_megabytes(value: &str) -> Option<u64> {
    let (digits, unit) = value.split_at(value.len().checked_sub(1)?);
    let factor = match unit {
        "M" => 1,
        "G" => 1024,
        _ => return None,
    };
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u64>().ok()?.checked_mul(factor)
}

/// `^[\w.+-]{min,max}$` with ASCII word characters.
pub fn is_token(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-'))
}

/// Free-form path typed by the user: bounded and free of NUL bytes.
pub fn is_safe_path_text(value: &str) -> bool {
    value.len() <= MAX_PATH_LEN && !value.contains('\0')
}

/// Minecraft offline names: `^[A-Za-z0-9_]{3,16}$`.
pub fn is_offline_username(value: &str) -> bool {
    (3..=16).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Deserializes a field, falling back to its default when the stored value has the wrong shape,
/// so one bad group in a hand-edited file does not reset every other setting.
pub fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_memory_units() {
        assert_eq!(to_megabytes("512M"), Some(512));
        assert_eq!(to_megabytes("2G"), Some(2048));
        for bad in ["", "G", "0G", "02G", "2g", "2 G", "2GB", "-1G", "1.5G"] {
            assert_eq!(to_megabytes(bad), None, "{bad}");
        }
    }

    #[test]
    fn memory_min_cannot_exceed_max() {
        assert!(MemoryConfig::new("1024M", "1G").validated().is_ok());
        assert!(MemoryConfig::new("2G", "1G").validated().is_err());
    }

    #[test]
    fn window_limits_are_inclusive() {
        let window = |width, height| WindowConfig {
            width,
            height,
            fullscreen: false,
        };
        assert!(window(320, 240).is_valid());
        assert!(window(7680, 4320).is_valid());
        assert!(!window(319, 240).is_valid());
        assert!(!window(320, 4321).is_valid());
    }

    #[test]
    fn tokens_and_usernames() {
        assert!(is_token("1.20.1", 0, 64));
        assert!(is_token("", 0, 64));
        assert!(!is_token("", 1, 64));
        assert!(!is_token("1.20/../x", 0, 64));
        assert!(is_offline_username("Steve_01"));
        assert!(!is_offline_username("ab"));
        assert!(!is_offline_username("bad-name"));
    }
}
