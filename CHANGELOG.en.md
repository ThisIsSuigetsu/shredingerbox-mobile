# Changelog

## 0.7.0

- A subscription shows how much traffic is left and until when it is valid - if the panel
  reports it. When traffic or time is running out, the line is highlighted.
- Subscriptions refresh by themselves on launch if more than 12 hours have passed: new servers
  and addresses from the panel show up without pressing a button.
- "Best server automatically" in "Settings": the app picks the server with the lowest ping and
  switches if it stops responding.
- Minor fixes.

## 0.6.0

- Light theme. By default it follows the system, and you can pick one in "Settings" → "Theme".
- Servers can be sorted by ping: the "By ping" button on the "Servers" tab.
- Minor fixes.

## 0.5.0

- English language. The app picks it from the phone language, and you can switch it in
  "Settings" → "Language".

## 0.4.3

- Fixed the QR code scanner: in 0.4.x it did not open and showed a confusing error
  about `getClass()`.

## 0.4.2

- After an update the app shows a "What's new" window with the changes in this version.
- The description in the update window now looks like a list, not a solid block of text.
- Minor fixes.

## 0.4.1

- Closing the app while the VPN is on - with "Back" or by swiping it away from recents - no
  longer turns the VPN off. Before, the whole process closed together with the window, and the
  core with it.
- "Back" with the VPN on minimizes the app instead of closing it, so it opens instantly later.
  With the VPN off it closes as usual.
- If Android stops the VPN by itself, for example when memory is low, the app notices and shows
  "Off" instead of staying on "Connected".

## 0.4.0

- A quick settings tile: turn the VPN on and off without opening the app. You can add it in
  "Settings" → "Startup".
- "When the phone starts": the VPN connects by itself after a reboot.
- Support for Android's "Always-on VPN" from the system settings.
- `shredingerbox://add?url=…` links open the app and offer to add a subscription or key.
- The "Add" window has a "Scan a QR code" button.
- The "Core log" card collapses too, like "Mode".
- Every release description has a short first install guide.

## 0.3.0

- Monthly traffic per server: next to the server in the list and for the current one on the home
  screen. The counter resets on the 1st.
- Download and upload speed on the home screen while the VPN is on.
- The "Mode" card on the home screen collapses with a tap on its title and remembers it.

## 0.2.0

First release.

- `vless://`, `hy2://`, `trojan://`, `ss://` keys and subscriptions: paste, scan or copy them -
  the app will offer to add them by itself. Servers are grouped by subscription.
- Server ping. While the VPN is on, latency is measured through the tunnel, and switching servers
  is instant, without reconnecting.
- Rules, as on desktop: apps and sites via VPN or direct, ready-made sets (YouTube, Discord,
  Instagram and more), "other traffic". "Direct" apps do not see the VPN at all - handy for banks.
- Two modes, switched with a pill on the home screen: TUN - a VPN for the whole phone, and
  "Proxy" - no VPN, a local `127.0.0.1:2080` proxy for apps where it is set.
- Updates from the app: the new version downloads from GitHub and installs through the system
  installer.
- A core log with copying - useful if something does not work.
