//! URL normalization, deduplication and cache helpers.

use crate::error::Result;
use url::Url;

/// Normalize a URL for deduplication.
///
/// Rules:
/// - lowercase scheme and host
/// - remove default ports
/// - remove trailing slash on path (except root)
/// - sort query parameters
/// - remove common tracking parameters
/// - remove fragments
pub fn normalize_url(raw: &str) -> Result<String> {
    let mut u = Url::parse(raw)?;

    // Force lowercase host
    if let Some(host) = u.host_str() {
        let lower = host.to_lowercase();
        let _ = u.set_host(Some(&lower));
    }

    // Remove fragment
    u.set_fragment(None);

    // Remove default ports
    if u.port() == Some(80) && u.scheme() == "http" {
        let _ = u.set_port(None);
    }
    if u.port() == Some(443) && u.scheme() == "https" {
        let _ = u.set_port(None);
    }

    // Clean path: collapse multiple slashes, remove trailing slash (except root)
    let path = u.path().to_string();
    let cleaned_path = clean_path(&path);
    u.set_path(&cleaned_path);

    // Filter and sort query parameters
    let tracking_params = [
        "utm_source",
        "utm_medium",
        "utm_campaign",
        "utm_term",
        "utm_content",
        "fbclid",
        "gclid",
        "mc_cid",
        "mc_eid",
        "_ga",
        "ref",
        "ref_",
        "source",
    ];

    let pairs: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| {
            let key = k.to_lowercase();
            !tracking_params.iter().any(|t| key == *t)
        })
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if pairs.is_empty() {
        u.set_query(None);
    } else {
        let mut sorted = pairs;
        sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let query = sorted
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding_encode(k), urlencoding_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        u.set_query(Some(&query));
    }

    Ok(u.to_string())
}

fn clean_path(path: &str) -> String {
    let parts: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

/// Minimal percent-encoding for query values (keeps it simple and deterministic).
fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

/// Extract hostname from a URL string.
pub fn hostname_of(raw: &str) -> Option<String> {
    Url::parse(raw)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
}

/// Extract path from a URL string.
pub fn path_of(raw: &str) -> Option<String> {
    Url::parse(raw).ok().map(|u| u.path().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_basic() {
        let n = normalize_url("HTTPS://Example.COM/Path/?b=2&a=1#frag").unwrap();
        assert_eq!(n, "https://example.com/Path?a=1&b=2");
    }

    #[test]
    fn removes_tracking() {
        let n = normalize_url("https://example.com/page?utm_source=foo&id=1").unwrap();
        assert_eq!(n, "https://example.com/page?id=1");
    }

    #[test]
    fn trailing_slash() {
        let n = normalize_url("https://example.com/foo/").unwrap();
        assert_eq!(n, "https://example.com/foo");
    }

    #[test]
    fn root_path() {
        let n = normalize_url("https://example.com/").unwrap();
        assert_eq!(n, "https://example.com/");
    }
}
