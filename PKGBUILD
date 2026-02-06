# Maintainer: ShawnTheShadow <shawn@stsg.io>
pkgname=air1-monitor-git
pkgver=0.1.4.r111.g2c3a4e9
pkgrel=1
pkgdesc="Air1 Monitor - MQTT monitoring application (Git version)"
arch=('x86_64')
url="https://github.com/ShawnTheShadow/Air1-Monitor"
license=('MIT')
depends=('gcc-libs' 'glibc')
makedepends=('cargo' 'git')
provides=("${pkgname%-git}")
conflicts=("${pkgname%-git}")
source=("git+$url.git"
        "air1-monitor.desktop"
        "Air1MQTT.png")
sha256sums=('SKIP'
            '44e0573ba7f45166407072e5b3fa46f0d4c7883a26d7c87101a1158985162cff'
            '1775cc1c6b83df1291a65e906faf09d2f7113ed052381e911ce70fa72971c110')

pkgver() {
  cd Air1-Monitor
  printf "0.1.4.r%s.g%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

prepare() {
  cd Air1-Monitor
  
  # Create vendor directory
  export CARGO_HOME="$srcdir/cargo-home"
  mkdir -p .cargo
  cargo vendor > .cargo/config.toml
}

build() {
  cd Air1-Monitor
  export CARGO_HOME="$srcdir/cargo-home"
  # --offline ensures we only use vendored crates
  cargo build --frozen --release --offline
}

check() {
  cd Air1-Monitor
  export CARGO_HOME="$srcdir/cargo-home"
  export SKIP_KEYRING_TESTS=1
  cargo test --frozen --offline
}

package() {
  cd Air1-Monitor
  install -Dm755 "target/release/${pkgname%-git}" "$pkgdir/usr/bin/${pkgname%-git}"
  install -Dm644 "../air1-monitor.desktop" "$pkgdir/usr/share/applications/${pkgname%-git}.desktop"
  install -Dm644 "../Air1MQTT.png" "$pkgdir/usr/share/pixmaps/${pkgname%-git}.png"
}
