use eyre::{eyre, Result};
use regex::Regex;
use semver::{BuildMetadata, Prerelease, Version};
use serde::Serialize;

/// Represents a part of a semver version (e.g. major, minor)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Major,
    Minor,
    Patch,
    Prerelease(Option<String>),
    None,
}

/// Holds replaceable values like {current_version}
pub struct Placeholders<'a> {
    pub current_version: &'a Version,
    pub new_version: &'a Version,
}

impl<'a> Placeholders<'a> {
    pub fn replace(&self, s: &str) -> String {
        let re_current_version = Regex::new("\\{current_version\\}").unwrap();
        let re_new_version = Regex::new("\\{new_version\\}").unwrap();

        let result = re_current_version.replace(s, self.current_version.to_string());
        let result = re_new_version.replace(&result, self.new_version.to_string());

        result.to_string()
    }
}

/// Bump a part of a version.
///
/// This function increments a part of a semver version based on the `Part` it is given.
/// The `build` arguments allows for additional build information to be added to the incremented version.
pub fn bump(v: &Version, part: &Part, build: Option<String>) -> Result<Version> {
    let mut new_version = match part {
        Part::Major => Ok::<Version, semver::Error>(Version::new(v.major + 1, 0, 0)),
        Part::Minor => Ok(Version::new(v.major, v.minor + 1, 0)),
        Part::Patch => Ok(Version::new(v.major, v.minor, v.patch + 1)),
        Part::Prerelease(None) => {
            let mut new_version = v.clone();
            if v.pre.is_empty() {
                return Err(eyre!(
                    "Can't remove a 'prerelease' (x.x.x-<prerelease>) from version '{}', missing prerelease", v.to_string()
                ));
            }
            new_version.pre = Prerelease::EMPTY;
            return Ok(new_version);
        }
        Part::Prerelease(Some(label)) => {
            let mut new_version = v.clone();
            let pre = match parse_prerelease(v.pre.as_str()) {
                // old version has same prerelease, but no version
                (Some(existing), None) if &existing == label => {
                    Prerelease::new(&make_prerelease(label, 1))?
                }
                // old version has same prerelease and version
                (Some(existing), Some(v)) if &existing == label => {
                    Prerelease::new(&make_prerelease(label, v + 1))?
                }
                // old version has different version
                (Some(existing), _) if &existing != label => Prerelease::new(label)?,
                // unknown, return empty
                _ => Prerelease::EMPTY,
            };
            new_version.pre = pre;
            Ok(new_version)
        }
        // TODO: Fix error return type
        Part::None => panic!("impossible"),
    }?;

    // Add build metadata
    new_version.build = build.map_or(BuildMetadata::EMPTY, |x| BuildMetadata::new(&x).unwrap());

    Ok(new_version)
}

fn parse_prerelease(s: &str) -> (Option<String>, Option<u64>) {
    let splits: Vec<&str> = s.split('.').collect();
    if let Some(label) = splits.first() {
        if let Some(version_string) = splits.get(1) {
            let version: u64 = version_string.parse().unwrap();
            return (Some(label.to_string()), Some(version));
        }
        return (Some(label.to_string()), None);
    }
    (None, None)
}

fn make_prerelease(label: &str, version: u64) -> String {
    format!("{label}.{version}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cases() {
        let cases: Vec<(&str, Part, Option<String>, &str)> = vec![
            // Add prerelease
            (
                "1.0.0",
                Part::Prerelease(Some("beta".to_string())),
                None,
                "1.0.0-beta",
            ),
            // Increment prerelease
            (
                "1.0.0-beta",
                Part::Prerelease(Some("beta".to_string())),
                None,
                "1.0.0-beta.1",
            ),
            // Change prerelease
            (
                "1.0.0-beta",
                Part::Prerelease(Some("test".to_string())),
                None,
                "1.0.0-test",
            ),
            // Inc. patch
            ("1.1.1", Part::Patch, None, "1.1.2"),
            // Inc. minor
            ("1.1.1", Part::Minor, None, "1.2.0"),
            // Inc. major
            ("1.1.1", Part::Major, None, "2.0.0"),
            // Keep build meta
            (
                "1.1.1+zlib-1.0.0",
                Part::Major,
                Some("zlib-1.0.0".to_string()),
                "2.0.0+zlib-1.0.0",
            ),
            // Remove build meta
            ("1.1.1+zlib-1.0.0", Part::Major, None, "2.0.0"),
            // Build meta and patch
            (
                "1.8.3-nightly.23+extra",
                Part::Patch,
                Some("extra".to_string()),
                "1.8.4+extra",
            ),
            // Build meta and prerelease
            (
                "1.8.3-nightly.23+extra",
                Part::Prerelease(Some("nightly".to_string())),
                Some("extra".to_string()),
                "1.8.3-nightly.24+extra",
            ),
            // Build meta + release
            (
                "1.8.3-nightly.23+extra",
                Part::Prerelease(None),
                Some("extra".to_string()),
                "1.8.3+extra",
            ),
            // release
            ("1.3.3-nightly.999", Part::Prerelease(None), None, "1.3.3"),
        ];

        for (before, part, build, expect) in cases {
            let version = Version::parse(before).unwrap();
            let expected_version = Version::parse(expect).unwrap();
            assert_eq!(bump(&version, &part, build).unwrap(), expected_version)
        }
    }

    #[test]
    fn test_release_non_prerelease() {
        let cases: Vec<(&str, Part, Option<String>, &str)> =
            vec![("1.0.0", Part::Prerelease(None), None, "1.0.0")];
        for (before, part, build, _expect) in cases {
            let version = Version::parse(before).unwrap();
            assert!(bump(&version, &part, build).is_err())
        }
    }

    #[test]
    fn test_replace() {
        let placeholders = Placeholders {
            current_version: &Version::parse("1.0.0-alpha.1+something").unwrap(),
            new_version: &Version::parse("2.0.0").unwrap(),
        };

        let cases = [
            ("{current_version}", "1.0.0-alpha.1+something"),
            ("{new_version}", "2.0.0"),
        ];
        for (input, expect) in cases {
            assert_eq!(placeholders.replace(input), expect)
        }
    }
}
