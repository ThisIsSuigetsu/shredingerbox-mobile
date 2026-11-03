# ShredingerBox Mobile

Мобильный VPN-клиент на ядре [sing-box](https://github.com/SagerNet/sing-box) — Android первым,
позже iOS. Младший брат десктопного [ShredingerBox](https://github.com/ThisIsSuigetsu/shredingerbox):
добавил подписку, нажал одну кнопку — подключено.

> Статус: MVP для Android — подключение по ключу или подписке, список серверов, журнал ядра. Пинга и правил пока нет.

## Стек

- **[Tauri 2](https://v2.tauri.app/) (mobile)** — оболочка приложения.
- **Интерфейс** — TypeScript + CSS без фреймворков, тёмная тема в стиле десктопа.
- **Rust** — разбор ключей и подписок, сборка конфига sing-box, правила и готовые наборы.
  Переносится из десктопного клиента (`links.rs`, `config.rs`, `presets.rs`).
- **sing-box как библиотека `libbox`** — собирается `gomobile` из исходников sing-box в `.aar`
  (на iOS — `.xcframework`). Запускать ядро отдельным exe, как на десктопе, на телефоне нельзя.
- **Kotlin-плагин Tauri** с `VpnService`: поднимает TUN-интерфейс и отдаёт его файловый
  дескриптор в libbox.
- **iOS (позже)** — Swift Network Extension (`NEPacketTunnelProvider`) с тем же libbox.
  Нужен платный аккаунт Apple Developer.

Почему не нативный Kotlin/Compose: пришлось бы заново писать интерфейс и всю логику ключей и
конфига на Kotlin, а потом ещё раз на Swift для iOS. С Tauri общая часть одна на все платформы,
а платформенным остаётся только сам VPN-сервис.

## План

### Этап 1 — MVP (Android)
- [ ] Добавить подписку по ссылке или отдельный ключ (`vless://`, `hy2://`, `trojan://`, `ss://`).
- [ ] Список серверов, сгруппированный по подпискам.
- [ ] Подключение через `VpnService` + libbox, уведомление и плитка в шторке.
- [ ] Пинг серверов.

### Этап 2 — правила
- [ ] Приложения через VPN или напрямую — по имени пакета, выбор из списка установленных.
- [ ] Сайты через VPN или напрямую.
- [ ] Готовые наборы сайтов (как на десктопе).
- [ ] Расход трафика за месяц.

### Этап 3 — сборка и обновления
- [ ] Сборка подписанного APK в GitHub Actions (вместе со сборкой libbox).
- [ ] Своё обновление: проверить релиз на GitHub, скачать APK и открыть установщик.

### Этап 4 — iOS
- [ ] Network Extension с libbox, тот же интерфейс и Rust-логика.
- [ ] Раздача через TestFlight или AltStore.

## Что нужно для сборки

- Android Studio (Android SDK, платформа API 35+, Build-Tools, NDK, встроенная JBR 21).
- Rust с таргетами `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`,
  `i686-linux-android`.
- Node.js и npm.
- Go и `gomobile` — для сборки libbox.

На Windows ещё нужен включённый **режим разработчика** (Параметры → Система → Для
разработчиков): Tauri кладёт собранную библиотеку в Android-проект символической ссылкой.

Отладочный APK для телефонов на arm64:

```bash
npm install
ARCHS=arm64 scripts/build-libbox.sh   # ядро sing-box → src-tauri/gen/android/app/libs/libbox.aar
npx tauri android build --debug --apk --target aarch64
```

`scripts/build-libbox.sh` скачивает исходники sing-box нужной версии в `build/`, ставит
`gomobile` и собирает библиотеку только с теми протоколами, что умеет клиент. Без `ARCHS`
собираются все четыре архитектуры — так нужно для релиза. На Windows скрипт запускается из Git Bash.

Запуск на подключённом телефоне с перезагрузкой интерфейса на лету: `npx tauri android dev`.

## Лицензия

[GPL-3.0-or-later](LICENSE), как и sing-box.
