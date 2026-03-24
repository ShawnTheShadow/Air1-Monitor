# Maintainer: ShawnTheShadow <shawn@stsg.io>
pkgname=air1-monitor-git
pkgver=0.1.6.r131.g2e80c6d
pkgrel=1
pkgdesc="Air1 Monitor - MQTT monitoring application (Git version)"
arch=('x86_64')
url="https://github.com/ShawnTheShadow/Air1-Monitor"
license=('MIT')
depends=('dbus' 'gcc-libs' 'glibc' 'gtk4')
makedepends=('cargo' 'cmake' 'git' 'pkgconf')
provides=("${pkgname%-git}")
conflicts=("${pkgname%-git}")
source=("git+$url.git"
        "air1-monitor.desktop"
        "Air1MQTT.png")
sha256sums=('SKIP'
            '44e0573ba7f45166407072e5b3fa46f0d4c7883a26d7c87101a1158985162cff'
            '1775cc1c6b83df1291a65e906faf09d2f7113ed052381e911ce70fa72971c110')

# Set AIR1_MONITOR_LOCAL_DEV=1 to package from the current working tree
# (including uncommitted changes) instead of the git source defined above.
_repo_dir() {
  if [[ "${AIR1_MONITOR_LOCAL_DEV:-0}" == "1" ]]; then
    printf '%s\n' "$srcdir/Air1-Monitor-local"
  else
    printf '%s\n' "$srcdir/Air1-Monitor"
  fi
}

_asset_dir() {
  if [[ "${AIR1_MONITOR_LOCAL_DEV:-0}" == "1" ]]; then
    printf '%s\n' "$srcdir/Air1-Monitor-local"
  else
    printf '%s\n' "$srcdir"
  fi
}

pkgver() {
  cd "$(_repo_dir)"
  local base_ver
  base_ver="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
  printf "%s.r%s.g%s" "$base_ver" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

prepare() {
  if [[ "${AIR1_MONITOR_LOCAL_DEV:-0}" == "1" ]]; then
    rm -rf "$srcdir/Air1-Monitor-local"
    mkdir -p "$srcdir/Air1-Monitor-local"
    tar \
      --exclude=.git \
      --exclude=.makepkg-build \
      --exclude=vendor \
      --exclude=target \
      -C "$startdir" -cf - . | tar -C "$srcdir/Air1-Monitor-local" -xf -
  fi

  cd "$(_repo_dir)"
  
  # Create vendor directory
  export CARGO_HOME="$srcdir/cargo-home"
  mkdir -p .cargo
  cargo vendor > .cargo/config.toml
}

build() {
  cd "$(_repo_dir)"
  export CARGO_HOME="$srcdir/cargo-home"
  # --offline ensures we only use vendored crates
  cargo build --frozen --release --offline
}

check() {
  cd "$(_repo_dir)"
  export CARGO_HOME="$srcdir/cargo-home"
  export SKIP_KEYRING_TESTS=1
  cargo test --frozen --offline
}

package() {
  cd "$(_repo_dir)"
  local assets
  assets="$(_asset_dir)"
  install -Dm755 "target/release/${pkgname%-git}" "$pkgdir/usr/bin/${pkgname%-git}"
  install -Dm644 "$assets/air1-monitor.desktop" "$pkgdir/usr/share/applications/${pkgname%-git}.desktop"
  install -Dm644 "$assets/Air1MQTT.png" "$pkgdir/usr/share/pixmaps/${pkgname%-git}.png"
}
