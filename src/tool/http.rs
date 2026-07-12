use std::net::{IpAddr, Ipv6Addr};
use std::str::FromStr;
use std::time::Duration;

use reqwest::header::{HeaderName, HeaderValue};
use tokio::time::timeout;

use crate::models::tool::{Tool, ToolFunction};

// Sensitive headers that should never be echoed back to the LLM
const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "proxy-authorization",
];

// Maximum response body size (1 MB)
const MAX_RESPONSE_BODY: usize = 1_048_576;

// Clamp timeout to sensible bounds
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_TIMEOUT_SECS: u64 = 60;

// Max redirects
const MAX_REDIRECTS: usize = 5;

pub fn http_request_tool() -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: ToolFunction {
            name: "http_request".to_string(),
            description:
                "Perform an HTTP request to a public URL. Supports GET, POST, PUT, DELETE. \
                 Headers and an optional body can be provided. Internal/private IP addresses \
                 are blocked (SSRF protection). Response includes status code, relevant \
                 headers, and the body (truncated at 1 MB)."
                    .to_string(),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to send the request to (must be http:// or https://)"
                    },
                    "method": {
                        "type": "string",
                        "description": "HTTP method. Defaults to GET.",
                        "enum": ["GET", "POST", "PUT", "DELETE"]
                    },
                    "headers": {
                        "type": "object",
                        "description": "Optional HTTP headers as key-value pairs."
                    },
                    "body": {
                        "type": "string",
                        "description": "Optional request body string (used with POST/PUT)."
                    },
                    "timeout_seconds": {
                        "type": "integer",
                        "description": "Request timeout in seconds (max 60). Defaults to 30."
                    }
                },
                "additionalProperties": false,
                "required": ["url"]
            })),
            strict: Some(true),
        },
    }
}

// ── SSRF protection ────────────────────────────────────────────────────

/// Returns true if the IP address is in a private / reserved / bogon range.
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                // 0.0.0.0/8 (bogon)
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || is_private_ipv6(&v6)
        }
    }
}

/// Check for private IPv6 ranges: fc00::/7 (unique local) and fe80::/10 (link-local).
fn is_private_ipv6(v6: &Ipv6Addr) -> bool {
    let segments = v6.segments();
    // fc00::/7 — unique local addresses
    if segments[0] & 0xfe00 == 0xfc00 {
        return true;
    }
    // fe80::/10 — link-local
    if segments[0] & 0xffc0 == 0xfe80 {
        return true;
    }
    false
}

/// Resolve a hostname and verify that *no* resolved address is private.
async fn validate_public_host(host: &str) -> Result<(), String> {
    // Fast path: try parsing as an IP address directly.
    if let Ok(ip) = IpAddr::from_str(host) {
        if is_private_ip(ip) {
            return Err(format!(
                "Blocked: {} is a private/internal IP address. Only public URLs are allowed.",
                host
            ));
        }
        return Ok(());
    }

    // Resolve hostname.
    let addrs = tokio::net::lookup_host((host, 0))
        .await
        .map_err(|e| format!("Failed to resolve host '{}': {}", host, e))?;

    for addr in addrs {
        if is_private_ip(addr.ip()) {
            return Err(format!(
                "Blocked: host '{}' resolves to private/internal IP {}. Only public URLs are allowed.",
                host, addr.ip()
            ));
        }
    }

    Ok(())
}

// ── Execution ──────────────────────────────────────────────────────────

pub async fn http_request_exec(
    url: &str,
    method: &str,
    headers: Option<&serde_json::Value>,
    body: Option<&str>,
    timeout_secs: Option<u64>,
) -> String {
    // Clamp timeout.
    let timeout_secs = timeout_secs
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
        .clamp(1, MAX_TIMEOUT_SECS);

    // ── Validate URL scheme ─────────────────────────────────────────
    let parsed_url = match reqwest::Url::parse(url) {
        Ok(u) => u,
        Err(e) => return format!("Invalid URL '{}': {}", url, e),
    };

    let scheme = parsed_url.scheme();
    if scheme != "http" && scheme != "https" {
        return format!(
            "Blocked: URL scheme '{}' is not allowed. Only http:// and https:// are supported.",
            scheme
        );
    }

    // ── Validate host is public (SSRF guard) ────────────────────────
    if let Some(host) = parsed_url.host_str() {
        if let Err(e) = validate_public_host(host).await {
            return e;
        }
    } else {
        return format!("Could not extract host from URL: {}", url);
    }

    // ── Build client ────────────────────────────────────────────────
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        .timeout(Duration::from_secs(60))
        .user_agent("babak_code/0.1 (AI coding assistant)")
        .build()
    {
        Ok(c) => c,
        Err(e) => return format!("Failed to build HTTP client: {}", e),
    };

    let http_method = match method.to_uppercase().as_str() {
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        _ => reqwest::Method::GET,
    };

    let mut req = client.request(http_method.clone(), parsed_url.clone());

    // ── Add headers ─────────────────────────────────────────────────
    if let Some(headers_obj) = headers {
        if let Some(obj) = headers_obj.as_object() {
            for (key, value) in obj {
                let val_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if let (Ok(name), Ok(val)) =
                    (HeaderName::from_str(key), HeaderValue::from_str(&val_str))
                {
                    req = req.header(name, val);
                }
            }
        }
    }

    // ── Add body ─────────────────────────────────────────────────────
    if let Some(b) = body {
        if !b.is_empty() {
            req = req.body(b.to_string());
        }
    }

    // ── Execute with timeout ─────────────────────────────────────────
    let response = match timeout(Duration::from_secs(timeout_secs), req.send()).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => return format!("HTTP request failed: {}", e),
        Err(_) => {
            return format!("Request timed out after {} seconds.", timeout_secs);
        }
    };

    // ── Collect status ───────────────────────────────────────────────
    let status = response.status();
    let status_code = status.as_u16();
    let status_text = status.canonical_reason().unwrap_or("");

    // ── Filter and collect response headers (skip sensitive ones) ───
    let mut resp_headers: Vec<String> = Vec::new();
    for (name, value) in response.headers() {
        let name_str = name.as_str().to_lowercase();
        if !SENSITIVE_HEADERS.contains(&name_str.as_str()) {
            resp_headers.push(format!(
                "{}: {}",
                name_str,
                value.to_str().unwrap_or("<binary>")
            ));
        }
    }
    resp_headers.sort();

    // ── Read body (capped) ───────────────────────────────────────────
    let body_bytes = match timeout(Duration::from_secs(30), response.bytes()).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(e)) => return format!("Failed to read response body: {}", e),
        Err(_) => return "Timed out while reading response body.".to_string(),
    };

    let truncated = body_bytes.len() > MAX_RESPONSE_BODY;
    let body_slice = if truncated {
        &body_bytes[..MAX_RESPONSE_BODY]
    } else {
        &body_bytes
    };

    // Try UTF-8; fall back to lossy if invalid.
    let body_text = String::from_utf8_lossy(body_slice);

    // ── Format output ─────────────────────────────────────────────────
    let mut output = format!(
        "{} {}\n\nStatus: {} {}\n\nHeaders:\n",
        http_method, parsed_url, status_code, status_text,
    );

    for h in &resp_headers {
        output.push_str(&format!("  {}\n", h));
    }

    output.push_str(&format!(
        "\nBody ({} bytes{}{}):\n\n{}",
        body_bytes.len(),
        if truncated { ", truncated to 1 MB" } else { "" },
        if body_text.is_empty() { " (empty)" } else { "" },
        body_text,
    ));

    output
}