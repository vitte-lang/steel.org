#!/usr/bin/env bash
set -euo pipefail

APP_NAME="steel"
ARCH="$(dpkg --print-architecture)"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

VERSION="$(
  awk -F' *= *' '
    /^\[package\]/ { in_pkg=1; next }
    /^\[/ { in_pkg=0 }
    in_pkg && /^version *=/ {
      gsub(/"/, "", $2);
      print $2;
      exit
    }
  ' "${ROOT_DIR}/Cargo.toml"
)"

if [[ -z "${VERSION}" ]]; then
  echo "error: could not read version from Cargo.toml" >&2
  exit 1
fi

DIST_DIR="${ROOT_DIR}/dist"
PKG_DIR="${DIST_DIR}/deb"
DEBIAN_DIR="${PKG_DIR}/DEBIAN"
BIN_PATH="${ROOT_DIR}/target/release/steel"
MAN_PATH="${ROOT_DIR}/doc/steel.1"

rm -rf "${PKG_DIR}"
mkdir -p "${DEBIAN_DIR}" "${PKG_DIR}/usr/bin" "${PKG_DIR}/usr/share/man/man1"

(
  cd "${ROOT_DIR}"
  cargo build --release
)

if [[ ! -f "${BIN_PATH}" ]]; then
  echo "error: missing binary at ${BIN_PATH}" >&2
  exit 1
fi

if [[ ! -f "${MAN_PATH}" ]]; then
  echo "error: missing man page at ${MAN_PATH}" >&2
  exit 1
fi

install -m 0755 "${BIN_PATH}" "${PKG_DIR}/usr/bin/steel"
install -m 0644 "${MAN_PATH}" "${PKG_DIR}/usr/share/man/man1/steel.1"

cat > "${DEBIAN_DIR}/control" <<EOF
Package: ${APP_NAME}
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: Vitte Team <dev@vitte-lang.org>
Description: Steel - declarative configuration layer for Vitte build system
EOF

dpkg-deb --build "${PKG_DIR}" "${DIST_DIR}/${APP_NAME}_${VERSION}_${ARCH}.deb"

echo "OK: ${DIST_DIR}/${APP_NAME}_${VERSION}_${ARCH}.deb"
