use axum::http::{HeaderMap, header};

/// Analytics-only Session cookie parser; do not treat machine Bearer as a user session.
pub(crate) fn raw_cookie(headers: &HeaderMap) -> Option<String> {
    let mut session = None;
    // HTTP 可以有多條 Cookie，不能只讀第一條。
    for value in headers.get_all(header::COOKIE).iter() {
        let line = value.to_str().ok()?;
        for part in line.split(';').map(str::trim) {
            if let Some((name, raw)) = part.split_once('=') {
                if name != "analytics_session" {
                    continue;
                }
                if session.is_some()
                    || raw.len() != 64
                    || !raw.bytes().all(|c| c.is_ascii_hexdigit())
                {
                    return None;
                }
                session = Some(raw.to_owned());
            } else if part == "analytics_session" {
                return None;
            }
        }
    }
    session
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_preserves_exact_name_and_rejects_duplicate_or_malformed_cookie() {
        let mut headers = HeaderMap::new();
        assert_eq!(raw_cookie(&headers), None);
        headers.insert(header::COOKIE, "analytics_session=bad".parse().unwrap());
        assert_eq!(raw_cookie(&headers), None);
        let raw = "a".repeat(64);
        headers.insert(
            header::COOKIE,
            format!("x=1; analytics_session={raw}").parse().unwrap(),
        );
        assert_eq!(raw_cookie(&headers), Some(raw.clone()));
        headers.append(
            header::COOKIE,
            format!("analytics_session={raw}").parse().unwrap(),
        );
        assert_eq!(raw_cookie(&headers), None);
        headers.clear();
        headers.insert(
            header::COOKIE,
            format!("chat_session={raw}").parse().unwrap(),
        );
        assert_eq!(raw_cookie(&headers), None);
    }
}
