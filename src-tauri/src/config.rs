//! Сборка конфига sing-box (формат 1.12+) для Android.
//!
//! TUN поднимает не ядро, а VpnService: ядро только описывает адреса и маршруты,
//! а Kotlin-сторона строит по ним интерфейс и отдаёт его дескриптор.
//! Правил пока нет — весь трафик идёт через сервер, кроме локальной сети.

use serde_json::{json, Value};

/// `servers` — разобранные серверы как (тег, outbound), `selected_tag` — текущий.
/// Над ними строится селектор `proxy`, как на десктопе: так позже можно будет
/// переключать сервер без переподключения.
pub fn build(servers: &[(String, Value)], selected_tag: &str) -> Value {
    let mut outbounds: Vec<Value> = vec![json!({
        "type": "selector",
        "tag": "proxy",
        "outbounds": servers.iter().map(|(tag, _)| tag.clone()).collect::<Vec<_>>(),
        "default": selected_tag,
        "interrupt_exist_connections": true
    })];
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
            "final": "dns-remote",
            "strategy": "prefer_ipv4"
        },
        "inbounds": [{
            "type": "tun",
            "tag": "tun-in",
            "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
            "auto_route": true,
            "strict_route": true,
            // По умолчанию стек mixed отдаёт TCP ядру Android через слушатель на адресе
            // туннеля — на Samsung (One UI) пакеты до него не доходят и TCP молча висит.
            // gvisor принимает TCP целиком внутри sing-box.
            "stack": "gvisor"
        }],
        "outbounds": outbounds,
        "route": {
            "rules": [
                { "action": "sniff" },
                { "protocol": "dns", "action": "hijack-dns" },
                { "ip_is_private": true, "outbound": "direct" }
            ],
            "final": "proxy",
            "auto_detect_interface": true,
            "default_domain_resolver": "dns-local"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_tun_and_final() {
        let servers = vec![("srv-a".to_string(), json!({ "type": "vless", "server": "a" }))];
        let c = build(&servers, "srv-a");
        assert_eq!(c["outbounds"][0]["type"], "selector");
        assert_eq!(c["outbounds"][0]["default"], "srv-a");
        assert_eq!(c["outbounds"][1]["tag"], "srv-a");
        assert_eq!(c["outbounds"][2]["tag"], "direct");
        assert_eq!(c["inbounds"][0]["type"], "tun");
        assert_eq!(c["inbounds"][0]["stack"], "gvisor");
        assert_eq!(c["route"]["final"], "proxy");
    }
}
