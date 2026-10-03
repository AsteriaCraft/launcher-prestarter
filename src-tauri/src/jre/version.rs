//! Java version strings as Liberica writes them: `25+37`, `25.0.4+9` (PSU), `25.0.4.1+1` (CSPU).
//! Ordered by feature, interim, update, patch, then build: `25.0.4.1+1` is newer than `25.0.4+9`.

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JavaVersion {
    pub numbers: [u32; 4],
    pub build: u32,
}

impl JavaVersion {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (numbers_text, build_text) = match text.split_once('+') {
            Some((n, b)) => (n, Some(b)),
            None => (text, None),
        };
        let mut numbers = [0u32; 4];
        for (index, part) in numbers_text.split('.').enumerate() {
            if index == 4 || part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            numbers[index] = part.parse().ok()?;
        }
        let build = match build_text {
            None => 0,
            Some(b) if !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()) => b.parse().ok()?,
            Some(_) => return None,
        };
        (numbers[0] > 0).then_some(Self { numbers, build })
    }

    pub fn feature(&self) -> u32 {
        self.numbers[0]
    }
}

impl PartialOrd for JavaVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JavaVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numbers.cmp(&other.numbers).then(self.build.cmp(&other.build))
    }
}

impl fmt::Display for JavaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let len = self.numbers.iter().rposition(|n| *n != 0).map_or(1, |i| i + 1).max(1);
        let parts: Vec<String> = self.numbers[..len].iter().map(u32::to_string).collect();
        write!(f, "{}+{}", parts.join("."), self.build)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> JavaVersion {
        JavaVersion::parse(text).unwrap()
    }

    #[test]
    fn parses_liberica_versions() {
        assert_eq!(v("25+37"), JavaVersion { numbers: [25, 0, 0, 0], build: 37 });
        assert_eq!(v("25.0.4+9"), JavaVersion { numbers: [25, 0, 4, 0], build: 9 });
        assert_eq!(v("25.0.4.1+1"), JavaVersion { numbers: [25, 0, 4, 1], build: 1 });
        assert_eq!(v("21.0.2"), JavaVersion { numbers: [21, 0, 2, 0], build: 0 });
    }

    #[test]
    fn refuses_garbage() {
        for bad in ["", "+1", "25.", "25..1", "x25", "25+", "25+b1", "1.2.3.4.5", "0.1"] {
            assert!(JavaVersion::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn orders_cspu_above_psu() {
        assert!(v("25.0.4.1+1") > v("25.0.4+9"));
        assert!(v("25.0.4+9") > v("25+37"));
        assert!(v("25.0.5+1") > v("25.0.4.1+1"));
        assert!(v("26+1") > v("25.0.9+99"));
        assert_eq!(v("25.0.4+9").feature(), 25);
    }

    #[test]
    fn displays_round_trip() {
        for text in ["25+37", "25.0.4+9", "25.0.4.1+1"] {
            assert_eq!(v(text).to_string(), text);
        }
    }
}
