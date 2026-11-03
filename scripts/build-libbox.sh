#!/usr/bin/env bash
# Собирает ядро sing-box как Android-библиотеку libbox.aar через gomobile.
# Работает в Git Bash на Windows и в Linux (GitHub Actions).
#
#   scripts/build-libbox.sh            - все архитектуры (arm64, arm, x86_64, x86)
#   ARCHS=arm64 scripts/build-libbox.sh - только arm64, быстрее для отладки на телефоне
#
# Нужны: Go, ANDROID_HOME и NDK_HOME (или ANDROID_NDK_HOME), JDK в JAVA_HOME.
set -euo pipefail

# Та же версия ядра, что и в десктопном клиенте.
SING_BOX_VERSION="v1.14.4"
ARCHS="${ARCHS:-arm64,arm,amd64,386}"

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/build/sing-box"
out_dir="$root/src-tauri/gen/android/app/libs"

if [ ! -d "$src/.git" ]; then
  git -c advice.detachedHead=false clone --depth 1 --branch "$SING_BOX_VERSION" \
    https://github.com/SagerNet/sing-box.git "$src"
elif [ "$(git -C "$src" describe --tags)" != "$SING_BOX_VERSION" ]; then
  git -C "$src" fetch --depth 1 origin tag "$SING_BOX_VERSION"
  git -C "$src" -c advice.detachedHead=false checkout "$SING_BOX_VERSION"
fi

cd "$src"

# gomobile - форк SagerNet, версия из go.mod sing-box.
gomobile_version="$(go list -m -f '{{.Version}}' github.com/sagernet/gomobile)"
go install "github.com/sagernet/gomobile/cmd/gomobile@$gomobile_version"
go install "github.com/sagernet/gomobile/cmd/gobind@$gomobile_version"
gobin="$(go env GOPATH)/bin"

export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-${NDK_HOME:?задайте NDK_HOME}}"

# Только то, что умеет клиент: vless (Reality, utls), hy2 (quic), trojan, ss,
# TUN через gvisor и Clash API для счётчика трафика. Без Tailscale, NaiveProxy,
# OpenVPN и прочего - библиотека выходит в разы меньше.
tags="with_gvisor,with_quic,with_utls,with_clash_api,badlinkname,tfogo_checklinkname0"
ldflags="-X github.com/sagernet/sing-box/constant.Version=${SING_BOX_VERSION#v} -X runtime.godebugDefault=multipathtcp=0,tlssha1=1 -checklinkname=0 -s -w -buildid="

targets="$(echo "$ARCHS" | sed 's/\([^,]*\)/android\/\1/g')"

mkdir -p "$out_dir"
"$gobin/gomobile" bind -v \
  -o "$out_dir/libbox.aar" \
  -target "$targets" \
  -androidapi 24 \
  -javapkg=io.nekohasekai \
  -libname=box \
  -trimpath -buildvcs=false \
  -ldflags "$ldflags" \
  -tags "$tags" \
  ./experimental/libbox

ls -l "$out_dir/libbox.aar"
