use crate::{curl_bytes, Result};
use serde::{Deserialize, Serialize};

const RELEASES_URL: &str = "https://www.php.net/releases/index.php?json&version=8&max=100";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) version: String,
    pub(crate) filename: String,
    pub(crate) url: String,
    pub(crate) sha256: String,
}

#[derive(Deserialize)]
struct Release {
    version: String,
    source: Option<Vec<ReleaseSource>>,
}

#[derive(Deserialize)]
struct ReleaseSource {
    filename: String,
    sha256: String,
}

pub(crate) fn valid_minor(minor: &str) -> bool {
    matches!(minor, "8.1" | "8.2" | "8.3" | "8.4" | "8.5" | "8.6")
}

fn version_tuple(version: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
        })
    {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts[2].parse().ok()?;
    Some((major, minor, patch))
}

fn resolve_releases(minor: &str, payload: &serde_json::Value) -> Result<Source> {
    if !valid_minor(minor) {
        return Err(format!("Unsupported PHP minor: {minor}").into());
    }
    let releases: Vec<Release> = match payload {
        serde_json::Value::Object(map) => map
            .iter()
            .filter_map(|(version, item)| {
                let mut value = item.clone();
                value
                    .as_object_mut()?
                    .insert("version".to_owned(), version.clone().into());
                serde_json::from_value(value).ok()
            })
            .collect(),
        serde_json::Value::Array(_) => serde_json::from_value(payload.clone())?,
        _ => return Err("Invalid php.net releases JSON: expected object or list".into()),
    };
    let mut candidates = Vec::new();
    for release in releases {
        let Some((major, minor_version, _)) = version_tuple(&release.version) else {
            continue;
        };
        if format!("{major}.{minor_version}") != minor {
            continue;
        }
        for source in release.source.unwrap_or_default() {
            if source.filename != format!("php-{}.tar.xz", release.version)
                || source.sha256.len() != 64
                || !source.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                continue;
            }
            candidates.push((
                version_tuple(&release.version).unwrap(),
                Source {
                    version: release.version.clone(),
                    filename: source.filename,
                    url: format!(
                        "https://www.php.net/distributions/php-{}.tar.xz",
                        release.version
                    ),
                    sha256: source.sha256.to_ascii_lowercase(),
                },
            ));
        }
    }
    candidates.sort_by_key(|candidate| candidate.0);
    candidates
        .pop()
        .map(|candidate| candidate.1)
        .ok_or_else(|| format!("No validated php.net tar.xz release for PHP {minor}").into())
}

pub(crate) fn resolve(minor: &str) -> Result<Source> {
    let payload: serde_json::Value = serde_json::from_slice(&curl_bytes(RELEASES_URL)?)?;
    resolve_releases(minor, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_selects_highest_valid_numeric_patch_and_rejects_bad_metadata() {
        let payload = serde_json::json!({
            "8.5.9": {"source":[{"filename":"php-8.5.9.tar.xz","sha256":"a".repeat(64)}]},
            "8.5.12": {"source":[{"filename":"php-8.5.12.tar.xz","sha256":"b".repeat(64)}]},
            "8.5.99RC1": {"source":[{"filename":"php-8.5.99RC1.tar.xz","sha256":"c".repeat(64)}]},
            "8.5.13": {"source":[{"filename":"php-8.5.12.tar.xz","sha256":"d".repeat(64)}]},
            "8.6.1": {"source":[{"filename":"php-8.6.1.tar.xz","sha256":"e".repeat(64)}]}
        });
        assert_eq!(resolve_releases("8.5", &payload).unwrap().version, "8.5.12");
        assert!(resolve_releases("8.7", &payload).is_err());
        assert_eq!(resolve_releases("8.6", &payload).unwrap().version, "8.6.1");
        let invalid = serde_json::json!({
            "8.6.2": {"source":[{"filename":"php-8.6.2.tar.xz","sha256":"not-a-hash"}]}
        });
        assert!(resolve_releases("8.6", &invalid).is_err());
    }
}
