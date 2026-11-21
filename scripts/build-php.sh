#!/usr/bin/env bash
set -Eeuo pipefail

: "${PHP_MINOR:?Set PHP_MINOR to 8.1 through 8.6}"
: "${TARGET_ARCH:?Set TARGET_ARCH}"
: "${BUILD_ROOT:?Set BUILD_ROOT}"
script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(CDPATH= cd -- "$script_dir/.." && pwd)"
[[ "$PHP_MINOR" =~ ^8\.[1-6]$ ]] || { echo "Unsupported PHP minor: $PHP_MINOR" >&2; exit 2; }
[[ "$TARGET_ARCH" =~ ^(x86_64|arm64)$ ]] || { echo "Unsupported architecture: $TARGET_ARCH" >&2; exit 2; }

mkdir -p "$BUILD_ROOT"/{src,logs,dist}
work="$BUILD_ROOT/src/php-$PHP_MINOR"
mkdir -p "$work"
log="$BUILD_ROOT/logs/build.log"
exec > >(tee -a "$log") 2>&1

python3 "$script_dir/resolve-php-source.py" "$PHP_MINOR" "$work/source.json"
source_url="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["url"])' "$work/source.json")"
source_sha="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' "$work/source.json")"
archive="$work/php.tar.xz"
curl --fail --location --retry 3 --output "$archive" "$source_url"
python3 - "$archive" "$source_sha" <<'PY'
import hashlib, pathlib, sys
actual = hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest()
if actual != sys.argv[2]:
    raise SystemExit(f"SHA256 mismatch: expected {sys.argv[2]}, got {actual}")
PY
tar -xJf "$archive" -C "$work" --strip-components=1

prefix="$BUILD_ROOT/install"
mkdir -p "$prefix"
if [[ "$RUNNER_OS" == "macOS" ]]; then
  brew_prefix="$(brew --prefix)"
  pkg_config_path=""
  cppflags=""
  ldflags=""
  for formula in openssl@3 libxml2 curl sqlite3 zlib oniguruma readline libzip bzip2 libpng jpeg-turbo webp icu4c; do
    dep_prefix="$(brew --prefix "$formula")"
    pkg_config_path="$pkg_config_path:$dep_prefix/lib/pkgconfig:$dep_prefix/share/pkgconfig"
    cppflags="$cppflags -I$dep_prefix/include"
    ldflags="$ldflags -L$dep_prefix/lib"
  done
  export PKG_CONFIG_PATH="${pkg_config_path#:}${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  export CPPFLAGS="$cppflags ${CPPFLAGS:-}"
  export LDFLAGS="$ldflags ${LDFLAGS:-}"
  export PATH="$brew_prefix/bin:$PATH"
fi

cd "$work"
configure_options=(
  "--prefix=$prefix"
  "--with-config-file-path=$prefix/lib"
  "--with-config-file-scan-dir=$prefix/etc/conf.d"
  --enable-cli --disable-cgi --enable-phar
  --with-openssl --with-curl --with-zlib --with-zip --with-bz2
  --with-readline --with-pdo-sqlite --with-sqlite3
  --enable-mbstring --enable-intl
  --enable-session --enable-filter --enable-ctype --enable-tokenizer
  --enable-fileinfo --enable-dom --enable-xml --enable-xmlwriter
  --enable-simplexml --enable-opcache
)
./configure "${configure_options[@]}" 2>&1 | tee "$BUILD_ROOT/logs/configure.log"
make -j"$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu)" 2>&1 | tee "$BUILD_ROOT/logs/make.log"
make install 2>&1 | tee "$BUILD_ROOT/logs/install.log"

cp LICENSE "$prefix/LICENSE"
mkdir -p "$prefix/etc/conf.d"
cp php.ini-production "$prefix/lib/php.ini"
cp php.ini-production "$prefix/lib/php.ini-production"
cp php.ini-development "$prefix/lib/php.ini-development"

python3 "$repo_dir/scripts/relocate-sdk.py" "$prefix"
mv "$prefix" "$BUILD_ROOT/relocated"
rm -rf "$prefix"
mkdir -p "$BUILD_ROOT/package"
tar -C "$BUILD_ROOT" -czf "$BUILD_ROOT/dist/php-$PHP_MINOR-$RUNNER_OS-$TARGET_ARCH.tar.gz" relocated
python3 - "$BUILD_ROOT" "$PHP_MINOR" "$TARGET_ARCH" "${configure_options[*]}" <<'PY'
import hashlib, json, pathlib, platform, sys
root = pathlib.Path(sys.argv[1])
minor, arch, options = sys.argv[2:]
archive = next((root / "dist").glob("*.tar.gz"))
manifest = {
    "php_minor": minor,
    "source_version": json.loads((root / "src" / f"php-{minor}" / "source.json").read_text())["version"],
    "source_sha256": json.loads((root / "src" / f"php-{minor}" / "source.json").read_text())["sha256"],
    "platform": platform.system().lower(),
    "architecture": arch,
    "configure_options": options.split(),
    "runtime_dependencies": "System-linked OpenSSL 3, curl, XML, SQLite, zlib, and other build-host libraries; not self-contained.",
    "archive": archive.name,
    "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
}
(root / "dist" / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
(root / "dist" / "SHA256SUMS").write_text(f'{manifest["archive_sha256"]}  {archive.name}\n')
PY
mkdir -p "$BUILD_ROOT/package"
tar -xzf "$BUILD_ROOT/dist/"*.tar.gz -C "$BUILD_ROOT/package"
