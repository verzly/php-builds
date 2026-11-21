use crate::{capture, Result};
use std::{fs, path::Path, process::Command};

pub(crate) fn relocate(prefix: &Path) -> Result<()> {
    let bin = prefix.join("bin");
    let config = bin.join("php-config");
    let extension = capture(Command::new(&config).arg("--extension-dir"))?;
    let extension = Path::new(&extension)
        .strip_prefix(prefix)
        .map_err(|_| "php-config extension dir is outside install prefix")?
        .to_string_lossy();
    for name in ["php-config", "phpize"] {
        let path = bin.join(name);
        let mut text = fs::read_to_string(&path)?;
        if name == "php-config" {
            for (key, value) in [
                ("prefix", "$prefix"),
                ("include_dir", "$prefix/include/php"),
                ("extension_dir", ""),
                ("ini_dir", "$prefix/etc/conf.d"),
                ("ini_path", "$prefix/lib/php.ini"),
                ("php_cli_binary", "$prefix/bin/php"),
                ("man_dir", "$prefix/share/man"),
            ] {
                let replacement = if key == "extension_dir" {
                    format!("$prefix/{extension}")
                } else {
                    value.to_owned()
                };
                text = replace_shell_assignment(&text, key, &replacement)?;
            }
        } else {
            text = replace_shell_assignment(&text, "prefix", "$prefix")?;
        }
        text = text.replace(&prefix.to_string_lossy().to_string(), "$prefix");
        text = text.replace(
            "prefix=\"$prefix\"",
            "prefix=\"$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\"",
        );
        fs::write(path, text)?;
    }
    let php = bin.join("php");
    let php_real = bin.join("php.real");
    fs::rename(&php, &php_real)?;
    fs::write(
        &php,
        format!(
            "#!/usr/bin/env bash\nset -eu\nroot=\"$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\"\nif [ -z \"${{PHPRC+x}}\" ]; then export PHPRC=\"$root/lib/php.ini\"; fi\nif [ -z \"${{PHP_INI_SCAN_DIR+x}}\" ]; then export PHP_INI_SCAN_DIR=\"$root/etc/conf.d\"; fi\nexec \"$root/bin/php.real\" -d \"extension_dir=$root/{extension}\" \"$@\"\n"
        ),
    )?;
    set_executable(&php)?;
    set_executable(&php_real)?;
    Ok(())
}

fn replace_shell_assignment(text: &str, key: &str, value: &str) -> Result<String> {
    let needle = format!("{key}=");
    let matches: Vec<_> = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.starts_with(&needle))
        .collect();
    if matches.len() != 1 {
        return Err(format!("Expected one {key}= assignment, found {}", matches.len()).into());
    }
    let (line_index, _) = matches[0];
    let replacement = if value == "$prefix" {
        format!("{key}=\"$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\"")
    } else {
        format!("{key}=\"{value}\"")
    };
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    lines[line_index] = replacement;
    Ok(format!("{}\n", lines.join("\n")))
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_config_assignments_rewrite_quoted_paths_and_keep_unrelated_flags() {
        let template = "prefix='/old'\nextension_dir='/old/lib/php/extensions/abi'\nldflags='-L/old/lib -framework CoreFoundation'\n";
        let rewritten = replace_shell_assignment(template, "prefix", "$prefix").unwrap();
        let rewritten = rewritten.replace("/old", "$prefix");
        let rewritten = replace_shell_assignment(
            &rewritten,
            "extension_dir",
            "$prefix/lib/php/extensions/abi",
        )
        .unwrap();
        assert!(rewritten.contains("prefix=\"$(CDPATH="));
        assert!(rewritten.contains("extension_dir=\"$prefix/lib/php/extensions/abi\""));
        assert!(rewritten.contains("-framework CoreFoundation"));
    }

    #[cfg(unix)]
    #[test]
    fn relocate_rewrites_php_config_and_wraps_the_binary() {
        use std::os::unix::fs::PermissionsExt;

        let base = std::env::temp_dir().join(format!("php-builds-relocate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let prefix = base.join("install");
        let bin = prefix.join("bin");
        let extension = "lib/php/extensions/no-debug-non-zts-20210902";
        fs::create_dir_all(bin.join("x")).unwrap();
        fs::create_dir_all(prefix.join(extension)).unwrap();
        fs::create_dir_all(prefix.join("etc/conf.d")).unwrap();

        let config = bin.join("php-config");
        fs::write(
            &config,
            format!(
                "#!/usr/bin/env bash\nprefix=\"{p}\"\ninclude_dir=\"{p}/include/php\"\nextension_dir=\"{p}/{e}\"\nini_dir=\"{p}/etc/conf.d\"\nini_path=\"{p}/lib/php.ini\"\nphp_cli_binary=\"{p}/bin/php\"\nman_dir=\"{p}/share/man\"\nldflags=\"-L{p}/lib -lxml2\"\ncase \"$1\" in\n  --extension-dir) printf '%s\\n' \"$extension_dir\" ;;\nesac\n",
                p = prefix.display(),
                e = extension
            ),
        )
        .unwrap();
        let phpize = bin.join("phpize");
        fs::write(
            &phpize,
            format!(
                "#!/usr/bin/env bash\nprefix=\"{p}\"\nphpdir=\"{p}/lib/php/build\"\n",
                p = prefix.display()
            ),
        )
        .unwrap();
        let php = bin.join("php");
        fs::write(&php, "#!/usr/bin/env bash\nexit 0\n").unwrap();
        for file in [&config, &phpize, &php] {
            let mut permissions = fs::metadata(file).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(file, permissions).unwrap();
        }

        relocate(&prefix).unwrap();

        let legacy = prefix.to_string_lossy().to_string();
        let config_text = fs::read_to_string(&config).unwrap();
        assert!(!config_text.contains(&legacy));
        assert!(config_text.contains(&format!("extension_dir=\"$prefix/{extension}\"")));
        assert!(config_text.contains("-lxml2"));
        let phpize_text = fs::read_to_string(&phpize).unwrap();
        assert!(!phpize_text.contains(&legacy));
        assert!(phpize_text.contains("phpdir=\"$prefix/lib/php/build\""));
        assert!(bin.join("php.real").exists());
        let wrapper = fs::read_to_string(bin.join("php")).unwrap();
        assert!(wrapper.contains("PHP_INI_SCAN_DIR"));
        assert!(wrapper.contains(&format!("extension_dir=$root/{extension}")));
        fs::remove_dir_all(&base).unwrap();
    }
}
