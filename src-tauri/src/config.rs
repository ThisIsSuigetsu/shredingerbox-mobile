//! Сборка конфига sing-box (формат 1.12+) для Android.
//!
//! TUN поднимает не ядро, а VpnService: ядро только описывает адреса и маршруты,
//! а Kotlin-сторона строит по ним интерфейс и отдаёт его дескриптор.
//! Правила те же, что на десктопе; приложения матчатся по имени пакета.

use crate::presets;
use crate::settings::{FinalRoute, Mode, Rules, PROXY_PORT};
use serde_json::{json, Value};

/// Clash API ядра на 127.0.0.1: через него приложение переключает сервер
/// без переподключения и меряет пинг через VPN.
#[derive(Clone)]
pub struct Api {
    pub port: u16,
    pub secret: String,
}

/// «Госуслуги.рф» → «xn--c1aapkosapc.xn--p1ai»: sing-box сравнивает домены только в ASCII.
fn ascii_domain(d: &str) -> String {
    url::Url::parse(&format!("http://{d}"))
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| d.to_string())
}

fn dedup(mut v: Vec<String>) -> Vec<String> {
    v.iter_mut().for_each(|s| *s = ascii_domain(s.trim().trim_start_matches("*.")).to_ascii_lowercase());
    v.retain(|s| !s.is_empty());
    v.sort();
    v.dedup();
    v
}

/// Тег outbound'а, который сам выбирает лучший сервер (urltest).
pub const AUTO_TAG: &str = "auto";

/// `servers` - разобранные серверы как (тег, outbound), `selected_tag` - текущий.
/// Над ними строится селектор `proxy`, как на десктопе. С `auto` селектор по умолчанию
/// смотрит на urltest: ядро само меряет пинг и держит самый быстрый сервер.
pub fn build(mode: Mode, rules: &Rules, servers: &[(String, Value)], selected_tag: &str, auto: bool, api: &Api) -> Value {
    let mut domains = rules.proxy_domains.clone();
    domains.extend(presets::domains_for(&rules.presets));
    let domains = dedup(domains);
    let direct_domains = dedup(rules.direct_domains.clone());
    // Без VPN Android не говорит, какое приложение открыло соединение, -
    // правила по приложениям работают только в TUN.
    let (proxy_apps, direct_apps): (Vec<&str>, Vec<&str>) = match mode {
        Mode::Tun => (
            rules.proxy_apps.iter().map(|a| a.package.as_str()).collect(),
            rules.direct_apps.iter().map(|a| a.package.as_str()).collect(),
        ),
        Mode::Proxy => (vec![], vec![]),
    };

    let (final_out, final_dns) = match rules.final_route {
        FinalRoute::Proxy => ("proxy", "dns-remote"),
        FinalRoute::Direct => ("direct", "dns-local"),
    };

    // ВАЖНО: правило с пустым списком в sing-box совпадает со всем подряд,
    // поэтому добавляем правило только если в списке что-то есть.
    // «Сайты напрямую» - исключения (банки, госуслуги), поэтому они стоят первыми.
    // Приложений «напрямую» здесь нет: их VpnService вообще не пускает в туннель.
    let mut dns_rules = vec![];
    if !direct_domains.is_empty() {
        dns_rules.push(json!({ "domain_suffix": direct_domains, "server": "dns-local" }));
    }
    if !proxy_apps.is_empty() {
        dns_rules.push(json!({ "package_name": proxy_apps, "server": "dns-remote" }));
    }
    if !domains.is_empty() {
        dns_rules.push(json!({ "domain_suffix": domains, "server": "dns-remote" }));
    }

    let mut route_rules = vec![
        json!({ "action": "sniff" }),
        json!({ "protocol": "dns", "action": "hijack-dns" }),
        json!({ "ip_is_private": true, "outbound": "direct" }),
    ];
    if !direct_domains.is_empty() {
        route_rules.push(json!({ "domain_suffix": direct_domains, "outbound": "direct" }));
    }
    if !proxy_apps.is_empty() {
        route_rules.push(json!({ "package_name": proxy_apps, "outbound": "proxy" }));
    }
    if !domains.is_empty() {
        route_rules.push(json!({ "domain_suffix": domains, "outbound": "proxy" }));
    }

    let mut tun = json!({
        "type": "tun",
        "tag": "tun-in",
        "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
        "auto_route": true,
        "strict_route": true,
        // По умолчанию стек mixed отдаёт TCP ядру Android через слушатель на адресе
        // туннеля - на Samsung (One UI) пакеты до него не доходят и TCP молча висит.
        // gvisor принимает TCP целиком внутри sing-box.
        "stack": "gvisor"
    });
    // Пакеты отсюда Kotlin передаёт в addDisallowedApplication: такие приложения
    // ходят в сеть мимо VPN и не видят его - банки перестают ругаться.
    if !direct_apps.is_empty() {
        tun["exclude_package"] = json!(direct_apps);
    }
    let inbound = match mode {
        Mode::Tun => tun,
        Mode::Proxy => json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": PROXY_PORT
        }),
    };

    let tags: Vec<String> = servers.iter().map(|(tag, _)| tag.clone()).collect();
    let mut choices = tags.clone();
    if auto {
        choices.insert(0, AUTO_TAG.to_string());
    }
    let mut outbounds: Vec<Value> = vec![json!({
        "type": "selector",
        "tag": "proxy",
        "outbounds": choices,
        "default": if auto { AUTO_TAG } else { selected_tag },
        "interrupt_exist_connections": true
    })];
    if auto {
        outbounds.push(json!({
            "type": "urltest",
            "tag": AUTO_TAG,
            "outbounds": tags,
            "url": "https://www.gstatic.com/generate_204",
            "interval": "3m",
            // Не прыгать между серверами из-за пары миллисекунд.
            "tolerance": 50,
            "interrupt_exist_connections": true
        }));
    }
    for (tag, ob) in servers {
        let mut ob = ob.clone();
        ob["tag"] = json!(tag);
        outbounds.push(ob);
    }
    outbounds.push(json!({ "type": "direct", "tag": "direct" }));

    json!({
        // info: в журнале видно каждое соединение. Когда появится продвинутый режим,
        // уровень станет настройкой, как на десктопе.
        "log": { "level": "info", "timestamp": true },
        "dns": {
            "servers": [
                { "type": "https", "tag": "dns-remote", "server": "1.1.1.1", "detour": "proxy" },
                // Системный DNS телефона: ядро спрашивает его через Android DnsResolver.
                { "type": "local", "tag": "dns-local" }
            ],
            "rules": dns_rules,
            "final": final_dns,
            "strategy": "prefer_ipv4"
        },
        "inbounds": [inbound],
        "outbounds": outbounds,
        "route": {
            "rules": route_rules,
            "final": final_out,
            "auto_detect_interface": true,
            "default_domain_resolver": "dns-local"
        },
        "experimental": {
            "clash_api": {
                "external_controller": format!("127.0.0.1:{}", api.port),
                "secret": api.secret
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppRule;

    fn api() -> Api {
        Api { port: 9097, secret: "s".into() }
    }

    fn one() -> Vec<(String, Value)> {
        vec![("srv-a".to_string(), json!({ "type": "vless", "server": "a" }))]
    }

    #[test]
    fn selector_tun_and_api() {
        let rules = Rules { final_route: FinalRoute::Proxy, presets: vec![], ..Rules::default() };
        let c = build(Mode::Tun, &rules, &one(), "srv-a", false, &api());
        assert_eq!(c["outbounds"][0]["type"], "selector");
        assert_eq!(c["outbounds"][0]["default"], "srv-a");
        assert_eq!(c["outbounds"][1]["tag"], "srv-a");
        assert_eq!(c["outbounds"][2]["tag"], "direct");
        assert_eq!(c["inbounds"][0]["type"], "tun");
        assert_eq!(c["inbounds"][0]["stack"], "gvisor");
        assert!(c["inbounds"][0].get("exclude_package").is_none());
        assert_eq!(c["route"]["final"], "proxy");
        assert_eq!(c["dns"]["final"], "dns-remote");
        // Пустые списки не должны давать правил - иначе они совпадут со всем.
        assert_eq!(c["route"]["rules"].as_array().unwrap().len(), 3);
        assert!(c["dns"]["rules"].as_array().unwrap().is_empty());
        assert_eq!(c["experimental"]["clash_api"]["external_controller"], "127.0.0.1:9097");
    }

    #[test]
    fn rules_order_and_apps() {
        let rules = Rules {
            final_route: FinalRoute::Direct,
            proxy_apps: vec![AppRule { name: "Telegram".into(), package: "org.telegram.messenger".into() }],
            direct_apps: vec![AppRule { name: "Банк".into(), package: "ru.bank".into() }],
            proxy_domains: vec!["*.Example.com".into()],
            direct_domains: vec!["госуслуги.рф".into()],
            presets: vec!["discord".into()],
        };
        let c = build(Mode::Tun, &rules, &one(), "srv-a", false, &api());
        assert_eq!(c["route"]["final"], "direct");
        assert_eq!(c["dns"]["final"], "dns-local");
        assert_eq!(c["inbounds"][0]["exclude_package"], json!(["ru.bank"]));
        let r = c["route"]["rules"].as_array().unwrap();
        assert_eq!(r[3]["domain_suffix"], json!(["xn--c1aapkosapc.xn--p1ai"]));
        assert_eq!(r[3]["outbound"], "direct");
        assert_eq!(r[4]["package_name"], json!(["org.telegram.messenger"]));
        let proxied = r[5]["domain_suffix"].as_array().unwrap();
        assert!(proxied.contains(&json!("example.com")));
        assert!(proxied.contains(&json!("discord.gg")));
        assert_eq!(r[5]["outbound"], "proxy");
    }

    #[test]
    fn proxy_mode_has_no_tun_and_no_app_rules() {
        let rules = Rules {
            proxy_apps: vec![AppRule { name: "Telegram".into(), package: "org.telegram.messenger".into() }],
            direct_apps: vec![AppRule { name: "Банк".into(), package: "ru.bank".into() }],
            ..Rules::default()
        };
        let c = build(Mode::Proxy, &rules, &one(), "srv-a", false, &api());
        let inbounds = c["inbounds"].as_array().unwrap();
        assert_eq!(inbounds.len(), 1);
        assert_eq!(inbounds[0]["type"], "mixed");
        assert_eq!(inbounds[0]["listen"], "127.0.0.1");
        assert_eq!(inbounds[0]["listen_port"], PROXY_PORT);
        let text = c.to_string();
        assert!(!text.contains("package_name") && !text.contains("exclude_package"));
    }

    #[test]
    fn auto_select_uses_urltest() {
        let two = vec![
            ("srv-a".to_string(), json!({ "type": "vless", "server": "a" })),
            ("srv-b".to_string(), json!({ "type": "vless", "server": "b" })),
        ];
        let c = build(Mode::Tun, &Rules::default(), &two, "srv-b", true, &api());
        assert_eq!(c["outbounds"][0]["outbounds"], json!(["auto", "srv-a", "srv-b"]));
        assert_eq!(c["outbounds"][0]["default"], "auto");
        assert_eq!(c["outbounds"][1]["type"], "urltest");
        assert_eq!(c["outbounds"][1]["outbounds"], json!(["srv-a", "srv-b"]));
    }
}
