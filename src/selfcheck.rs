use crate::resolve::valid_minor;
use crate::Result;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

pub(crate) fn selfcheck(package: &Path, minor: &str) -> Result<()> {
    if !valid_minor(minor) {
        return Err(format!("Unsupported PHP minor: {minor}").into());
    }

    let bin = package.join("bin");
    let mut path = bin.display().to_string();
    path.push(':');
    path.push_str(&std::env::var("PATH").unwrap_or_default());
    let environment = BTreeMap::from([
        ("PATH", path),
        ("PHPRC", package.join("lib/php.ini").display().to_string()),
        (
            "PHP_INI_SCAN_DIR",
            package.join("etc/conf.d").display().to_string(),
        ),
        ("PHP_INI_DIR", package.join("lib").display().to_string()),
        ("PHP_BINARY", bin.join("php").display().to_string()),
    ]);

    let version = capture(
        Command::new(bin.join("php"))
            .args(["-v"])
            .envs(&environment),
    )?;
    if !version
        .strip_prefix("PHP ")
        .is_some_and(|version| version.starts_with(&format!("{minor}.")))
    {
        return Err(format!("Unexpected PHP version: {version}").into());
    }
    println!(
        "PHP version: {}",
        version.lines().next().unwrap_or_default()
    );

    let modules = capture(
        Command::new(bin.join("php"))
            .args(["-m"])
            .envs(&environment),
    )?;
    let modules: BTreeSet<_> = modules.lines().map(str::trim).collect();
    let required = [
        "Core",
        "ctype",
        "curl",
        "date",
        "filter",
        "fileinfo",
        "hash",
        "iconv",
        "intl",
        "json",
        "libxml",
        "mbstring",
        "openssl",
        "pcre",
        "PDO",
        "pdo_sqlite",
        "Phar",
        "posix",
        "readline",
        "Reflection",
        "session",
        "SPL",
        "sqlite3",
        "standard",
        "tokenizer",
        "zip",
        "zlib",
        "bz2",
        "Zend OPcache",
    ];
    let missing: Vec<_> = required
        .iter()
        .filter(|module| !modules.contains(**module))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(format!("Missing PHP modules: {}", missing.join(", ")).into());
    }
    println!(
        "PHP modules: all {} required modules loaded",
        required.len()
    );

    let script = r#"
$checks = [
    'openssl_encrypt' => fn() => openssl_encrypt('x', 'aes-128-cbc', 'k', 0, '1234567890123456') !== false,
    'curl_init' => fn() => function_exists('curl_init'),
    'gzcompress' => fn() => gzcompress('x') !== false,
    'bzcompress' => fn() => bzcompress('x') !== false,
    'ZipArchive' => fn() => class_exists('ZipArchive'),
    'SQLite3' => fn() => (new SQLite3(':memory:'))->querySingle('select 1') === 1,
    'PDO sqlite' => fn() => in_array('sqlite', PDO::getAvailableDrivers(), true),
    'mb_strtoupper' => fn() => mb_strtoupper('é') === 'É',
    'Collator' => fn() => class_exists('Collator'),
    'readline' => fn() => function_exists('readline'),
    'Zend OPcache' => fn() => extension_loaded('Zend OPcache'),
    'Phar' => fn() => class_exists('Phar'),
    'iconv' => fn() => function_exists('iconv'),
];
$failed = [];
foreach ($checks as $name => $check) {
    try {
        if (!$check()) {
            $failed[] = $name;
        }
    } catch (Throwable $error) {
        $failed[] = $name;
    }
}
if ($failed) {
    fwrite(STDERR, implode(', ', $failed));
    exit(1);
}
echo 'ok';
"#;
    let feature_output = Command::new(bin.join("php"))
        .args(["-r", script])
        .envs(&environment)
        .output()?;
    if !feature_output.status.success() {
        let failures = String::from_utf8_lossy(&feature_output.stderr);
        return Err(format!("PHP feature checks failed: {failures}").into());
    }
    if String::from_utf8(feature_output.stdout)?.trim() != "ok" {
        return Err("PHP feature checks returned unexpected output".into());
    }
    println!("PHP features: all 13 runtime checks passed");

    // The relocated SDK keeps the real binary as php.real behind a wrapper.
    let real = package.join("bin/php.real");
    let binary = if real.exists() {
        real
    } else {
        package.join("bin/php")
    };
    #[cfg(target_os = "linux")]
    check_linked_libraries("ldd", &[], &binary, |output| {
        output.lines().any(|line| line.contains("not found"))
    })?;
    #[cfg(target_os = "macos")]
    check_linked_libraries("otool", &["-L"], &binary, |output| {
        output.lines().skip(1).any(|line| {
            let Some(path) = line.split_whitespace().next() else {
                return false;
            };
            let path = if let Some(path) = path.strip_prefix("@rpath/") {
                package.join("lib").join(path)
            } else if let Some(path) = path.strip_prefix("@loader_path/") {
                package.join("bin").join(path)
            } else if let Some(path) = path.strip_prefix("@executable_path/") {
                package.join("bin").join(path)
            } else {
                Path::new(path).to_path_buf()
            };
            // System libraries live in the dyld shared cache, not on disk.
            if path.starts_with("/usr/lib") || path.starts_with("/System/Library") {
                return false;
            }
            !path.exists()
        })
    })?;
    println!("Dynamic libraries: passed or checker unavailable");
    Ok(())
}

fn capture(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "Command failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn check_linked_libraries(
    program: &str,
    flags: &[&str],
    binary: &Path,
    invalid: impl FnOnce(&str) -> bool,
) -> Result<()> {
    match Command::new(program).args(flags).arg(binary).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if invalid(&stdout) {
                return Err(format!("{program} found unresolved libraries:\n{stdout}").into());
            }
        }
        Ok(output) => {
            return Err(format!(
                "{program} failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            println!("Skipping dynamic library check: {program} is unavailable");
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
