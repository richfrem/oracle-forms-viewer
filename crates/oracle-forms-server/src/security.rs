//! Request guards for the local workbench server.
//!
//! The server is a local developer tool: it reads files from disk and can connect to
//! databases. It must therefore only answer its own UI, never another website the user
//! happens to have open, and never a DNS-rebound hostname. This module provides:
//!
//! * a **Host allow-list** (defeats DNS rebinding: `evil.example` resolving to 127.0.0.1),
//! * an **Origin / Sec-Fetch-Site check** on `/api/*` (defeats cross-site `fetch()` and
//!   form posts from other pages),
//! * [`safe_join`], which confines request-supplied relative paths to a base directory.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Hostnames that always identify the local machine.
const LOOPBACK_HOSTS: &[&str] = &["localhost", "127.0.0.1", "::1"];

#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Hostnames (without port) the server answers to.
    allowed_hosts: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self::loopback_only()
    }
}

impl SecurityConfig {
    /// Accept only loopback hostnames.
    pub fn loopback_only() -> Self {
        Self {
            allowed_hosts: LOOPBACK_HOSTS.iter().map(|h| h.to_string()).collect(),
        }
    }

    /// Loopback plus any extra hostnames listed in `OFW_ALLOWED_HOSTS` (comma-separated).
    pub fn from_env() -> Self {
        let mut cfg = Self::loopback_only();
        if let Ok(extra) = std::env::var("OFW_ALLOWED_HOSTS") {
            cfg.allowed_hosts.extend(
                extra
                    .split(',')
                    .map(|h| h.trim().to_ascii_lowercase())
                    .filter(|h| !h.is_empty()),
            );
        }
        cfg
    }

    pub fn allowed_hosts(&self) -> &[String] {
        &self.allowed_hosts
    }

    fn is_allowed_host(&self, host: &str) -> bool {
        let host = host.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
        self.allowed_hosts.contains(&host)
    }
}

/// Splits `host[:port]` (including bracketed IPv6) into its host part.
fn host_without_port(authority: &str) -> &str {
    let authority = authority.trim();
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match authority.rsplit_once(':') {
        // Only strip a numeric suffix, so a bare IPv6 address isn't mangled.
        Some((h, port)) if !h.contains(':') && port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => authority,
    }
}

fn reject(status: StatusCode, reason: &str) -> Response {
    tracing::warn!("Rejected request: {}", reason);
    (status, reason.to_string()).into_response()
}

/// Axum middleware enforcing the Host allow-list on every request and same-origin
/// requests on `/api/*`.
pub async fn guard(State(cfg): State<Arc<SecurityConfig>>, req: Request, next: Next) -> Response {
    let headers = req.headers();

    let host_header = headers.get(header::HOST).and_then(|v| v.to_str().ok()).map(str::to_string);
    if let Some(host) = &host_header {
        if !cfg.is_allowed_host(host_without_port(host)) {
            return reject(StatusCode::MISDIRECTED_REQUEST, "Host not allowed");
        }
    }

    if req.uri().path().starts_with("/api/") {
        // Browsers send Sec-Fetch-Site on every request they make.
        if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
            if site.eq_ignore_ascii_case("cross-site") {
                return reject(StatusCode::FORBIDDEN, "Cross-site API requests are not allowed");
            }
        }

        // Browsers send Origin on cross-origin requests and on same-origin POSTs.
        if let Some(origin) = headers.get(header::ORIGIN) {
            let origin = match origin.to_str() {
                Ok(o) => o,
                Err(_) => return reject(StatusCode::FORBIDDEN, "Invalid Origin header"),
            };
            if origin == "null" {
                return reject(StatusCode::FORBIDDEN, "Opaque origins are not allowed");
            }
            let authority = origin
                .strip_prefix("http://")
                .or_else(|| origin.strip_prefix("https://"))
                .unwrap_or("");
            if !cfg.is_allowed_host(host_without_port(authority)) {
                return reject(StatusCode::FORBIDDEN, "Cross-origin API requests are not allowed");
            }
            // Same origin means same host *and* port: another local web app on a
            // different port must not be able to drive this server either.
            if let Some(host) = &host_header {
                if !authority.eq_ignore_ascii_case(host) {
                    return reject(StatusCode::FORBIDDEN, "Cross-origin API requests are not allowed");
                }
            }
        }
    }

    next.run(req).await
}

/// Joins a request-supplied relative path onto `base`, refusing anything that could
/// escape it: absolute paths, drive prefixes, `..` components, or symlinks that resolve
/// outside `base`. Returns `None` if the path is unsafe or does not exist.
pub fn safe_join(base: &Path, relative: &str) -> Option<PathBuf> {
    let relative = relative.trim();
    if relative.is_empty() || relative.contains('\0') {
        return None;
    }
    let rel_path = Path::new(relative);
    for component in rel_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    // Backslashes are separators on Windows but not on Unix; refuse `..\` everywhere.
    if relative.split(['/', '\\']).any(|seg| seg == "..") {
        return None;
    }

    let canonical_base = base.canonicalize().ok()?;
    let candidate = canonical_base.join(rel_path).canonicalize().ok()?;
    candidate.starts_with(&canonical_base).then_some(candidate)
}

/// True if `name` is a single, ordinary file name (no directories, no traversal).
pub fn is_plain_file_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && Path::new(name).components().count() == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_parsing() {
        assert_eq!(host_without_port("127.0.0.1:7070"), "127.0.0.1");
        assert_eq!(host_without_port("localhost"), "localhost");
        assert_eq!(host_without_port("[::1]:7070"), "::1");
        assert_eq!(host_without_port("::1"), "::1");
    }

    #[test]
    fn safe_join_confines_paths() {
        let dir = std::env::temp_dir().join(format!("ofw-safe-join-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/file.txt"), "ok").unwrap();

        assert!(safe_join(&dir, "sub/file.txt").is_some());
        assert!(safe_join(&dir, "./sub/file.txt").is_some());
        assert!(safe_join(&dir, "../etc/hosts").is_none());
        assert!(safe_join(&dir, "sub/../../etc/hosts").is_none());
        assert!(safe_join(&dir, "sub\\..\\..\\x").is_none());
        assert!(safe_join(&dir, "/etc/hosts").is_none());
        assert!(safe_join(&dir, "").is_none());
        assert!(safe_join(&dir, "missing.txt").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn safe_join_rejects_symlink_escape() {
        let dir = std::env::temp_dir().join(format!("ofw-safe-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let link = dir.join("escape");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink("/etc", &link).unwrap();
        assert!(safe_join(&dir, "escape/hosts").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plain_file_names() {
        assert!(is_plain_file_name("orders_fmb.xml"));
        assert!(!is_plain_file_name("../orders_fmb.xml"));
        assert!(!is_plain_file_name("sub/orders_fmb.xml"));
        assert!(!is_plain_file_name("..\\x"));
        assert!(!is_plain_file_name("/etc/hosts"));
        assert!(!is_plain_file_name(".."));
        assert!(!is_plain_file_name(""));
    }
}
