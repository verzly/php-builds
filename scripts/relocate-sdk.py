#!/usr/bin/env python3
import pathlib
import re
import subprocess
import sys

prefix = pathlib.Path(sys.argv[1]).resolve()
bin_dir = prefix / "bin"

for name in ("phpize", "php-config"):
    path = bin_dir / name
    text = path.read_text()
    original_extension_dir = None
    if name == "php-config":
        original_extension_dir = pathlib.Path(subprocess.check_output(
            [str(path), "--extension-dir"], text=True
        ).strip())
        try:
            original_extension_dir = original_extension_dir.relative_to(prefix)
        except ValueError:
            raise SystemExit(f"php-config extension directory is outside install prefix: {original_extension_dir}")
    assignment = re.compile(r"(?m)^(prefix|extension_dir|ini_dir|ini_path|include_dir|php_cli_binary)=('([^']*)'|\"([^\"]*)\"|([^\n]*))$")
    matches = [(match, match.group(1)) for match in assignment.finditer(text)]
    if name == "php-config" and not matches:
        raise SystemExit(f"Cannot locate relocatable assignments in {path}")
    if name == "phpize":
        matches = [(match, key) for match, key in matches if key == "prefix"]
        if len(matches) != 1:
            raise SystemExit(f"Cannot locate prefix assignment in {path}")
    replacements = {
        "prefix": 'prefix="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"',
        "extension_dir": f'extension_dir="$prefix/{original_extension_dir}"',
        "ini_dir": 'ini_dir="$prefix/etc/conf.d"',
        "ini_path": 'ini_path="$prefix/lib"',
        "include_dir": 'include_dir="$prefix/include/php"',
        "php_cli_binary": 'php_cli_binary="$prefix/bin/php"',
    }
    replacements["man_dir"] = 'man_dir="$prefix/share/man"'
    if name == "php-config":
        man_match = re.search(r"(?m)^man_dir=.*$", text)
        if man_match:
            matches.append((man_match, "man_dir"))
    for match, key in sorted(matches, key=lambda item: item[0].start(), reverse=True):
        replacement = replacements.get(key)
        if replacement:
            text = text[:match.start()] + replacement + text[match.end():]
    # Preserve paths emitted from template substitutions elsewhere in php-config.
    text = text.replace(f"@prefix@", "$prefix")
    text = text.replace(str(prefix), "$prefix")
    path.write_text(text)
    path.chmod(0o755)

php = bin_dir / "php"
if php.exists():
    php.rename(bin_dir / "php.real")
    extension_output = subprocess.check_output(
        [str(bin_dir / "php.real"), "-r", "echo ini_get('extension_dir');"], text=True
    ).strip()
    extension_dir = pathlib.Path(extension_output)
    if not extension_dir.is_absolute():
        extension_dir = prefix / extension_dir
    try:
        extension_dir = extension_dir.relative_to(prefix)
    except ValueError:
        raise SystemExit(f"Compiled extension directory is outside install prefix: {extension_dir}")
    wrapper = f"""#!/usr/bin/env bash
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
if [ -z "${{PHPRC+x}}" ]; then export PHPRC="$root/lib/php.ini"; fi
if [ -z "${{PHP_INI_SCAN_DIR+x}}" ]; then export PHP_INI_SCAN_DIR="$root/etc/conf.d"; fi
exec "$root/bin/php.real" -d "extension_dir=$root/{extension_dir}" "$@"
"""
    (bin_dir / "php").write_text(wrapper)
    (bin_dir / "php").chmod(0o755)
