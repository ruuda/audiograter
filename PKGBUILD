# Maintainer: Ruud van Asseldonk <dev@veniogames.com>
pkgname=audiograter
pkgver=1.0
pkgrel=1
pkgdesc="GTK-based spectrogram viewer"
arch=('x86_64')
url='https://github.com/ruuda/audiograter'
license=('GPL3')
depends=('gtk3')
makedepends=('git' 'rustup')
validpgpkeys=('28EEB492BE15FF2DC93BCE865F231E540599697D')
source=("git+https://github.com/ruuda/audiograter#tag=v${pkgver}?signed")
sha256sums=('SKIP')

build() {
  cd "$srcdir/$pkgname"
  cargo build --release
}

check() {
  cd "$srcdir/$pkgname"
  cargo test
}

package() {
  mkdir -p "$pkgdir/usr/bin"
  cp "$srcdir/$pkgname/target/release/audiograter" "$pkgdir/usr/bin/"
}
