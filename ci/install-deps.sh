#!/bin/sh
set -eu

# libzip is optional: build.rs enables --with-zip only when pkg-config finds it,
# and the package is missing from some base repositories.
optional() {
    "$@" >/dev/null 2>&1 || true
}

# oniguruma-devel and libzip-devel live in EPEL on RHEL 8 and 9, and EPEL needs
# CodeReady Builder (crb) or PowerTools enabled first. Amazon Linux has no EPEL,
# so every step stays best-effort and the install below reports what is missing.
enable_epel() {
    major="$(. /etc/os-release && printf '%s' "${VERSION_ID%%.*}")"
    optional dnf install -y dnf-plugins-core
    optional yum install -y yum-utils
    case "$major" in
        8) optional dnf config-manager --set-enabled powertools ;;
        9 | 10) optional dnf config-manager --set-enabled crb ;;
    esac
    optional dnf config-manager --set-enabled "ol${major}_codeready_builder"
    optional dnf config-manager --set-enabled "ol${major}_developer_EPEL"
    optional dnf install -y epel-release
    optional dnf install -y "https://dl.fedoraproject.org/pub/epel/epel-release-latest-${major}.noarch.rpm"
    optional dnf makecache
}

# curl is deliberately absent from the lists below: the RHEL family images ship
# curl-minimal, which conflicts with the curl package.
if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    apt-get install -y build-essential make autoconf pkg-config git curl \
        libssl-dev libxml2-dev libcurl4-openssl-dev libsqlite3-dev \
        zlib1g-dev libonig-dev libreadline-dev libbz2-dev libpng-dev libjpeg-dev \
        libwebp-dev libicu-dev
    optional apt-get install -y libzip-dev
elif command -v dnf >/dev/null 2>&1; then
    enable_epel
    dnf install -y gcc gcc-c++ make autoconf pkgconf-pkg-config git \
        openssl-devel libxml2-devel libcurl-devel sqlite-devel \
        zlib-devel oniguruma-devel readline-devel bzip2-devel libpng-devel \
        libjpeg-turbo-devel libwebp-devel libicu-devel
    optional dnf install -y libzip-devel
elif command -v microdnf >/dev/null 2>&1; then
    enable_epel
    optional microdnf install -y epel-release
    microdnf install -y gcc gcc-c++ make autoconf pkgconf-pkg-config git \
        openssl-devel libxml2-devel libcurl-devel sqlite-devel \
        zlib-devel oniguruma-devel readline-devel bzip2-devel libpng-devel \
        libjpeg-turbo-devel libwebp-devel libicu-devel
    optional microdnf install -y libzip-devel
elif command -v yum >/dev/null 2>&1; then
    enable_epel
    yum install -y gcc gcc-c++ make autoconf pkgconfig git \
        openssl-devel libxml2-devel libcurl-devel sqlite-devel \
        zlib-devel oniguruma-devel readline-devel bzip2-devel libpng-devel \
        libjpeg-turbo-devel libwebp-devel libicu-devel
    optional yum install -y libzip-devel
elif command -v apk >/dev/null 2>&1; then
    apk add --no-cache build-base make autoconf pkgconf git curl \
        openssl-dev libxml2-dev curl-dev sqlite-dev zlib-dev oniguruma-dev \
        readline-dev bzip2-dev libpng-dev libjpeg-turbo-dev libwebp-dev icu-dev
    optional apk add --no-cache libzip-dev
elif command -v pacman >/dev/null 2>&1; then
    pacman -Sy --noconfirm --needed base-devel autoconf pkgconf git curl \
        openssl libxml2 curl sqlite zlib oniguruma readline bzip2 libpng \
        libjpeg-turbo libwebp icu
    optional pacman -S --noconfirm --needed libzip
else
    echo "Error: no supported package manager found (apt-get, dnf, microdnf, yum, apk, pacman)." >&2
    exit 1
fi
