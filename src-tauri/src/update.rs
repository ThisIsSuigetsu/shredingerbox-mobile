//! Своё обновление: новая версия ищется в релизах GitHub, APK скачивается
//! в кэш приложения, а установку делает системный установщик Android.

use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/ThisIsSuigetsu/shredingerbox-mobile/releases/latest";

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    /// Описание релиза с GitHub.
    pub notes: Option<String>,
    /// Ссылка на APK из релиза.
    pub url: String,
    pub size: u64,
}

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("ShredingerBox/", env!("CARGO_PKG_VERSION")))
        .timeout(timeout)
        .build()
        .map_err(|e| e.to_string())
}

/// «v0.2.0» → (0, 2, 0). Хвост вроде «-beta» отбрасываем.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0), parts.next().flatten().unwrap_or(0)))
}

/// Начало английской половины описания релиза (её пишет release.yml).
const ENGLISH: &str = "## English";

/// Описание релиза на нужном языке без инструкции для первой установки: тому, кто
/// обновляется, она не нужна. В старых релизах английской половины нет - тогда русская.
/// Строки пунктов в CHANGELOG перенесены по ширине - склеиваем их, чтобы в окне
/// обновления текст переносился сам.
fn notes(body: &str, en: bool) -> String {
    let changes = match body.split_once(ENGLISH) {
        Some((_, english)) if en => english.split("**First install**").next().unwrap_or_default(),
        _ => {
            let russian = body.split(ENGLISH).next().unwrap_or_default();
            russian.split("**Первая установка**").next().unwrap_or_default()
        }
    };
    let changes = changes.trim().trim_end_matches("---");
    let mut out: Vec<String> = vec![];
    for line in changes.trim().lines() {
        match out.last_mut() {
            Some(prev) if line.starts_with("  ") && !prev.is_empty() => {
                prev.push(' ');
                prev.push_str(line.trim());
            }
            _ => out.push(line.trim_end().to_string()),
        }
    }
    out.join("\n")
}

/// Последний релиз на GitHub, если он новее `current` и в нём есть APK.
pub async fn check(current: &str, en: bool) -> Result<Option<UpdateInfo>, String> {
    let text = client(Duration::from_secs(15))?
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| crate::tr!("Не удалось проверить обновления: {e}", "Could not check for updates: {e}"))?
        .error_for_status()
        .map_err(|e| crate::tr!("GitHub ответил ошибкой: {e}", "GitHub returned an error: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let release: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let tag = release["tag_name"].as_str().unwrap_or_default();
    let (Some(latest), Some(now)) = (parse_version(tag), parse_version(current)) else {
        return Ok(None);
    };
    if latest <= now {
        return Ok(None);
    }
    let apk = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(".apk")));
    let Some(apk) = apk else { return Ok(None) };
    Ok(Some(UpdateInfo {
        version: tag.trim_start_matches('v').to_string(),
        current: current.to_string(),
        notes: release["body"].as_str().map(|b| notes(b, en)).filter(|s| !s.is_empty()),
        url: apk["browser_download_url"].as_str().unwrap_or_default().to_string(),
        size: apk["size"].as_u64().unwrap_or(0),
    }))
}

pub async fn download(url: &str, to: &Path) -> Result<(), String> {
    let bytes = client(Duration::from_secs(600))?
        .get(url)
        .send()
        .await
        .map_err(|e| crate::tr!("Обновление не скачалось: {e}", "The update did not download: {e}"))?
        .error_for_status()
        .map_err(|e| crate::tr!("Обновление не скачалось: {e}", "The update did not download: {e}"))?
        .bytes()
        .await
        .map_err(|e| crate::tr!("Обновление не скачалось: {e}", "The update did not download: {e}"))?;
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(to, &bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("1.3"), Some((1, 3, 0)));
        assert_eq!(parse_version("v1.0.0-beta.1"), Some((1, 0, 0)));
        assert_eq!(parse_version("nightly"), None);
        assert!(parse_version("v0.10.0") > parse_version("v0.9.9"));
    }

    #[test]
    fn notes_without_install_guide() {
        assert_eq!(notes("Что нового.\n\n**Первая установка**\n1. Скачайте", false), "Что нового.");
        assert_eq!(notes("  Только текст  ", true), "Только текст");
        assert_eq!(
            notes("- Первый пункт,\n  его продолжение.\n- Второй.\n\nАбзац", false),
            "- Первый пункт, его продолжение.\n- Второй.\n\nАбзац"
        );
        let both = "- Новое.\n\n**Первая установка**\n1. Скачайте\n\n---\n\n## English\n\n- New.\n\n**First install**\n1. Download";
        assert_eq!(notes(both, false), "- Новое.");
        assert_eq!(notes(both, true), "- New.");
    }
}
