use url::Url;

const TRACKING_PARAMS: &[&str] = &["fbclid", "gclid", "msclkid", "igshid", "mc_cid", "mc_eid"];

/// 중복 판정용 키를 만든다. http/https 는 같은 것으로 보고, 프래그먼트·추적 파라미터·
/// 끝 슬래시를 버리고, 쿼리는 정렬한다. http(s) 가 아니면 원문을 그대로 쓴다.
pub fn normalize(raw: &str) -> String {
    let raw = raw.trim();
    let Ok(u) = Url::parse(raw) else {
        return raw.to_string();
    };
    if !matches!(u.scheme(), "http" | "https") {
        return raw.to_string();
    }
    let Some(host) = u.host_str() else {
        return raw.to_string();
    };

    let mut out = host.to_string();
    if let Some(port) = u.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    out.push_str(u.path().trim_end_matches('/'));

    let mut query: Vec<(String, String)> = u
        .query_pairs()
        .filter(|(k, _)| !k.starts_with("utm_") && !TRACKING_PARAMS.contains(&k.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    query.sort();
    if !query.is_empty() {
        out.push('?');
        out.push_str(
            &url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(query)
                .finish(),
        );
    }
    out
}

/// 사이트별 묶음에 쓰는 도메인. 소문자 호스트에서 `www.` 와 모바일용 `m.` 접두어를 뗀다.
/// 서브도메인은 합치지 않는다 (blog.naver.com 과 cafe.naver.com 은 따로). http(s) 가 아니면 빈 문자열.
pub fn site(raw: &str) -> String {
    let Ok(u) = Url::parse(raw.trim()) else {
        return String::new();
    };
    if !matches!(u.scheme(), "http" | "https") {
        return String::new();
    }
    let host = u.host_str().unwrap_or("");
    let trimmed = host.strip_prefix("www.").or_else(|| host.strip_prefix("m.")).unwrap_or(host);
    // "m.com" 처럼 접두어를 떼면 도메인이 아니게 되는 경우는 그대로 둔다.
    if trimmed.contains('.') { trimmed } else { host }.to_string()
}

#[cfg(test)]
mod tests {
    use super::{normalize, site};

    #[test]
    fn site_strips_common_prefixes_only() {
        assert_eq!(site("https://www.GitHub.com/rust-lang/rust"), "github.com");
        assert_eq!(site("http://m.youtube.com/watch?v=1"), "youtube.com");
        assert_eq!(site("https://blog.naver.com/x"), "blog.naver.com");
        assert_eq!(site("https://m.com/"), "m.com");
        assert_eq!(site("http://localhost:3000/"), "localhost");
        assert_eq!(site("about:config"), "");
        assert_eq!(site("not a url"), "");
    }

    #[test]
    fn scheme_slash_fragment_are_ignored() {
        let a = normalize("https://Example.com/docs/");
        assert_eq!(a, "example.com/docs");
        assert_eq!(a, normalize("http://example.com/docs#intro"));
        assert_eq!(normalize("https://example.com/"), "example.com");
    }

    #[test]
    fn tracking_params_dropped_and_query_sorted() {
        assert_eq!(
            normalize("https://example.com/p?b=2&utm_source=x&a=1&fbclid=zzz"),
            "example.com/p?a=1&b=2"
        );
    }

    #[test]
    fn non_http_kept_as_is() {
        assert_eq!(normalize("  about:config "), "about:config");
        assert_eq!(normalize("not a url"), "not a url");
    }
}
