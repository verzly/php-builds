#!/usr/bin/env python3
import os
import pathlib
import subprocess
import sys
import urllib.request
import shutil

root = pathlib.Path(sys.argv[1]).resolve()
minor = sys.argv[2]
bin_dir = root / "relocated" / "bin"
sdk = root / "relocated"
verify_root = root / "verification"
if verify_root.exists():
    shutil.rmtree(verify_root)
shutil.copytree(sdk, verify_root)
sdk = verify_root
bin_dir = sdk / "bin"
for executable in ("php", "phpize", "php-config"):
    if not (bin_dir / executable).is_file():
        raise SystemExit(f"Missing relocated entrypoint: {executable}")

environment = os.environ.copy()
environment.update({
    "PATH": f"{bin_dir}:{os.environ.get('PATH', '')}",
    "PHPRC": str(sdk / "lib/php.ini"),
    "PHP_INI_SCAN_DIR": str(sdk / "etc/conf.d"),
    "PHP_INI_DIR": str(sdk / "lib"),
    "PHP_BINARY": str(bin_dir / "php"),
    "PIE_WORKING_DIRECTORY": str(root / "pie-work"),
})
def run(command, **kwargs):
    subprocess.run(command, check=True, env=environment, **kwargs)

run([str(bin_dir / "php"), "-v"])
run([str(bin_dir / "phpize"), "--version"])
run([str(bin_dir / "php-config"), "--version"])
for arg in ("--prefix", "--includes", "--include-dir", "--php-binary", "--ini-path",
            "--ini-dir", "--extension-dir", "--man-dir", "--ldflags"):
    output = subprocess.check_output([str(bin_dir / "php-config"), arg], env=environment, text=True)
    if arg not in ("--ldflags",) and str(sdk) not in output:
        raise SystemExit(f"php-config {arg} is not relocation-aware: {output.strip()}")
if subprocess.check_output([str(bin_dir / "php"), "-r", "echo PHP_VERSION;"], env=environment, text=True).split(".")[:2] != minor.split("."):
    raise SystemExit("Relocated PHP version does not match requested minor")

pie = root / "pie.phar"
urllib.request.urlretrieve("https://github.com/php/pie/releases/download/1.5.5/pie.phar", pie)
import hashlib
if hashlib.sha256(pie.read_bytes()).hexdigest() != "2ca0dff6868f1b53c095625674deef38ac698588e0a25e4321cd834a5b7553bc":
    raise SystemExit("PIE 1.5.5 checksum mismatch")
run([str(bin_dir / "php"), str(pie), "--version"])
run([str(bin_dir / "php"), str(pie), "--with-php-config=" + str(bin_dir / "php-config"),
     "--with-phpize-path=" + str(bin_dir / "phpize"),
     "--working-dir=" + str(environment["PIE_WORKING_DIRECTORY"]),
     "install", "phpredis/phpredis:6.3.0"])
loaded = subprocess.check_output(
    [str(bin_dir / "php"), "-r", "echo extension_loaded('redis') ? 'yes' : 'no';"],
    env=environment, text=True)
if loaded.strip() != "yes":
    raise SystemExit("Redis extension did not load from relocated SDK")
version = subprocess.check_output([str(bin_dir / "php"), "-r", "echo phpversion('redis');"], env=environment, text=True)
if not version.strip():
    raise SystemExit("Redis extension version is unavailable")
extension_dir = pathlib.Path(subprocess.check_output(
    [str(bin_dir / "php"), "-r", "echo ini_get('extension_dir');"],
    env=environment, text=True))
module_files = list(extension_dir.glob("*redis*.so")) + list(extension_dir.glob("*redis*.dylib"))
if not module_files:
    raise SystemExit(f"Redis module file is missing from relocated extension directory: {extension_dir}")
