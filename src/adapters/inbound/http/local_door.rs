//! The API's only door is the network, and it has no login, so it refuses what
//! a web page can send: any request carrying `Origin` (a browser adds it on
//! cross-origin requests and preflights) and any `Host` that is not loopback
//! (DNS rebinding). A CLI or a desktop client sends neither. A request with no
//! `Host` at all (HTTP/1.0, in-process tests) passes: no browser sends one.

use axum::{
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};

pub async fn local_door(request: Request, next: Next) -> Result<Response, StatusCode> {
    let headers = request.headers();
    if headers.contains_key(header::ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(host) = headers.get(header::HOST) {
        let loopback = host.to_str().is_ok_and(is_loopback_host);
        if !loopback {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(next.run(request).await)
}

/// `127.0.0.1`, `localhost` or `[::1]`, with or without a port; nothing else.
fn is_loopback_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((inside, "")) => return inside == "::1",
            Some((inside, port)) if port.starts_with(':') => return inside == "::1",
            _ => return false,
        },
        None => host.split_once(':').map_or(host, |(name, _)| name),
    };
    name == "127.0.0.1" || name.eq_ignore_ascii_case("localhost")
}
