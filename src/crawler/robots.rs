//! Minimal robots.txt respect (fetch + allow/deny path check).
//! Reliability-first: on parse/network failure we allow the URL
//! (fail-open for availability) but still honour explicit Disallow when parsed.

use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

static CACHE: Lazy<Mutex<HashMap<String, RobotsRules>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Default)]
pub struct RobotsRules {
    pub disallows: Vec<String>,
    pub fetched: bool,
}

impl RobotsRules {
    pub fn allows(&self, path: &str) -> bool {
        if !self.fetched {
            return true; // fail-open
        }
        for d in &self.disallows {
            if d == "/" {
                return false;
            }
            if path.starts_with(d) {
                return false;
            }
        }
        true
    }
}

/// Parse a robots.txt body for User-agent: * rules only.
pub fn parse_robots(body: &str) -> RobotsRules {
    let mut rules = RobotsRules {
        fetched: true,
        ..Default::default()
    };
    let mut in_star = false;
    for line in body.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_lowercase();
        if lower.starts_with("user-agent:") {
            let ua = line.splitn(2, ':').nth(1).unwrap_or("").trim();
            in_star = ua == "*";
            continue;
        }
        if !in_star {
            continue;
        }
        if lower.starts_with("disallow:") {
            let path = line.splitn(2, ':').nth(1).unwrap_or("").trim();
            if !path.is_empty() {
                rules.disallows.push(path.to_string());
            }
        }
    }
    rules
}

pub async fn check_allowed(client: &reqwest::Client, url: &str) -> bool {
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let host = match parsed.host_str() {
        Some(h) => h.to_lowercase(),
        None => return false,
    };
    let path = parsed.path().to_string();

    {
        let cache = CACHE.lock().unwrap();
        if let Some(r) = cache.get(&host) {
            return r.allows(&path);
        }
    }

    let robots_url = format!("{}://{}/robots.txt", parsed.scheme(), host);
    let rules = match client.get(&robots_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            parse_robots(&body)
        }
        _ => RobotsRules::default(), // fail-open
    };

    let allowed = rules.allows(&path);
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(host, rules);
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_disallow() {
        let body = r#"
User-agent: *
Disallow: /admin
Disallow: /private/
"#;
        let r = parse_robots(body);
        assert!(!r.allows("/admin"));
        assert!(!r.allows("/private/x"));
        assert!(r.allows("/public"));
    }
}
