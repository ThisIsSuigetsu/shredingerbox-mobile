//! Загрузка подписки по ссылке.

use crate::links;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::time::Duration;

pub struct Fetched {
    /// Название из заголовка `profile-title` (3x-ui, Marzban и др.), иначе домен.
    pub name: String,
    pub links: Vec<String>,
}

pub async fn fetch(url: &str) -> Result<Fetched, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("ShredingerBox/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| crate::tr!("Подписка не загрузилась: {e}", "The subscription did not load: {e}"))?
        .error_for_status()
        .map_err(|e| crate::tr!("Сервер подписки ответил ошибкой: {e}", "The subscription server returned an error: {e}"))?;
    let title = resp
        .headers()
        .get("profile-title")
        .and_then(|v| v.to_str().ok())
        .and_then(profile_title);
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let links = links::parse_subscription_body(&body);
    if links.is_empty() {
        return Err(crate::tr!("В подписке не нашлось ни одного поддерживаемого сервера", "The subscription has no supported servers"));
    }
    let name = title.unwrap_or_else(|| host_of(url));
    Ok(Fetched { name, links })
}

/// `profile-title` бывает обычным текстом или `base64:…`.
fn profile_title(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let title = match raw.strip_prefix("base64:") {
        Some(b64) => String::from_utf8(STANDARD.decode(b64.trim()).ok()?).ok()?,
        None => raw.to_string(),
    };
    let title = title.trim().to_string();
    (!title.is_empty()).then_some(title)
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| crate::tr!("Подписка", "Subscription"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles() {
        assert_eq!(profile_title("base64:0JzQvtGP"), Some("Моя".into()));
        assert_eq!(profile_title(" Friends VPN "), Some("Friends VPN".into()));
        assert_eq!(profile_title(""), None);
        assert_eq!(host_of("https://sub.example.com:2096/sub/abc"), "sub.example.com");
    }
}
