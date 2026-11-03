//! Настройки клиента: что хранится в settings.json в папке данных приложения.
//! Серверы и подписки устроены так же, как в десктопном клиенте.

use crate::links;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub link: String,
    /// URL подписки, из которой пришёл сервер (None — добавлен вручную).
    #[serde(default)]
    pub subscription: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Subscription {
    pub url: String,
    pub name: String,
}

/// TUN — VPN для всего телефона по правилам. Прокси — без VPN: ядро слушает
/// 127.0.0.1, и через него ходят только приложения, где этот прокси прописан.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Tun,
    Proxy,
}

/// Порт локального прокси в режиме «Прокси» — как на десктопе.
pub const PROXY_PORT: u16 = 2080;

/// Куда идёт трафик, который не попал ни в одно правило.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum FinalRoute {
    Proxy,
    #[default]
    Direct,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AppRule {
    /// Название, как в списке приложений («Telegram»).
    pub name: String,
    /// Имя пакета Android («org.telegram.messenger») — по нему и работает правило.
    pub package: String,
}

/// Правила как на десктопе, только приложения задаются пакетами, а не exe.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Rules {
    pub final_route: FinalRoute,
    pub proxy_apps: Vec<AppRule>,
    /// Эти приложения вообще не попадают в VPN и не видят его — удобно для банков.
    pub direct_apps: Vec<AppRule>,
    pub proxy_domains: Vec<String>,
    /// Сайты, которые всегда идут напрямую, — даже из приложений «через VPN» и наборов.
    pub direct_domains: Vec<String>,
    /// Включённые готовые наборы (id из presets.rs).
    pub presets: Vec<String>,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            final_route: FinalRoute::Direct,
            proxy_apps: vec![],
            direct_apps: vec![],
            proxy_domains: vec![],
            direct_domains: vec![],
            presets: vec!["youtube".into(), "discord".into()],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub servers: Vec<Server>,
    pub subscriptions: Vec<Subscription>,
    pub selected: Option<String>,
    pub mode: Mode,
    pub rules: Rules,
}

pub fn new_id() -> String {
    use rand::Rng;
    format!("{:08x}", rand::thread_rng().gen::<u32>())
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Выбранный сервер, а если выбор потерялся — первый из списка.
    pub fn selected_server(&self) -> Option<&Server> {
        self.selected
            .as_ref()
            .and_then(|id| self.servers.iter().find(|s| &s.id == id))
            .or_else(|| self.servers.first())
    }

    /// Если выбранного сервера больше нет — выбираем первый.
    fn fix_selected(&mut self) {
        let exists = self.selected.as_ref().is_some_and(|id| self.servers.iter().any(|s| &s.id == id));
        if !exists {
            self.selected = self.servers.first().map(|s| s.id.clone());
        }
    }

    /// Ключи из текста, по одному на строку. Возвращает, сколько добавлено,
    /// или первую ошибку разбора, если не подошёл ни один.
    pub fn add_keys(&mut self, text: &str) -> Result<usize, String> {
        let mut added = 0;
        let mut first_err = None;
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if self.servers.iter().any(|s| s.link == line && s.subscription.is_none()) {
                added += 1;
                continue;
            }
            match links::parse(line) {
                Ok(p) => {
                    let id = new_id();
                    // Только что добавленный ключ сразу выбран — так его ждёт пользователь.
                    self.selected = Some(id.clone());
                    self.servers.push(Server { id, name: p.name, link: line.to_string(), subscription: None });
                    added += 1;
                }
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        if added == 0 {
            return Err(first_err.unwrap_or_else(|| "Вставьте ключ или ссылку на подписку".into()));
        }
        Ok(added)
    }

    /// Добавить подписку или обновить её название.
    pub fn upsert_subscription(&mut self, url: &str, name: &str) {
        match self.subscriptions.iter_mut().find(|s| s.url == url) {
            Some(sub) => sub.name = name.to_string(),
            None => self.subscriptions.push(Subscription { url: url.to_string(), name: name.to_string() }),
        }
    }

    /// Заменить серверы одной подписки свежим списком.
    /// Сервер с тем же ключом сохраняет id — на нём будет держаться счётчик трафика за месяц.
    pub fn merge_subscription(&mut self, url: &str, links_list: Vec<String>) {
        let old_selected_link = self.selected_server().map(|srv| srv.link.clone());
        let mut old_ids: HashMap<String, String> = self
            .servers
            .iter()
            .filter(|srv| srv.subscription.as_deref() == Some(url))
            .map(|srv| (srv.link.clone(), srv.id.clone()))
            .collect();
        self.servers.retain(|srv| srv.subscription.as_deref() != Some(url));
        for link in links_list {
            if let Ok(p) = links::parse(&link) {
                self.servers.push(Server {
                    id: old_ids.remove(&link).unwrap_or_else(new_id),
                    name: p.name,
                    link,
                    subscription: Some(url.to_string()),
                });
            }
        }
        if let Some(link) = old_selected_link {
            if let Some(srv) = self.servers.iter().find(|srv| srv.link == link) {
                self.selected = Some(srv.id.clone());
            }
        }
        self.fix_selected();
    }

    pub fn remove_server(&mut self, id: &str) {
        self.servers.retain(|srv| srv.id != id);
        self.fix_selected();
    }

    pub fn remove_subscription(&mut self, url: &str) {
        self.subscriptions.retain(|sub| sub.url != url);
        self.servers.retain(|srv| srv.subscription.as_deref() != Some(url));
        self.fix_selected();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "trojan://pass@a.example.com:443#A";
    const B: &str = "trojan://pass@b.example.com:443#B";
    const C: &str = "trojan://pass@c.example.com:443#C";

    #[test]
    fn add_keys_selects_new_and_skips_duplicates() {
        let mut s = Settings::default();
        assert_eq!(s.add_keys(&format!("{A}\n\n{B}")).unwrap(), 2);
        assert_eq!(s.servers.len(), 2);
        assert_eq!(s.selected_server().unwrap().name, "B");
        assert_eq!(s.add_keys(A).unwrap(), 1);
        assert_eq!(s.servers.len(), 2);
        assert!(s.add_keys("просто текст").is_err());
    }

    #[test]
    fn merge_keeps_ids_and_selection() {
        let mut s = Settings::default();
        s.upsert_subscription("https://sub", "Моя");
        s.merge_subscription("https://sub", vec![A.into(), B.into()]);
        let b_id = s.servers[1].id.clone();
        s.selected = Some(b_id.clone());
        s.merge_subscription("https://sub", vec![C.into(), B.into()]);
        assert_eq!(s.servers.len(), 2);
        assert_eq!(s.servers[1].id, b_id);
        assert_eq!(s.selected.as_deref(), Some(b_id.as_str()));
    }

    #[test]
    fn removing_selected_picks_first() {
        let mut s = Settings::default();
        s.add_keys(A).unwrap();
        s.upsert_subscription("https://sub", "Моя");
        s.merge_subscription("https://sub", vec![B.into()]);
        let b_id = s.servers[1].id.clone();
        s.selected = Some(b_id);
        s.remove_subscription("https://sub");
        assert_eq!(s.servers.len(), 1);
        assert_eq!(s.selected_server().unwrap().name, "A");
        assert!(s.subscriptions.is_empty());
    }
}
