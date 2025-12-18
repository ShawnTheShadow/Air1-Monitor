# Maintainer: ShawnTheShadow <shawn@stsg.io>
pkgname=air1-monitor
pkgver=0.1.5
pkgrel=1
pkgdesc="Air1 Monitor - MQTT monitoring application"
arch=('x86_64')
url="https://github.com/ShawnTheShadow/Air1-Monitor"
license=('MIT')
depends=('gcc-libs')
makedepends=('cargo' 'rust')
source=()
sha256sums=()

pkgver() {
    cd "${startdir:-.}"
    printf "0.1.4.r%s" "$(git rev-list --count HEAD)"
}

prepare() {
    cd "$startdir"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$startdir"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    export RUSTFLAGS="-C strip=none"
    
    cargo build --release
}

check() {
    cd "$startdir"
    export RUSTUP_TOOLCHAIN=stable
    cargo test
}

package() {
    cd "$startdir"
    install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
    install -Dm644 "air1-monitor.desktop" "$pkgdir/usr/share/applications/$pkgname.desktop"
    install -Dm644 "Air1MQTT.png" "$pkgdir/usr/share/pixmaps/$pkgname.png"
}
