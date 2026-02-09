#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Steel"
BIN_NAME="steel"
VERSION="2.2026"
IDENT="io.vitte-lang.steel"

MODE="${1:-universal2}"          # x86_64 | universal2
MIN_OS="${MIN_OS:-}"             # ex: 26.0 (optionnel). Laisse vide = pas de contrainte.

# --- 0) Pré-requis targets ---
if [[ "${MODE}" == "x86_64" ]]; then
  rustup target add x86_64-apple-darwin >/dev/null
elif [[ "${MODE}" == "universal2" ]]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
else
  echo "Usage: $0 [x86_64|universal2]"
  exit 2
fi

# --- 1) Build ---
rm -rf dist
mkdir -p dist/bin dist/pkgroot/usr/local/bin dist/tmp

if [[ "${MODE}" == "x86_64" ]]; then
  echo "[1/4] Build Rust release (x86_64) ..."
  cargo +stable build --release --target x86_64-apple-darwin
  cp "target/x86_64-apple-darwin/release/${BIN_NAME}" "dist/bin/${BIN_NAME}"
  cp "target/x86_64-apple-darwin/release/steecleditor" "dist/bin/steecleditor"
  FINAL_PKG="dist/${APP_NAME}-${VERSION}-x86_64.pkg"
else
  echo "[1/4] Build Rust release (aarch64 + x86_64) ..."
  cargo +stable build --release --target aarch64-apple-darwin
  cargo +stable build --release --target x86_64-apple-darwin

  echo "[2/4] lipo universal2 ..."
  lipo -create \
    "target/aarch64-apple-darwin/release/${BIN_NAME}" \
    "target/x86_64-apple-darwin/release/${BIN_NAME}" \
    -output "dist/bin/${BIN_NAME}"

  lipo -create \
    "target/aarch64-apple-darwin/release/steecleditor" \
    "target/x86_64-apple-darwin/release/steecleditor" \
    -output "dist/bin/steecleditor"

  FINAL_PKG="dist/${APP_NAME}-${VERSION}-universal2.pkg"
fi

# --- 2) Payload (install direct dans /usr/local/bin) ---
echo "[3/4] Préparation pkgroot ..."
install -m 0755 "dist/bin/${BIN_NAME}" "dist/pkgroot/usr/local/bin/${BIN_NAME}"
install -m 0755 "dist/bin/steecleditor" "dist/pkgroot/usr/local/bin/steecleditor"

# --- 3) pkgbuild (component) ---
echo "[4/4] pkgbuild + productbuild (unsigned) ..."
pkgbuild \
  --root "dist/pkgroot" \
  --install-location / \
  --identifier "${IDENT}" \
  --version "${VERSION}" \
  "dist/${APP_NAME}.component.pkg"

# --- productbuild final ---
if [[ -n "${MIN_OS}" ]]; then
  # Distribution.xml minimal avec contrainte OS
  DIST_XML="dist/tmp/Distribution.xml"
  cat > "${DIST_XML}" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="1">
  <title>${APP_NAME}</title>

  <allowed-os-versions>
    <os-version min="${MIN_OS}"/>
  </allowed-os-versions>

  <options customize="never" require-scripts="false"/>
  <domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true"/>

  <pkg-ref id="${IDENT}"/>

  <choices-outline>
    <line choice="default"/>
  </choices-outline>

  <choice id="default" visible="false" title="${APP_NAME}">
    <pkg-ref id="${IDENT}"/>
  </choice>

  <pkg-ref id="${IDENT}" version="${VERSION}" onConclusion="none">${APP_NAME}.component.pkg</pkg-ref>
</installer-gui-script>
EOF

  productbuild \
    --distribution "${DIST_XML}" \
    --package-path dist \
    "${FINAL_PKG}"
else
  productbuild \
    --package "dist/${APP_NAME}.component.pkg" \
    "${FINAL_PKG}"
fi

echo
echo "OK: ${FINAL_PKG}"
echo "Test: sudo installer -pkg \"${FINAL_PKG}\" -target / && ${BIN_NAME} --version"
echo "Gatekeeper check (optionnel): spctl --assess --type install -vv \"${FINAL_PKG}\""
