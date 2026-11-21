#!/usr/bin/env python3
import json
import re
import sys
import urllib.request

minor, output = sys.argv[1:]
if not re.fullmatch(r"8\.[1-6]", minor):
    raise SystemExit(f"Unsupported PHP minor: {minor}")
url = "https://www.php.net/releases/index.php?json&version=8&max=100"
with urllib.request.urlopen(url, timeout=30) as response:
    releases = json.load(response)
if isinstance(releases, dict):
    releases = [dict(value, version=key) for key, value in releases.items() if isinstance(value, dict)]
if not isinstance(releases, list):
    raise SystemExit("Invalid php.net releases JSON: expected object or list")

matches = []
for release in releases:
    if not isinstance(release, dict):
        continue
    version = release.get("version", "")
    if not isinstance(version, str) or not re.fullmatch(r"8\.\d+\.\d+", version):
        continue
    if ".".join(version.split(".")[:2]) != minor:
        continue
    sources = release.get("source", [])
    if not isinstance(sources, list):
        continue
    for source in sources:
        if not isinstance(source, dict):
            continue
        filename, digest = source.get("filename", ""), source.get("sha256", "")
        if filename == f"php-{version}.tar.xz" and isinstance(digest, str) and re.fullmatch(r"[0-9a-fA-F]{64}", digest):
            matches.append((version, filename, digest.lower()))
if not matches:
    raise SystemExit(f"No validated php.net tar.xz release for PHP {minor}")
version, filename, digest = max(matches, key=lambda item: tuple(map(int, item[0].split("."))))
with open(output, "w", encoding="utf-8") as stream:
    json.dump({"version": version, "filename": filename, "sha256": digest,
               "url": f"https://www.php.net/distributions/{filename}"}, stream, indent=2)
    stream.write("\n")
