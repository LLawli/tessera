#!/bin/bash
# Tessera's p11-kit (the manifest's p11-kit module: the release plus the
# patches in flatpak/patches) against this distribution's own p11-kit, on the
# paths apps sign by. Run as root in a Debian or Ubuntu container whose
# p11-kit is 0.25.5 or older (Debian 13, Ubuntu 24.04), from the repository's
# root:  bash tests/p11kit-compat/run.sh
#
# A SoftHSM token is served the way Tessera serves a driver (a p11-kit server
# with --provider), and signed with RSA-PSS (sign-test.py, as Firefox does)
# and RSA-PKCS (pkcs11-tool):
#   native app   the system's p11-kit-proxy.so, reading a remote: line
#   Flatpak app  a client of the plain release (as the runtimes have)
#                through the user's server, which is Tessera's p11-kit
#   older app    the system's p11-kit-client.so through that same server
# and, so that the check can fail, the plain release on Tessera's side, which
# must fail on the native path. Exits non-zero on any unexpected result.
set -euo pipefail
cd "$(dirname "$0")/../.."
REPO=$PWD
MANIFEST=flatpak/io.github.pedrohqb.Tessera.yml

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null
apt-get install -y -qq meson ninja-build gcc pkg-config libtasn1-6-dev libffi-dev \
  python3 curl ca-certificates xz-utils patch \
  p11-kit p11-kit-modules softhsm2 opensc openssl >/dev/null

# The release and its sha256, from the manifest's p11-kit module.
url=$(grep -A3 'type: archive' $MANIFEST | grep -o 'https://github.com/p11-glue/p11-kit/releases/download/[^ ]*')
sha=$(grep -A4 "$url" $MANIFEST | sed -n 's/.*sha256: *//p' | head -1)
patches=$(sed -n '/^  - name: p11-kit$/,/^  - name: /p' $MANIFEST | sed -n 's/^ *- \(patches\/.*\.patch\)$/flatpak\/\1/p')
[ -n "$url" ] && [ -n "$sha" ] && [ -n "$patches" ] || { echo "could not read the p11-kit module from $MANIFEST"; exit 1; }
W=$(mktemp -d)
curl -fsSL "$url" -o "$W/p11-kit.tar.xz"
echo "$sha  $W/p11-kit.tar.xz" | sha256sum -c --quiet

build() { # <name> <apply patches: yes|no>
  local d=$W/src-$1
  mkdir -p "$d" && tar -xf "$W/p11-kit.tar.xz" -C "$d" --strip-components=1
  if [ "$2" = yes ]; then
    for p in $patches; do patch -d "$d" -p1 -s < "$REPO/$p"; done
  fi
  # The manifest's options; the client module goes to the prefix.
  LDFLAGS='-Wl,-rpath,$ORIGIN' meson setup "$d/_b" "$d" --prefix=/opt/$1 --libdir=lib \
    -Dsysconfdir=/etc -Dsystem_config=/etc/pkcs11 -Dmodule_config=/usr/share/p11-kit/modules \
    -Dmodule_path=/opt/$1/lib/pkcs11 -Dtrust_module=disabled -Dlibffi=disabled \
    -Dsystemd=disabled -Dbash_completion=disabled -Dzsh_completion=disabled \
    -Dgtk_doc=false -Dman=false -Dnls=false -Dtest=false >/dev/null
  ninja -C "$d/_b" >/dev/null
  ninja -C "$d/_b" install >/dev/null
}
build plain no
build tessera yes

# Only the token served "as Tessera" may be visible.
rm -f /usr/share/p11-kit/modules/softhsm2.module /usr/share/p11-kit/modules/opensc*.module
ARCH=/usr/lib/x86_64-linux-gnu
SOFTHSM=$(ls /usr/lib/softhsm/libsofthsm2.so $ARCH/softhsm/libsofthsm2.so 2>/dev/null | head -1)
mkdir -p /var/lib/softhsm/tokens /etc/pkcs11/modules
softhsm2-util --init-token --free --label tessera --pin 1234 --so-pin 4321 >/dev/null
openssl genrsa -out "$W/k.pem" 2048 2>/dev/null
openssl pkcs8 -topk8 -nocrypt -in "$W/k.pem" -out "$W/k8.pem"
softhsm2-util --import "$W/k8.pem" --token tessera --label k --id 01 --pin 1234 >/dev/null
head -c 32 /dev/urandom > "$W/data"
printf 'remote: unix:path=%s/tessera\n' "$W" > /etc/pkcs11/modules/tessera.module

signs() { # <module>: both mechanisms sign
  PIN=1234 timeout 30 python3 scripts/sign-test.py "$1" tessera fixed 2>&1 | grep -q "valid RSA-PSS" &&
  timeout 30 pkcs11-tool --module "$1" --token-label tessera --login --pin 1234 --sign \
    -m RSA-PKCS -i "$W/data" -o "$W/sig" >/dev/null 2>&1 &&
  openssl pkeyutl -verify -inkey "$W/k.pem" -in "$W/data" -sigfile "$W/sig" >/dev/null 2>&1
}
signs_via() ( # <socket> <client module>: both sign through that server
  export P11_KIT_SERVER_ADDRESS=unix:path=$1
  signs "$2"
)
PIDS=()
serve() { # <prefix> <socket> [provider]
  rm -f "$2"
  LD_LIBRARY_PATH=/opt/$1/lib P11_KIT_PRIVATEDIR=/opt/$1/libexec/p11-kit \
    /opt/$1/bin/p11-kit server -f -n "$2" ${3:+--provider "$3"} "pkcs11:" >/dev/null 2>&1 &
  PIDS+=($!)
  for _ in $(seq 50); do [ -S "$2" ] && return; sleep 0.1; done
  echo "server at $2 did not start"; exit 1
}
stopall() { for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null || true; done; PIDS=(); sleep 0.5; }
fails=0
expect() { # <ok|fail> <what> <command...>
  local want=$1 what=$2; shift 2
  if "$@"; then got=ok; else got=fail; fi
  if [ "$got" = "$want" ]; then echo "  $what: $got (as expected)"; else echo "  $what: $got, expected $want"; fails=$((fails + 1)); fi
}
. /etc/os-release
echo "== $PRETTY_NAME, system p11-kit $(dpkg-query -W -f='${Version}' libp11-kit0)"

echo "Tessera serving with the plain release (the check must be able to fail):"
serve plain "$W/tessera" "$SOFTHSM"
expect fail "native app" signs $ARCH/p11-kit-proxy.so
stopall

echo "Tessera serving with its p11-kit:"
serve tessera "$W/tessera" "$SOFTHSM"
expect ok "native app" signs $ARCH/p11-kit-proxy.so
serve tessera "$W/hub"
expect ok "Flatpak app through Tessera's p11-kit as the user's server" \
  signs_via "$W/hub" /opt/plain/lib/pkcs11/p11-kit-client.so
expect ok "older app through that server" \
  signs_via "$W/hub" $ARCH/pkcs11/p11-kit-client.so
stopall

[ "$fails" -eq 0 ] || { echo "$fails unexpected result(s)"; exit 1; }
echo "all as expected"
