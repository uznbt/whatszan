//! WhatsApp URL routing and anti-tracker bypass, ported from `src/util.mjs`.

use url::Url;

pub const URL_SCHEME: &str = "whatsapp";

/// Convert `whatsapp://`, `chat.whatsapp.com`, `wa.me`, and
/// `api.whatsapp.com/send` links into a URL that WhatsApp Web understands.
pub fn convert_whatsapp_url(input: &str) -> Option<String> {
    if input.starts_with(&format!("{URL_SCHEME}:")) {
        let parsed = Url::parse(input).ok()?;
        let host = parsed.host_str()?;
        let query = parsed.query().map(|q| format!("?{q}")).unwrap_or_default();
        let fragment = parsed
            .fragment()
            .map(|f| format!("#{f}"))
            .unwrap_or_default();

        if host == "chat" {
            return Some(format!("https://web.whatsapp.com/accept{query}"));
        }

        return Some(format!(
            "https://web.whatsapp.com/{}{}{}{}",
            host,
            parsed.path(),
            query,
            fragment
        ));
    }

    let parsed = Url::parse(input).ok()?;
    match parsed.host_str()? {
        "chat.whatsapp.com" => {
            let code = parsed.path().trim_start_matches('/');
            Some(format!("https://web.whatsapp.com/accept?code={code}"))
        }
        "wa.me" => {
            let phone = parsed.path().trim_start_matches('/');
            let search = parsed.query().map(|q| format!("&{q}")).unwrap_or_default();
            Some(format!(
                "https://web.whatsapp.com/send?phone={phone}{search}"
            ))
        }
        "api.whatsapp.com" if parsed.path().starts_with("/send") => {
            let query = parsed.query().unwrap_or_default();
            Some(format!("https://web.whatsapp.com/send?{query}"))
        }
        _ => None,
    }
}

/// WhatsApp wraps outbound links in `l.wl.co/l?u=<target>`. Unwrap the target
/// so clicking a link never opens the tracker in an external browser.
pub fn extract_real_url(input: &str) -> String {
    if let Ok(parsed) = Url::parse(input) {
        if parsed.host_str() == Some("l.wl.co") {
            if let Some((_, target)) = parsed.query_pairs().find(|(key, _)| key.as_ref() == "u") {
                return target.into_owned();
            }
        }
    }
    input.to_string()
}

/// `https://web.whatsapp.com/favicon/1x/f12/...` -> 12.
pub fn unread_count_from_favicon(favicon_url: &str) -> u32 {
    const MARKER: &str = "https://web.whatsapp.com/favicon/1x/f";
    let Some(start) = favicon_url.find(MARKER) else {
        return 0;
    };
    let rest = &favicon_url[start + MARKER.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(0)
}

pub fn is_whatsapp_link(input: &str) -> bool {
    if input.starts_with(&format!("{URL_SCHEME}:")) {
        return true;
    }
    Url::parse(input)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| {
            matches!(
                host.as_str(),
                "chat.whatsapp.com" | "api.whatsapp.com" | "wa.me"
            )
        })
}

pub fn is_web_whatsapp(input: &str) -> bool {
    Url::parse(input)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "web.whatsapp.com")
}

/// WhatsApp-owned infrastructure that the web client loads in subframes.
///
/// The hidden `flows.whatsapp.net/flows/cache_management/` iframe is created by
/// WhatsApp's cache cleaner. Navigation policy must let it load in place;
/// treating it as an external link would open the user's browser and break the
/// cleanup. `web.whatsapp.com` is the application itself.
pub fn is_whatsapp_internal(input: &str) -> bool {
    Url::parse(input)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| {
            host == "web.whatsapp.com" || host == "whatsapp.net" || host.ends_with(".whatsapp.net")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whatsapp_scheme_chat_is_rewritten_to_accept() {
        assert_eq!(
            convert_whatsapp_url("whatsapp://chat?code=ABC123").as_deref(),
            Some("https://web.whatsapp.com/accept?code=ABC123")
        );
    }

    #[test]
    fn group_invite_is_rewritten() {
        assert_eq!(
            convert_whatsapp_url("https://chat.whatsapp.com/XYZ").as_deref(),
            Some("https://web.whatsapp.com/accept?code=XYZ")
        );
    }

    #[test]
    fn wa_me_keeps_query_parameters() {
        assert_eq!(
            convert_whatsapp_url("https://wa.me/628123456789?text=Halo").as_deref(),
            Some("https://web.whatsapp.com/send?phone=628123456789&text=Halo")
        );
    }

    #[test]
    fn tracker_links_are_unwrapped() {
        assert_eq!(
            extract_real_url("https://l.wl.co/l?u=https%3A%2F%2Fexample.com%2Fpath"),
            "https://example.com/path"
        );
        assert_eq!(
            extract_real_url("https://example.com"),
            "https://example.com"
        );
    }

    #[test]
    fn unread_count_parses_favicons() {
        assert_eq!(
            unread_count_from_favicon("https://web.whatsapp.com/favicon/1x/f12/abc.png"),
            12
        );
        assert_eq!(
            unread_count_from_favicon("https://web.whatsapp.com/favicon/1x/favicon.png"),
            0
        );
    }

    #[test]
    fn recognizes_whatsapp_hosts() {
        assert!(is_whatsapp_link("https://wa.me/123"));
        assert!(is_whatsapp_link("whatsapp://send?text=hi"));
        assert!(!is_whatsapp_link("https://example.com"));
        assert!(is_web_whatsapp("https://web.whatsapp.com/"));
    }

    #[test]
    fn recognizes_whatsapp_internal_hosts() {
        assert!(is_whatsapp_internal("https://web.whatsapp.com/"));
        assert!(is_whatsapp_internal(
            "https://flows.whatsapp.net/flows/cache_management/"
        ));
        assert!(is_whatsapp_internal(
            "https://static.whatsapp.net/media/icon.png"
        ));
        assert!(!is_whatsapp_internal("https://chat.whatsapp.com/ABC"));
        assert!(!is_whatsapp_internal("https://example.com/"));
    }
}
