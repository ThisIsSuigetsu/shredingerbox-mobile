//! Загрузка подписки по ссылке.

use crate::links;
use crate::settings::SubUsage;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::time::Duration;

pub struct Fetched {
    /// Название из заголовка `profile-title` (3x-ui, Marzban и др.), иначе домен.
    pub name: String,
    pub links: Vec<String>,
    pub usage: Option<SubUsage>,
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
    let usage = resp
        .headers()
        .get("subscription-userinfo")
        .and_then(|v| v.to_str().ok())
        .and_then(userinfo);
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let links = links::parse_subscription_body(&body);
    if links.is_empty() {
        return Err(crate::tr!("В подписке не нашлось ни одного поддерживаемого сервера", "The subscription has no supported servers"));
    }
    let name = title.unwrap_or_else(|| host_of(url));
    Ok(Fetched { name, links, usage })
}

/// `upload=1; download=2; total=3; expire=4` - так расход и срок отдают 3x-ui, Marzban,
/// Remnawave и другие панели. Нет ни трафика, ни срока - None.
fn userinfo(raw: &str) -> Option<SubUsage> {
    let mut usage = SubUsage { used: 0, total: 0, expire: 0 };
    let mut any = false;
    for part in raw.split(';') {
        let Some((key, value)) = part.split_once('=') else { continue };
        // Некоторые панели пишут числа с точкой: «1.5e9» или «123.0».
        let Ok(n) = value.trim().parse::<f64>() else { continue };
        let n = n.max(0.0) as u64;
        match key.trim().to_ascii_lowercase().as_str() {
            "upload" | "download" => usage.used += n,
            "total" => usage.total = n,
            "expire" => usage.expire = n,
            _ => continue,
        }
        any = true;
    }
    (any && (usage.total > 0 || usage.expire > 0 || usage.used > 0)).then_some(usage)
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

    #[test]
    fn usage_header() {
        assert_eq!(
            userinfo("upload=100; download=900; total=5000; expire=1800000000"),
            Some(SubUsage { used: 1000, total: 5000, expire: 1800000000 })
        );
        assert_eq!(userinfo("upload=0; download=10; total=0; expire=0"), Some(SubUsage { used: 10, total: 0, expire: 0 }));
        assert_eq!(userinfo("upload=0; download=0; total=0; expire=0"), None);
        assert_eq!(userinfo("garbage"), None);
    }
}
