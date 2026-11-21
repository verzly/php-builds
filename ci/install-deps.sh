#!/bin/sh
set -eu

if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    apt-get install -y build-essential make autoconf pkg-config git curl \
        libssl-dev libxml2-dev libcurl4-openssl-dev libsqlite3-dev \
        zlib1g-dev libonig-dev libreadline-dev libzip-dev libbz2-dev \
        libpng-dev libjpeg-dev libwebp-dev libicu-dev
elif command -v dnf >/dev/null 2>&1; then
    dnf install -y gcc gcc-c++ make autoconf pkgconf-pkg-config git curl \
        openssl-devel libxml2-devel libcurl-devel sqlite-devel \
        zlib-devel oniguruma-devel readline-devel libzip-devel bzip2-devel \
        libpng-devel libjpeg-turbo-devel libwebp-devel libicu-devel
elif command -v microdnf >/dev/null 2>&1; then
    microdnf install -y gcc gcc-c++ make autoconf pkgconf-pkg-config git curl \
        openssl-devel libxml2-devel libcurl-devel sqlite-devel \
        zlib-devel oniguruma-devel readline-devel libzip-devel bzip2-devel \
        libpng-devel libjpeg-turbo-devel libwebp-devel libicu-devel
elif command -v yum >/dev/null 2>&1; then
    yum install -y gcc gcc-c++ make autoconf pkgconfig git curl \
        openssl-devel libxml2-devel libcurl-devel sqlite-devel \
        zlib-devel oniguruma-devel readline-devel libzip-devel bzip2-devel \
        libpng-devel libjpeg-turbo-devel libwebp-devel libicu-devel
elif command -v apk >/dev/null 2>&1; then
    apk add --no-cache build-base make autoconf pkgconf git curl \
        openssl-dev libxml2-dev curl-dev sqlite-dev zlib-dev \
        oniguruma-dev readline-dev libzip-dev bzip2-dev libpng-dev \
        libjpeg-turbo-dev libwebp-dev icu-dev
elif command -v pacman >/dev/null 2>&1; then
    pacman -Sy --noconfirm --needed base-devel autoconf pkgconf git curl \
        openssl libxml2 curl sqlite zlib oniguruma readline libzip bzip2 \
        libpng libjpeg-turbo libwebp icu
else
    echo "Error: no supported package manager found (apt-get, dnf, microdnf, yum, apk, pacman)." >&2
    exit 1
fi
