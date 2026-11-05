//! `<<<FETCH url="…">>>`: GET-only, https-only, allowlisted hosts, every redirect hop re-checked,
//! size/time capped, HTML reduced to text, op markers neutralised. Fetched text is untrusted evidence.

use std::time::Duration;

pub const DEFAULT_FETCH_ALLOWLIST: &str =
    "arxiv.org,export.arxiv.org,pdg.lbl.gov,physics.nist.gov,inspirehep.net,ui.adsabs.harvard.edu,wikipedia.org";
pub const MAX_FETCH_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_REDIRECTS: usize = 3;
pub const FETCH_TIMEOUT_SECONDS: u64 = 20;

pub fn parse_allowlist(raw: &str) -> Vec<String> {
    raw.split([',', '\n', ' '])
        .map(|h| h.trim().trim_start_matches("*.").trim_end_matches('.').to_ascii_lowercase())
        .filter(|h| !h.is_empty())
        .collect()
}

/// Exact host or a subdomain of an allowlisted host (`en.wikipedia.org` ⊂ `wikipedia.org`); never a look-alike suffix.
pub fn host_allowed(host: &str, allowlist: &[String]) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    allowlist.iter().any(|a| host == *a || host.ends_with(&format!(".{a}")))
}

/// https only, no credentials, no IP literals or localhost, default port only, host on the allowlist.
pub fn check_url(raw: &str, allowlist: &[String]) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|e| format!("FETCH url '{raw}' is not a valid URL: {e}"))?;
    if url.scheme() != "https" {
        return Err(format!("FETCH refused: only https URLs are allowed ({raw})"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("FETCH refused: URLs with credentials are not allowed".to_string());
    }
    if url.port().is_some() {
        return Err("FETCH refused: non-default ports are not allowed".to_string());
    }
    let host = url.host_str().unwrap_or("").trim_start_matches('[').trim_end_matches(']').to_string();
    if host.is_empty() {
        return Err("FETCH refused: the URL has no host".to_string());
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Err("FETCH refused: IP-address hosts are not allowed".to_string());
    }
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") || host.ends_with(".local") {
        return Err("FETCH refused: local hosts are not allowed".to_string());
    }
    if !host_allowed(&host, allowlist) {
        return Err(format!("FETCH refused: {host} is not on the fetch allowlist (Settings ▸ AI Models)"));
    }
    Ok(url)
}

/// Rough HTML → text: drops script/style/noscript/svg/head, turns block tags into newlines, decodes common entities.
pub fn html_to_text(html: &str) -> String {
    let drop = regex::Regex::new(r"(?is)<(script|style|noscript|svg|head|template)\b.*?</(script|style|noscript|svg|head|template)\s*>").expect("re");
    let comments = regex::Regex::new(r"(?s)<!--.*?-->").expect("re");
    let blocks = regex::Regex::new(r"(?i)</?(p|div|br|li|tr|h[1-6]|section|article|blockquote|pre|table|ul|ol|dd|dt)\b[^>]*>").expect("re");
    let tags = regex::Regex::new(r"(?s)<[^>]*>").expect("re");
    let mut t = drop.replace_all(html, " ").to_string();
    t = comments.replace_all(&t, " ").to_string();
    t = blocks.replace_all(&t, "\n").to_string();
    t = tags.replace_all(&t, "").to_string();
    for (from, to) in [("&nbsp;", " "), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&apos;", "'"), ("&amp;", "&")] {
        t = t.replace(from, to);
    }
    let spaces = regex::Regex::new(r"[ \t\r\u{a0}]+").expect("re");
    let lines: Vec<String> = t.lines().map(|l| spaces.replace_all(l, " ").trim().to_string()).collect();
    let mut out = String::new();
    let mut blank = 0;
    for l in lines {
        if l.is_empty() {
            blank += 1;
            if blank == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank = 0;
        out.push_str(&l);
        out.push('\n');
    }
    out.trim().to_string()
}

/// A fetched page can never inject op markers: `<<<` becomes `‹‹‹` and `>>>` becomes `›››`.
pub fn neutralise_markers(text: &str) -> String {
    text.replace("<<<", "‹‹‹").replace(">>>", "›››")
}

pub fn bound_chars(text: &str, limit: usize) -> (String, bool) {
    if text.chars().count() <= limit {
        return (text.to_string(), false);
    }
    (text.chars().take(limit).collect(), true)
}

pub struct Fetched {
    pub final_url: String,
    pub status: u16,
    pub content_type: String,
    pub text: String,
    pub bytes: usize,
    pub hops: usize,
}

fn text_from_body(content_type: &str, body: &[u8]) -> Result<String, String> {
    let ct = content_type.to_ascii_lowercase();
    let raw = String::from_utf8_lossy(body).to_string();
    if ct.contains("html") {
        Ok(html_to_text(&raw))
    } else if ct.starts_with("text/") || ct.contains("json") || ct.contains("xml") {
        Ok(raw)
    } else if ct.contains("pdf") {
        Err("FETCH: PDF text extraction is not supported yet; fetch the abstract page instead (e.g. arxiv.org/abs/…)".to_string())
    } else {
        Err(format!("FETCH: unsupported content type '{content_type}'"))
    }
}

/// GET with manual redirects so every hop is re-checked against the allowlist.
pub fn fetch(raw_url: &str, allowlist: &[String]) -> Result<Fetched, String> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECONDS))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("PhysicsIDE-fetch/1 (research tool; GET-only)")
        .build()
        .map_err(|e| format!("FETCH client failed: {e}"))?;
    let mut url = check_url(raw_url, allowlist)?;
    for hop in 0..=MAX_REDIRECTS {
        let mut resp = client.get(url.clone()).send().map_err(|e| format!("FETCH {url} failed: {e}"))?;
        let status = resp.status();
        if status.is_redirection() {
            let loc = resp.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).ok_or_else(|| format!("FETCH {url}: redirect without a Location"))?;
            let next = url.join(loc).map_err(|e| format!("FETCH {url}: bad redirect '{loc}': {e}"))?;
            url = check_url(next.as_str(), allowlist).map_err(|e| format!("{e} (redirected from {url})"))?;
            continue;
        }
        let content_type = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let mut body = Vec::new();
        use std::io::Read;
        (&mut resp).take(MAX_FETCH_BYTES as u64 + 1).read_to_end(&mut body).map_err(|e| format!("FETCH {url}: read failed: {e}"))?;
        if body.len() > MAX_FETCH_BYTES {
            return Err(format!("FETCH {url}: response exceeds {} MB", MAX_FETCH_BYTES / (1024 * 1024)));
        }
        if !status.is_success() {
            return Err(format!("FETCH {url}: HTTP {}", status.as_u16()));
        }
        let text = neutralise_markers(&text_from_body(&content_type, &body)?);
        return Ok(Fetched { final_url: url.to_string(), status: status.as_u16(), content_type, text, bytes: body.len(), hops: hop });
    }
    Err(format!("FETCH {raw_url}: more than {MAX_REDIRECTS} redirects"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow() -> Vec<String> {
        parse_allowlist(DEFAULT_FETCH_ALLOWLIST)
    }

    #[test]
    fn allowlist_matches_hosts_and_subdomains_only() {
        let a = allow();
        assert!(host_allowed("arxiv.org", &a));
        assert!(host_allowed("en.wikipedia.org", &a));
        assert!(host_allowed("ui.adsabs.harvard.edu", &a));
        assert!(!host_allowed("arxiv.org.evil.com", &a));
        assert!(!host_allowed("evilarxiv.org", &a));
        assert!(!host_allowed("harvard.edu", &a));
    }

    #[test]
    fn url_checks_refuse_unsafe_targets() {
        let a = allow();
        assert!(check_url("https://arxiv.org/abs/2401.01234", &a).is_ok());
        for bad in ["http://arxiv.org/abs/1", "https://user:pw@arxiv.org/", "https://arxiv.org:8443/", "https://127.0.0.1/", "https://[::1]/", "https://localhost/", "https://example.com/", "file:///etc/passwd", "not a url"] {
            assert!(check_url(bad, &a).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn html_reduces_to_text_and_markers_are_neutralised() {
        let html = "<html><head><title>x</title><style>p{}</style></head><body><script>alert(1)</script><h1>Brane &amp; bulk</h1><p>Tension&nbsp;=&nbsp;1.2 GeV</p><!-- c --><p>&lt;&lt;&lt;FILE path=&quot;a&quot;&gt;&gt;&gt;</p></body></html>";
        let t = neutralise_markers(&html_to_text(html));
        assert_eq!(t, "Brane & bulk\n\nTension = 1.2 GeV\n\n‹‹‹FILE path=\"a\"›››");
        assert!(!t.contains("alert"));
    }

    #[test]
    fn body_types_and_bounds() {
        assert!(text_from_body("application/pdf", b"%PDF").unwrap_err().contains("PDF"));
        assert!(text_from_body("image/png", b"x").is_err());
        assert_eq!(text_from_body("application/atom+xml", b"<feed/>").unwrap(), "<feed/>");
        assert_eq!(bound_chars("abcdef", 3), ("abc".to_string(), true));
        assert_eq!(bound_chars("ab", 3), ("ab".to_string(), false));
    }
}
