# ShredingerBox Mobile

[Русский](README.md) · **English**

A VPN client for Android built on the [sing-box](https://github.com/SagerNet/sing-box) core. The
younger sibling of the desktop [ShredingerBox](https://github.com/ThisIsSuigetsu/shredingerbox):
add a subscription, tap one button - connected.

**[Download the latest version](https://github.com/ThisIsSuigetsu/shredingerbox-mobile/releases/latest)** ·
[Changelog](CHANGELOG.en.md)

## Install

1. Download the `ShredingerBox-….apk` file from the [release page](https://github.com/ThisIsSuigetsu/shredingerbox-mobile/releases/latest)
   to your phone.
2. Open it. Android will ask whether apps may be installed from the browser or "Files" - allow it.
3. Open ShredingerBox and add the subscription or key you were sent.
4. Tap the big button. The first time, Android will ask to allow the VPN.

After that the app updates itself: when a new version comes out, it offers to download it and
shows what changed.

Requires Android 7 or newer on 64-bit ARM - that is almost every phone from recent years.

## Features

- **Subscriptions and keys** `vless://`, `hy2://`, `trojan://`, `ss://`: paste them, scan a QR
  code, open a `shredingerbox://add?url=…` link or just copy them - the app will offer to add them.
- **Server ping.** While the VPN is on, latency is measured through the tunnel, and servers switch
  instantly, without reconnecting.
- **Rules**, as on desktop: apps and sites via VPN or direct, ready-made sets (YouTube, Discord,
  Instagram and more). "Direct" apps do not see the VPN at all - handy for banking apps.
- **Two modes:** TUN - a VPN for the whole phone, and "Proxy" - no VPN, a local `127.0.0.1:2080`
  proxy for apps where it is set.
- **Traffic usage** per server for the month, and speed on the home screen.
- **Quick settings tile**, connecting when the phone starts, and Always-on VPN.
- The VPN keeps running even if you close the app.
- **Core log** with copying - useful if something does not work.
- English and Russian: follows the phone language, or pick one in the settings.

There is no iPhone client yet - see [the roadmap](#roadmap). A subscription works in ready-made
sing-box and Xray clients.

## Roadmap

- [x] Subscriptions and keys, servers grouped by subscription, ping and instant switching.
- [x] TUN and Proxy modes, quick settings tile, autostart and Always-on VPN.
- [x] Adding by `shredingerbox://add?url=…` link and by QR code.
- [x] Rules for apps and sites, ready-made sets, monthly traffic and speed.
- [x] Signed APK built in GitHub Actions, self-update from GitHub releases.
- [x] English language (0.5.0).
- [ ] A week or two without breakage - version 1.0.0.
- [ ] iOS - postponed: a VPN on iPhone needs a paid Apple Developer account (a free Apple ID does
  not allow Network Extension).

## How it works

- **[Tauri 2](https://v2.tauri.app/) (mobile)** - the app shell.
- **Interface** - TypeScript + CSS without frameworks, a dark theme in the desktop style.
- **Rust** - parsing keys and subscriptions, building the sing-box config, rules and ready-made
  sets, ping, traffic counter, updates. Part of it comes from the desktop client (`links.rs`,
  `presets.rs`).
- **sing-box as the `libbox` library** - built from the sing-box sources into an `.aar` with
  `gomobile`. Running the core as a separate executable, as on desktop, is not possible on a phone.
- **Kotlin** - `VpnService` with libbox, the quick settings tile and autostart. They live in a
  separate `:vpn` process so the VPN does not close together with the window.

Why not native Kotlin/Compose: the interface and all the key and config logic would have to be
written again in Kotlin, and then once more in Swift for iOS. With Tauri the shared part is one
for all platforms, and only the VPN service itself is platform-specific.

## Building

You need:

- Android Studio (Android SDK, platform API 35+, Build-Tools, NDK, the bundled JBR 21).
- Rust with the `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`,
  `i686-linux-android` targets.
- Node.js and npm.
- Go and `gomobile` - to build libbox.

On Windows you also need **Developer Mode** turned on (Settings → System → For developers): Tauri
puts the built library into the Android project as a symbolic link.

A debug APK for arm64 phones:

```bash
npm install
ARCHS=arm64 scripts/build-libbox.sh   # sing-box core → src-tauri/gen/android/app/libs/libbox.aar
npx tauri android build --debug --apk --target aarch64
```

`scripts/build-libbox.sh` downloads the sources of the right sing-box version into `build/`,
installs `gomobile` and builds the library with only the protocols the client supports. Without
`ARCHS` all four architectures are built. On Windows run the script from Git Bash.

To look at the interface without a phone: `npm run dev` and open `http://localhost:1420` - demo
data is used instead of Tauri. To run on a connected phone with live interface reload:
`npx tauri android dev`.

## Releasing

GitHub Actions (`.github/workflows/release.yml`) builds the release from a `v*` tag: the core, a
signed arm64 APK and a release with it. The release description is the section of this version
from [CHANGELOG.md](CHANGELOG.md) and [CHANGELOG.en.md](CHANGELOG.en.md) - the app shows the one
in its language in the update window.

1. Add a `## X.Y.Z` section to `CHANGELOG.md` and `CHANGELOG.en.md` - what changed, in plain words.
2. Bump the version in `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`.
3. Commit, tag `vX.Y.Z` and push the commit and the tag.

The build checks that the tag matches `package.json` and that both changelogs have its section.

Signing: the key is the same forever - Android will not install an APK signed with another key
over the installed one. The key is stored in the repository secrets `ANDROID_KEYSTORE_BASE64`,
`ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`. Keep a copy outside the
repository - without it the next version cannot be released. Debug builds are signed with a
different key, so a release will not install over them: uninstall the debug version once.

## License

[GPL-3.0-or-later](LICENSE), like sing-box.
