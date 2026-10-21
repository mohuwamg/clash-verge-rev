use anyhow::{Result, bail, ensure};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};
use std::{collections::BTreeSet, net::Ipv4Addr, time::Duration};

pub const GROUP: &str = "ZK Auto Failover";
pub const INTERVAL_SECONDS: u64 = 60;
pub const RETRY_INTERVAL_SECONDS: u64 = 30;

pub fn next_check_delay(phase: &str, message: Option<&str>, elapsed: Duration) -> Duration {
    if phase == "retry_wait" && message == Some("pair_unavailable") {
        Duration::from_secs(RETRY_INTERVAL_SECONDS)
    } else if phase == "retry_wait" {
        Duration::from_secs(INTERVAL_SECONDS)
    } else {
        Duration::from_secs(INTERVAL_SECONDS)
            .saturating_sub(elapsed)
            .max(Duration::from_secs(1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct AutoFailoverConfig {
    pub enabled: bool,
    pub subscription_url: String,
    pub profile_uid: Option<String>,
    pub websites: Vec<String>,
    pub delay_urls: Vec<String>,
    pub delay_timeout_ms: u32,
    pub website_timeout_ms: u32,
    pub concurrency: usize,
}

impl Default for AutoFailoverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            subscription_url: String::new(),
            profile_uid: None,
            websites: vec!["https://api.openai.com/v1/models".into()],
            delay_urls: vec!["https://api.openai.com/v1/models".into()],
            delay_timeout_ms: 5_000,
            website_timeout_ms: 10_000,
            concurrency: 5,
        }
    }
}

pub fn valid_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw)?;
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "Expected an HTTP(S) URL"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URL credentials are not supported"
    );
    ensure!(url.fragment().is_none(), "URL fragments are not supported");
    Ok(url)
}

impl AutoFailoverConfig {
    pub fn validate(&self) -> Result<()> {
        if self.subscription_url.trim().is_empty() {
            ensure!(!self.enabled, "Subscription URL is required when enabled");
        } else {
            valid_url(&self.subscription_url)?;
        }
        ensure!(
            (1..=20).contains(&self.websites.len()),
            "Configure between 1 and 20 websites"
        );
        ensure!(
            (1..=3).contains(&self.delay_urls.len()),
            "Configure between 1 and 3 delay URLs"
        );
        for url in self.websites.iter().chain(&self.delay_urls) {
            let parsed = valid_url(url)?;
            ensure!(parsed.domain().is_some(), "Probe URLs must use a domain name");
            ensure!(
                !parsed.host_str().unwrap_or_default().contains(','),
                "Invalid probe domain"
            );
        }
        ensure!(
            (500..=30_000).contains(&self.delay_timeout_ms),
            "Delay timeout must be 500–30000 ms"
        );
        ensure!(
            (1_000..=60_000).contains(&self.website_timeout_ms),
            "Website timeout must be 1000–60000 ms"
        );
        ensure!((1..=20).contains(&self.concurrency), "Concurrency must be 1–20");
        Ok(())
    }

    pub fn domains(&self) -> Vec<String> {
        let mut domains = BTreeSet::new();
        for raw in &self.websites {
            if let Ok(url) = Url::parse(raw)
                && let Some(host) = url.domain()
            {
                domains.insert(host.to_owned());
                if domain_matches(host, "github.com") {
                    domains.extend(["github.com", "githubassets.com", "githubusercontent.com"].map(str::to_owned));
                }
                if domain_matches(host, "chatgpt.com") || domain_matches(host, "openai.com") {
                    domains.extend(
                        [
                            "chatgpt.com",
                            "openai.com",
                            "oaistatic.com",
                            "oaiusercontent.com",
                            "auth0.com",
                        ]
                        .map(str::to_owned),
                    );
                }
            }
        }
        domains.into_iter().collect()
    }
}

pub fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain || host.strip_suffix(domain).is_some_and(|prefix| prefix.ends_with('.'))
}

pub fn redirect_allowed(url: &Url, domains: &[String]) -> bool {
    url.scheme() == "https"
        && url
            .domain()
            .is_some_and(|host| domains.iter().any(|domain| domain_matches(host, domain)))
}

// The subscription labels the exit IP and route, e.g. "US 198.29.100.246 · 主线".
// Do not infer an exit IP from the proxy server's address or partially match IPs.
fn labeled_ip(name: &str) -> Option<Ipv4Addr> {
    let mut addresses = name
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .filter_map(|part| part.parse::<Ipv4Addr>().ok());
    let ip = addresses.next()?;
    // Ambiguous labels must never authorize an automatic switch.
    addresses.next().is_none().then_some(ip)
}

fn route_identity(name: &str) -> Option<(Ipv4Addr, bool)> {
    let name = name.trim();
    let backup = if name.ends_with("主线") {
        false
    } else if name.ends_with("备用") {
        true
    } else {
        return None;
    };
    labeled_ip(name).map(|ip| (ip, backup))
}

/// Only the opposite main/backup route with the exact same labeled IP is eligible.
/// Unknown or ambiguous current labels never authorize a cross-IP fallback.
pub fn paired_candidates(nodes: &[String], current: Option<&str>) -> Vec<String> {
    let Some((ip, backup)) = current.and_then(route_identity) else {
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    nodes
        .iter()
        .filter(|name| route_identity(name) == Some((ip, !backup)) && seen.insert(name.as_str()))
        .cloned()
        .collect()
}

/// Applied after scripts and merges, only to the bound subscription. Raw subscription stays intact.
pub fn enhance(config: &mut Mapping, settings: &AutoFailoverConfig) -> Result<()> {
    settings.validate()?;
    let groups = config
        .entry(Value::from("proxy-groups"))
        .or_insert_with(|| Value::Sequence(Vec::new()));
    let groups = groups
        .as_sequence_mut()
        .ok_or_else(|| anyhow::anyhow!("Invalid proxy-groups"))?;
    ensure!(
        !groups
            .iter()
            .any(|group| group.get("name").and_then(Value::as_str) == Some(GROUP)),
        "Reserved proxy group name already exists: {GROUP}"
    );
    let mut group = Mapping::new();
    group.insert("name".into(), GROUP.into());
    group.insert("type".into(), "select".into());
    let nodes: Vec<Value> = config
        .get("proxies")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter(|node| {
            !matches!(
                node.get("type").and_then(Value::as_str),
                Some("direct" | "reject" | "dns" | "pass")
            )
        })
        .filter_map(|node| node.get("name").cloned())
        .collect();
    if !nodes.is_empty() {
        group.insert("proxies".into(), Value::Sequence(nodes));
    }
    let providers: Vec<Value> = config
        .get("proxy-providers")
        .and_then(Value::as_mapping)
        .map(|providers| providers.keys().cloned().collect())
        .unwrap_or_default();
    if !providers.is_empty() {
        group.insert("use".into(), Value::Sequence(providers));
    }
    // An empty subscription must still load so the user can update it.
    if !group.contains_key("proxies") && !group.contains_key("use") {
        group.insert("proxies".into(), Value::Sequence(vec!["REJECT".into()]));
    }
    if let Some(groups) = config.get_mut("proxy-groups").and_then(Value::as_sequence_mut) {
        groups.insert(0, Value::Mapping(group));
    }
    let mut rules: Vec<Value> = settings
        .domains()
        .iter()
        .map(|domain| Value::from(format!("DOMAIN-SUFFIX,{domain},{GROUP}")))
        .collect();
    if let Some(existing) = config.get("rules").and_then(Value::as_sequence) {
        rules.extend(existing.iter().cloned());
    }
    config.insert("rules".into(), Value::Sequence(rules));
    Ok(())
}

pub fn classify_response(url: &Url, status: u16, body: &str) -> Result<()> {
    // This unauthenticated endpoint is also useful as a connectivity probe. Its
    // expected authentication error proves an HTTPS response, not model access.
    // Keep this exception scoped to the API endpoint and a structured error;
    // an arbitrary 401 page (or proxy login page) must not pass website checks.
    if url.scheme() == "https"
        && url.host_str() == Some("api.openai.com")
        && url.path().trim_end_matches('/') == "/v1/models"
        && status == 401
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(body)
        && let Some(error) = json.get("error")
        && matches!(
            error.get("type").and_then(serde_json::Value::as_str),
            Some("invalid_request_error" | "authentication_error")
        )
        && error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|message| !message.trim().is_empty())
    {
        return Ok(());
    }
    let text = body.to_lowercase();
    if matches!(status, 200..=299 | 403) {
        // Explicit blocks take precedence over incidental verification text.
        for marker in [
            "unsupported_country",
            "not available in your country",
            "request blocked",
            "<title>access denied",
        ] {
            ensure!(!text.contains(marker), "blocked_page: {marker}");
        }
        if ["<title>just a moment", "cf-chl-widget", "verify you are human"]
            .iter()
            .any(|marker| text.contains(marker))
        {
            bail!("verification_required");
        }
    }
    match status {
        200..=299 => {}
        403 => bail!("access_denied"),
        429 => bail!("rate_limited"),
        500..=599 => bail!("server_error: HTTP {status}"),
        _ => bail!("unexpected_status: HTTP {status}"),
    }
    ensure!(
        !text.contains("<title>attention required"),
        "blocked_page: attention required"
    );
    let host = url.domain().unwrap_or_default();
    if domain_matches(host, "github.com") {
        ensure!(
            text.contains("github") && (text.contains("<html") || text.contains("<!doctype")),
            "unexpected_github_page"
        );
    } else if domain_matches(host, "chatgpt.com") || domain_matches(host, "openai.com") {
        ensure!(
            (text.contains("chatgpt") || text.contains("openai"))
                && (text.contains("<html") || text.contains("<!doctype")),
            "unexpected_chatgpt_page"
        );
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const MISSING_AUTH: &str = r#"{"error":{"message":"Missing bearer authentication in header","type":"invalid_request_error","param":null,"code":null}}"#;

    #[test]
    fn unauthenticated_models_response_proves_network_connectivity() {
        let url = Url::parse("https://api.openai.com/v1/models").unwrap();
        assert!(classify_response(&url, 401, MISSING_AUTH).is_ok());
        assert!(
            classify_response(
                &url,
                401,
                r#"{"error":{"message":"You didn't provide an API key.","type":"invalid_request_error"}}"#
            )
            .is_ok()
        );
        for status in [403, 407, 429, 500, 502, 503] {
            assert!(classify_response(&url, status, MISSING_AUTH).is_err());
        }
    }

    #[test]
    fn models_probe_does_not_accept_unrelated_auth_pages() {
        let url = Url::parse("https://api.openai.com/v1/models").unwrap();
        for body in [
            "<html>Proxy login required</html>",
            "Missing bearer authentication in header",
            r#"{"error":{"message":"Missing bearer authentication in header"}}"#,
            r#"{"error":{"message":"","type":"invalid_request_error"}}"#,
            r#"{"error":{"message":"Region blocked","type":"unsupported_country"}}"#,
        ] {
            assert!(classify_response(&url, 401, body).is_err());
        }
        for raw in [
            "http://api.openai.com/v1/models",
            "https://api.openai.com.evil.test/v1/models",
            "https://chatgpt.com/",
            "https://github.com/",
            "https://api.openai.com/v1/responses",
        ] {
            assert!(classify_response(&Url::parse(raw).unwrap(), 401, MISSING_AUTH).is_err());
        }
    }

    #[test]
    fn verification_is_distinct_from_access_denial_or_rate_limiting() {
        let url = Url::parse("https://chatgpt.com/").unwrap();
        let challenge = "<html><title>Just a moment...</title></html>";
        for status in [200, 403] {
            assert_eq!(
                classify_response(&url, status, challenge).unwrap_err().to_string(),
                "verification_required"
            );
        }
        assert_eq!(
            classify_response(&url, 403, "Forbidden").unwrap_err().to_string(),
            "access_denied"
        );
        assert_eq!(
            classify_response(&url, 429, challenge).unwrap_err().to_string(),
            "rate_limited"
        );
        assert!(
            classify_response(
                &url,
                200,
                "<html>ChatGPT not available in your country; verify you are human</html>"
            )
            .unwrap_err()
            .to_string()
            .starts_with("blocked_page")
        );
    }

    #[test]
    fn only_same_ip_opposite_route_in_both_directions_for_every_ip() {
        for ip in ["198.29.100.246", "66.167.8.36", "66.167.11.61"] {
            let primary = format!("US {ip} · 主线");
            let backup = format!("US {ip} · 备用");
            let nodes = vec![
                "US 1.2.3.4 · 备用".into(),
                primary.clone(),
                backup.clone(),
                backup.clone(),
                format!("duplicate route {ip} · 主线"),
            ];
            assert_eq!(paired_candidates(&nodes, Some(&primary)), [backup.clone()]);
            assert_eq!(
                paired_candidates(&[primary.clone(), backup.clone()], Some(&backup)),
                [primary]
            );
        }
    }

    #[test]
    fn missing_unknown_or_ambiguous_identity_never_selects_another_ip() {
        let nodes = ["US 66.167.8.36 · 备用".into()];
        for current in [
            None,
            Some("unknown"),
            Some("US 66.167.8.36"),
            Some("US 999.167.8.36 · 主线"),
            Some("1.2.3.4 66.167.8.36 · 主线"),
            Some("US 66.167.8.3 · 主线"),
        ] {
            assert!(paired_candidates(&nodes, current).is_empty());
        }
        assert!(paired_candidates(&[], Some("US 66.167.8.36 · 主线")).is_empty());
        assert_eq!(
            route_identity("US 66.167.8.36 · 备用  "),
            Some((Ipv4Addr::new(66, 167, 8, 36), true))
        );
    }

    #[test]
    fn only_pair_failure_accelerates_retry_and_recovery_returns_to_one_minute() {
        let elapsed = Duration::from_secs(7);
        assert_eq!(next_check_delay("healthy", None, elapsed).as_secs(), 53);
        assert_eq!(
            next_check_delay("retry_wait", Some("pair_unavailable"), elapsed).as_secs(),
            30
        );
        assert_eq!(next_check_delay("healthy", None, elapsed).as_secs(), 53);
        assert_eq!(
            next_check_delay("verification_required", Some("verification_required"), elapsed).as_secs(),
            53
        );
        assert_eq!(
            next_check_delay("retry_wait", Some("core_error"), elapsed).as_secs(),
            60
        );
        assert_eq!(next_check_delay("healthy", None, Duration::from_secs(90)).as_secs(), 1);
    }

    #[test]
    fn routes_override_direct_rules_and_automatic_groups() {
        let mut yaml: Mapping = serde_yaml_ng::from_str("proxies:\n - {name: one, type: ss}\nproxy-groups:\n - {name: automatic, type: fallback, proxies: [one]}\nrules: ['MATCH,DIRECT']\n").unwrap();
        enhance(&mut yaml, &AutoFailoverConfig::default()).unwrap();
        let group = &yaml["proxy-groups"][0];
        assert_eq!(group["name"].as_str(), Some(GROUP));
        assert_eq!(group["proxies"][0].as_str(), Some("one"));
        assert!(yaml["rules"][0].as_str().unwrap().ends_with(GROUP));
        assert_eq!(yaml["proxy-groups"][1]["name"].as_str(), Some("automatic"));
        assert!(enhance(&mut yaml, &AutoFailoverConfig::default()).is_err());
    }

    #[test]
    fn providers_and_empty_subscriptions_are_supported() {
        let mut yaml: Mapping = serde_yaml_ng::from_str("proxy-providers: {remote: {type: http}}\n").unwrap();
        enhance(&mut yaml, &AutoFailoverConfig::default()).unwrap();
        assert_eq!(yaml["proxy-groups"][0]["use"][0].as_str(), Some("remote"));
        let mut empty = Mapping::new();
        enhance(&mut empty, &AutoFailoverConfig::default()).unwrap();
        assert_eq!(empty["proxy-groups"][0]["proxies"][0].as_str(), Some("REJECT"));
    }

    #[test]
    fn challenge_and_rate_limit_are_not_healthy() {
        let github = Url::parse("https://github.com/").unwrap();
        let chatgpt = Url::parse("https://chatgpt.com/").unwrap();
        assert!(classify_response(&github, 200, "<html>GitHub sign in</html>").is_ok());
        assert!(classify_response(&chatgpt, 200, "<html>ChatGPT log in</html>").is_ok());
        assert!(classify_response(&chatgpt, 200, "<html><title>Just a moment...</title>ChatGPT</html>").is_err());
        assert!(classify_response(&github, 429, "GitHub").is_err());
        assert!(classify_response(&github, 200, "<html>captive portal</html>").is_err());
    }

    #[test]
    fn redirects_must_stay_on_managed_https_domains() {
        let domains = AutoFailoverConfig::default().domains();
        assert!(redirect_allowed(
            &Url::parse("https://auth.openai.com/login").unwrap(),
            &domains
        ));
        assert!(!redirect_allowed(
            &Url::parse("https://evilgithub.com/").unwrap(),
            &domains
        ));
        assert!(!redirect_allowed(
            &Url::parse("https://github.com.evil.test/").unwrap(),
            &domains
        ));
        assert!(!redirect_allowed(&Url::parse("http://github.com/").unwrap(), &domains));
    }

    #[test]
    fn invalid_settings_are_rejected() {
        let mut config = AutoFailoverConfig::default();
        assert!(config.validate().is_ok());
        config.enabled = true;
        assert!(config.validate().is_err());
        config.subscription_url = "https://example.com/subscription".into();
        assert!(config.validate().is_ok());
        config.concurrency = 0;
        assert!(config.validate().is_err());
        config.concurrency = 5;
        config.websites.clear();
        assert!(config.validate().is_err());
        assert!(valid_url("file:///tmp/test").is_err());
    }
}
