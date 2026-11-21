#!/bin/sh
set -eu

# libzip is optional: build.rs enables --with-zip only when pkg-config finds it,
# and the package is missing from some base repositories.
optional() {
    "$@" >/dev/null 2>&1 || true
}

if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    apt-get install -y build-essential make autoconf pkg-config git curl \
        libssl-dev libxml2-dev libcurl4-openssl-dev libsqlite3-dev \
        zlib1g-dev libreadline-dev libbz2-dev libpng-dev libjpeg-dev \
        libwebp-dev libicu-dev
    optional apt-get install -y libzip-dev
elif command -v dnf >/dev/null 2>&1; then
    # EPEL carries libzip-devel on RHEL 8 and 9; it is absent on Amazon Linux 2023.
    optional dnf install -y epel-release
    dnf install -y gcc gcc-c++ make autoconf pkgconf-pkg-config git curl \
        openssl-devel libxml2-devel libcurl-devel sqlite-devel \
        zlib-devel readline-devel bzip2-devel libpng-devel \
        libjpeg-turbo-devel libwebp-devel libicu-devel
    optional dnf install -y libzip-devel
elif command -v microdnf >/dev/null 2>&1; then
    optional microdnf install -y epel-release
    microdnf install -y gcc gcc-c++ make autoconf pkgconf-pkg-config git curl \
        openssl-devel libxml2-devel libcurl-devel sqlite-devel \
        zlib-devel readline-devel bzip2-devel libpng-devel \
        libjpeg-turbo-devel libwebp-devel libicu-devel
    optional microdnf install -y libzip-devel
elif command -v yum >/dev/null 2>&1; then
    optional yum install -y epel-release
    yum install -y gcc gcc-c++ make autoconf pkgconfig git curl \
        openssl-devel libxml2-devel libcurl-devel sqlite-devel \
        zlib-devel readline-devel bzip2-devel libpng-devel \
        libjpeg-turbo-devel libwebp-devel libicu-devel
    optional yum install -y libzip-devel
elif command -v apk >/dev/null 2>&1; then
    apk add --no-cache build-base make autoconf pkgconf git curl \
        openssl-dev libxml2-dev curl-dev sqlite-dev zlib-dev \
        readline-dev bzip2-dev libpng-dev libjpeg-turbo-dev libwebp-dev icu-dev
    optional apk add --no-cache libzip-dev
elif command -v pacman >/dev/null 2>&1; then
    pacman -Sy --noconfirm --needed base-devel autoconf pkgconf git curl \
        openssl libxml2 curl sqlite zlib readline bzip2 libpng \
        libjpeg-turbo libwebp icu
    optional pacman -S --noconfirm --needed libzip
else
    echo "Error: no supported package manager found (apt-get, dnf, microdnf, yum, apk, pacman)." >&2
    exit 1
fi
