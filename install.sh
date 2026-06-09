#!/bin/sh
# Gnit installer - curl -sSL https://raw.githubusercontent.com/mostlydev/gnit/master/install.sh | sh
set -eu

REPO="mostlydev/gnit"
INSTALL_DIR="${GNIT_INSTALL_DIR:-$HOME/.local/bin}"

info() { printf '  %s\n' "$@"; }
err()  { printf 'Error: %s\n' "$@" >&2; exit 1; }

OS="$(uname -s)"
case "$OS" in
  Linux*)  OS="linux" ;;
  Darwin*) OS="darwin" ;;
  *)       err "Unsupported OS: $OS" ;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) ARCH="x86_64" ;;
  aarch64|arm64)
    if [ "$OS" = "darwin" ]; then
      ARCH="aarch64"
    else
      err "Unsupported architecture until a linux/aarch64 release is published: $ARCH"
    fi
    ;;
  *) err "Unsupported architecture: $ARCH" ;;
esac

info "Detected platform: ${OS}/${ARCH}"
info "Fetching latest release..."

TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')"

if [ -z "$TAG" ]; then
  err "Could not determine latest release tag"
fi

VERSION="${TAG#v}"
TARBALL="gnit-${VERSION}-${OS}-${ARCH}.tar.gz"
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

info "Downloading ${TARBALL}..."
curl -fsSL "${BASE_URL}/${TARBALL}" -o "${TMPDIR}/${TARBALL}"

info "Downloading checksums..."
curl -fsSL "${BASE_URL}/checksums.txt" -o "${TMPDIR}/checksums.txt"

info "Verifying checksum..."
EXPECTED="$(grep "${TARBALL}" "${TMPDIR}/checksums.txt" | awk '{print $1}')"
if [ -z "$EXPECTED" ]; then
  err "Tarball ${TARBALL} not found in checksums.txt"
fi

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "${TMPDIR}/${TARBALL}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "${TMPDIR}/${TARBALL}" | awk '{print $1}')"
else
  err "No sha256sum or shasum found; cannot verify integrity"
fi

if [ "$EXPECTED" != "$ACTUAL" ]; then
  err "Checksum mismatch"
fi

mkdir -p "$INSTALL_DIR"
tar -xzf "${TMPDIR}/${TARBALL}" -C "$TMPDIR"
mv "${TMPDIR}/gnit" "${INSTALL_DIR}/gnit"
chmod +x "${INSTALL_DIR}/gnit"

info "Installed gnit to ${INSTALL_DIR}/gnit"

# Clean up a pre-rename install (this tool shipped as "nit" through v0.8.2).
# Other projects also ship a binary named "nit", so only remove artifacts we
# can verify are ours: the binary by its exact version-line format, skill
# links by resolving into our legacy data directory.
LEGACY_BIN="${INSTALL_DIR}/nit"
if [ -x "$LEGACY_BIN" ] && [ ! -d "$LEGACY_BIN" ]; then
  LEGACY_VERSION="$("$LEGACY_BIN" --version 2>/dev/null || true)"
  if printf '%s' "$LEGACY_VERSION" | grep -qE '^nit 0\.[0-9]+\.[0-9]+$'; then
    rm -f "$LEGACY_BIN"
    info "Removed legacy binary ${LEGACY_BIN} (${LEGACY_VERSION})"
  fi
fi

LEGACY_DATA="${XDG_DATA_HOME:-$HOME/.local/share}/nit"
if [ -f "${LEGACY_DATA}/skills/nit/SKILL.md" ]; then
  for LEGACY_LINK in "$HOME/.claude/skills/nit" "$HOME/.codex/skills/nit" \
    "$HOME/.opencode/skills/nit" "$HOME/.grok/skills/nit"; do
    if [ -L "$LEGACY_LINK" ]; then
      case "$(readlink "$LEGACY_LINK")" in
        "${LEGACY_DATA}"/*)
          rm -f "$LEGACY_LINK"
          info "Removed legacy skill link ${LEGACY_LINK}"
          ;;
      esac
    fi
  done
  rm -rf "$LEGACY_DATA"
  info "Removed legacy skill data ${LEGACY_DATA}"
  info "Run 'gnit skills install --all' to reinstall agent skills under the new name."
fi

case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo ""
    info "${INSTALL_DIR} is not in your PATH."
    info "Add it with:"
    info "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    ;;
esac

echo ""
info "Run 'gnit doctor' to verify your installation."
