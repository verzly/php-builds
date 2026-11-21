use clap::{Parser, Subcommand};
use flate2::{write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};
use tar::{Archive, Builder};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const RELEASES_URL: &str = "https://www.php.net/releases/index.php?json&version=8&max=100";
const PIE_URL: &str = "https://github.com/php/pie/releases/download/1.5.5/pie.phar";
const PIE_SHA256: &str = "2ca0dff6868f1b53c095625674deef38ac698588e0a25e4321cd834a5b7553bc";

#[derive(Parser)]
#[command(
    name = "php-builds",
    about = "Build relocatable experimental PHP SDK bundles"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Resolve {
        #[arg(long)]
        minor: String,
        #[arg(long)]
        json: bool,
    },
    Build {
        #[arg(long)]
        minor: String,
        #[arg(long)]
        root: PathBuf,
    },
    Verify {
        #[arg(long)]
        package: PathBuf,
        #[arg(long)]
        minor: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Source {
    version: String,
    filename: String,
    url: String,
    sha256: String,
}

#[derive(Deserialize)]
struct Release {
    version: String,
    source: Option<Vec<ReleaseSource>>,
}

#[derive(Deserialize)]
struct ReleaseSource {
    filename: String,
    sha256: String,
}

#[derive(Serialize)]
struct Manifest {
    php_minor: String,
    source_version: String,
    source_sha256: String,
    platform: String,
    architecture: String,
    configure_options: Vec<String>,
    runtime_dependencies: String,
    archive: String,
    archive_sha256: String,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Resolve { minor, json } => {
            let source = resolve(&minor)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&source)?);
            } else {
                println!(
                    "{}\n{}\n{}\n{}",
                    source.version, source.filename, source.url, source.sha256
                );
            }
        }
        Commands::Build { minor, root } => println!("{}", build(&minor, &root)?.display()),
        Commands::Verify { package, minor } => verify(&package, &minor)?,
    }
    Ok(())
}

fn valid_minor(minor: &str) -> bool {
    matches!(minor, "8.1" | "8.2" | "8.3" | "8.4" | "8.5" | "8.6")
}

fn version_tuple(version: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
        })
    {
        return None;
    }
    let major = parts[0].parse().ok()?;
    let minor = parts[1].parse().ok()?;
    let patch = parts[2].parse().ok()?;
    Some((major, minor, patch))
}

fn resolve_releases(minor: &str, payload: &serde_json::Value) -> Result<Source> {
    if !valid_minor(minor) {
        return Err(format!("Unsupported PHP minor: {minor}").into());
    }
    let releases: Vec<Release> = match payload {
        serde_json::Value::Object(map) => map
            .iter()
            .filter_map(|(version, item)| {
                let mut value = item.clone();
                value
                    .as_object_mut()?
                    .insert("version".to_owned(), version.clone().into());
                serde_json::from_value(value).ok()
            })
            .collect(),
        serde_json::Value::Array(_) => serde_json::from_value(payload.clone())?,
        _ => return Err("Invalid php.net releases JSON: expected object or list".into()),
    };
    let mut candidates = Vec::new();
    for release in releases {
        let Some((major, minor_version, _)) = version_tuple(&release.version) else {
            continue;
        };
        if format!("{major}.{minor_version}") != minor {
            continue;
        }
        for source in release.source.unwrap_or_default() {
            if source.filename != format!("php-{}.tar.xz", release.version)
                || source.sha256.len() != 64
                || !source.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                continue;
            }
            candidates.push((
                version_tuple(&release.version).unwrap(),
                Source {
                    version: release.version.clone(),
                    filename: source.filename,
                    url: format!(
                        "https://www.php.net/distributions/php-{}.tar.xz",
                        release.version
                    ),
                    sha256: source.sha256.to_ascii_lowercase(),
                },
            ));
        }
    }
    candidates.sort_by_key(|candidate| candidate.0);
    candidates
        .pop()
        .map(|candidate| candidate.1)
        .ok_or_else(|| format!("No validated php.net tar.xz release for PHP {minor}").into())
}

fn resolve(minor: &str) -> Result<Source> {
    let payload: serde_json::Value = serde_json::from_slice(&curl_bytes(RELEASES_URL)?)?;
    resolve_releases(minor, &payload)
}

fn curl_bytes(url: &str) -> Result<Vec<u8>> {
    let output = Command::new("curl").args(["-fSL", url]).output()?;
    if !output.status.success() {
        return Err(format!(
            "curl failed for {url}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output.stdout)
}

fn run(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if !status.success() {
        return Err(format!("Command failed with status {status}: {command:?}").into());
    }
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

fn sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn extract_xz(archive: &Path, destination: &Path) -> Result<()> {
    let mut source = BufReader::new(File::open(archive)?);
    let mut decoded = Vec::new();
    lzma_rs::xz_decompress(&mut source, &mut decoded)?;
    Archive::new(decoded.as_slice()).unpack(destination)?;
    Ok(())
}

fn configure_options(prefix: &Path) -> Vec<String> {
    vec![
        format!("--prefix={}", prefix.display()),
        format!("--with-config-file-path={}/lib", prefix.display()),
        format!(
            "--with-config-file-scan-dir={}/etc/conf.d",
            prefix.display()
        ),
        "--disable-cgi".into(),
        // Extensions that the default build already enables are left implicit, so
        // the option list stays valid across every supported PHP line.
        "--without-pear".into(),
        "--with-openssl".into(),
        "--with-curl".into(),
        "--with-zlib".into(),
        "--with-zip".into(),
        "--with-bz2".into(),
        "--with-readline".into(),
        "--with-pdo-sqlite".into(),
        "--with-sqlite3".into(),
        "--enable-mbstring".into(),
        "--enable-intl".into(),
        "--enable-opcache".into(),
    ]
}

fn build(minor: &str, root: &Path) -> Result<PathBuf> {
    if !valid_minor(minor) {
        return Err(format!("Unsupported PHP minor: {minor}").into());
    }
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("logs"))?;
    fs::create_dir_all(root.join("dist"))?;
    let source = resolve(minor)?;
    let source_dir = root.join("src").join(format!("php-{}", source.version));
    fs::create_dir_all(&source_dir)?;
    let source_archive = source_dir.join(&source.filename);
    download(&source.url, &source_archive)?;
    let actual_hash = sha256(&source_archive)?;
    if actual_hash != source.sha256 {
        return Err(format!(
            "SHA256 mismatch: expected {}, got {actual_hash}",
            source.sha256
        )
        .into());
    }
    extract_xz(&source_archive, &source_dir)?;
    let source_tree = source_dir.join(format!("php-{}", source.version));

    let prefix = root.join("install");
    fs::create_dir_all(&prefix)?;
    let options = configure_options(&prefix);
    let mut configure = Command::new("./configure");
    configure.args(&options).current_dir(&source_tree);
    if cfg!(target_os = "macos") {
        let mut pkg_config = Vec::new();
        let mut cppflags = Vec::new();
        let mut ldflags = Vec::new();
        for formula in [
            "openssl@3",
            "libxml2",
            "curl",
            "sqlite3",
            "zlib",
            "oniguruma",
            "readline",
            "libzip",
            "bzip2",
            "libpng",
            "jpeg-turbo",
            "webp",
            "icu4c",
        ] {
            let prefix = capture(Command::new("brew").args(["--prefix", formula]))?;
            pkg_config.push(format!("{prefix}/lib/pkgconfig"));
            pkg_config.push(format!("{prefix}/share/pkgconfig"));
            cppflags.push(format!("-I{prefix}/include"));
            ldflags.push(format!("-L{prefix}/lib"));
        }
        configure.env("PKG_CONFIG_PATH", pkg_config.join(":"));
        configure.env("CPPFLAGS", cppflags.join(" "));
        configure.env("LDFLAGS", ldflags.join(" "));
    }
    run_logged(&mut configure, &root.join("logs/configure.log"))?;
    let jobs = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    run_logged(
        &mut Command::new("make")
            .arg(format!("-j{jobs}"))
            .current_dir(&source_tree),
        &root.join("logs/make.log"),
    )?;
    run_logged(
        &mut Command::new("make")
            .arg("install")
            .current_dir(&source_tree),
        &root.join("logs/install.log"),
    )?;

    let license = source_tree.join("LICENSE");
    fs::copy(license, prefix.join("LICENSE"))?;
    fs::create_dir_all(prefix.join("etc/conf.d"))?;
    fs::copy(
        source_tree.join("php.ini-production"),
        prefix.join("lib/php.ini"),
    )?;
    fs::copy(
        source_tree.join("php.ini-production"),
        prefix.join("lib/php.ini-production"),
    )?;
    fs::copy(
        source_tree.join("php.ini-development"),
        prefix.join("lib/php.ini-development"),
    )?;
    relocate(&prefix)?;

    let relocated = root.join("relocated");
    if relocated.exists() {
        fs::remove_dir_all(&relocated)?;
    }
    fs::rename(&prefix, &relocated)?;
    let archive_name = format!(
        "php-{}-{}-{}.tar.gz",
        source.version,
        os_name(),
        architecture()
    );
    let archive_path = root.join("dist").join(&archive_name);
    create_targz(&relocated, &archive_path)?;
    let archive_sha256 = sha256(&archive_path)?;
    let manifest = Manifest {
        php_minor: minor.into(),
        source_version: source.version,
        source_sha256: source.sha256,
        platform: os_name().into(),
        architecture: architecture().into(),
        configure_options: options,
        runtime_dependencies: "System-linked OpenSSL 3, curl, XML, SQLite, zlib, and other build-host libraries; not self-contained.".into(),
        archive: archive_name.clone(),
        archive_sha256: archive_sha256.clone(),
    };
    fs::write(
        root.join("dist/manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::write(
        root.join("dist/SHA256SUMS"),
        format!("{archive_sha256}  {archive_name}\n"),
    )?;
    let package = root.join("package");
    if package.exists() {
        fs::remove_dir_all(&package)?;
    }
    fs::create_dir_all(&package)?;
    extract_targz(&archive_path, &package)?;
    Ok(package)
}

fn download(url: &str, target: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args(["-fSL", url, "-o"])
        .arg(target)
        .status()?;
    if !status.success() {
        return Err(format!("curl failed for {url}: {status}").into());
    }
    Ok(())
}

fn run_logged(command: &mut Command, log: &Path) -> Result<()> {
    let output = command.output()?;
    fs::write(log, [&output.stdout[..], &output.stderr[..]].concat())?;
    std::io::stderr().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    if !output.status.success() {
        return Err(format!("Build command failed: {}", output.status).into());
    }
    Ok(())
}

fn relocate(prefix: &Path) -> Result<()> {
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

fn create_targz(source: &Path, output: &Path) -> Result<()> {
    let encoder = GzEncoder::new(File::create(output)?, Compression::default());
    let mut archive = Builder::new(encoder);
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            archive.append_dir_all(&name, &path)?;
        } else {
            archive.append_path_with_name(&path, &name)?;
        }
    }
    archive.into_inner()?.finish()?;
    Ok(())
}

fn extract_targz(archive: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive)?;
    Archive::new(flate2::read::GzDecoder::new(file)).unpack(destination)?;
    Ok(())
}

fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn architecture() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "arm64"
    } else {
        std::env::consts::ARCH
    }
}

fn verify(package: &Path, minor: &str) -> Result<()> {
    if !valid_minor(minor) {
        return Err(format!("Unsupported PHP minor: {minor}").into());
    }
    let root = package.parent().unwrap_or(package).join("verify-copy");
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    copy_tree(package, &root)?;
    let bin = root.join("bin");
    let work = package.parent().unwrap_or(package).join("pie-work");
    fs::create_dir_all(&work)?;
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut environment = BTreeMap::new();
    environment.insert("PATH", path);
    environment.insert("PHPRC", root.join("lib/php.ini").display().to_string());
    environment.insert(
        "PHP_INI_SCAN_DIR",
        root.join("etc/conf.d").display().to_string(),
    );
    environment.insert("PHP_INI_DIR", root.join("lib").display().to_string());
    environment.insert("PHP_BINARY", bin.join("php").display().to_string());
    environment.insert("PIE_WORKING_DIRECTORY", work.display().to_string());
    for (program, args) in [("php", vec!["-v"]), ("phpize", vec!["--version"])] {
        run(Command::new(bin.join(program))
            .args(args)
            .envs(&environment))?;
    }
    for option in [
        "--prefix",
        "--includes",
        "--include-dir",
        "--php-binary",
        "--ini-path",
        "--ini-dir",
        "--extension-dir",
        "--man-dir",
        "--ldflags",
    ] {
        let output = capture(
            Command::new(bin.join("php-config"))
                .arg(option)
                .envs(&environment),
        )?;
        if option != "--ldflags" && !output.contains(root.to_string_lossy().as_ref()) {
            return Err(format!("php-config {option} is not relocated: {output}").into());
        }
    }
    let version = capture(
        Command::new(bin.join("php"))
            .args(["-r", "echo PHP_VERSION;"])
            .envs(&environment),
    )?;
    if !version.starts_with(&format!("{minor}.")) {
        return Err(format!("Unexpected PHP version: {version}").into());
    }
    let pie = work.join("pie.phar");
    download(PIE_URL, &pie)?;
    if sha256(&pie)? != PIE_SHA256 {
        return Err("PIE 1.5.5 checksum mismatch".into());
    }
    run(Command::new(bin.join("php"))
        .arg(&pie)
        .args([
            "--with-php-config",
            bin.join("php-config").to_str().unwrap(),
            "--with-phpize-path",
            bin.join("phpize").to_str().unwrap(),
            "--working-dir",
            work.to_str().unwrap(),
            "install",
            "phpredis/phpredis:6.3.0",
        ])
        .envs(&environment))?;
    let loaded = capture(
        Command::new(bin.join("php"))
            .args(["-r", "echo extension_loaded('redis') ? 'yes' : 'no';"])
            .envs(&environment),
    )?;
    if loaded != "yes" {
        return Err("Redis extension did not load".into());
    }
    let version = capture(
        Command::new(bin.join("php"))
            .args(["-r", "echo phpversion('redis');"])
            .envs(&environment),
    )?;
    if version.is_empty() {
        return Err("Redis extension version is empty".into());
    }
    let extension = PathBuf::from(capture(
        Command::new(bin.join("php"))
            .args(["-r", "echo ini_get('extension_dir');"])
            .envs(&environment),
    )?);
    if !extension.starts_with(&root) {
        return Err(format!(
            "Extension directory is outside relocated package: {}",
            extension.display()
        )
        .into());
    }
    if !fs::read_dir(extension)?
        .filter_map(|entry| entry.ok())
        .any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.contains("redis") && (name.ends_with(".so") || name.ends_with(".dylib"))
        })
    {
        return Err("Redis module file not found under relocated extension directory".into());
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_selects_highest_valid_numeric_patch_and_rejects_bad_metadata() {
        let payload = serde_json::json!({
            "8.5.9": {"source":[{"filename":"php-8.5.9.tar.xz","sha256":"a".repeat(64)}]},
            "8.5.12": {"source":[{"filename":"php-8.5.12.tar.xz","sha256":"b".repeat(64)}]},
            "8.5.99RC1": {"source":[{"filename":"php-8.5.99RC1.tar.xz","sha256":"c".repeat(64)}]},
            "8.5.13": {"source":[{"filename":"php-8.5.12.tar.xz","sha256":"d".repeat(64)}]},
            "8.6.1": {"source":[{"filename":"php-8.6.1.tar.xz","sha256":"e".repeat(64)}]}
        });
        assert_eq!(resolve_releases("8.5", &payload).unwrap().version, "8.5.12");
        assert!(resolve_releases("8.7", &payload).is_err());
        assert_eq!(resolve_releases("8.6", &payload).unwrap().version, "8.6.1");
        let invalid = serde_json::json!({
            "8.6.2": {"source":[{"filename":"php-8.6.2.tar.xz","sha256":"not-a-hash"}]}
        });
        assert!(resolve_releases("8.6", &invalid).is_err());
    }

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

    #[test]
    fn archive_round_trip_keeps_the_package_root() {
        let base = std::env::temp_dir().join(format!("php-builds-archive-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let source = base.join("source");
        fs::create_dir_all(source.join("bin")).unwrap();
        fs::write(source.join("bin/php"), b"stub").unwrap();
        let archive = base.join("package.tar.gz");
        create_targz(&source, &archive).unwrap();
        let out = base.join("out");
        fs::create_dir_all(&out).unwrap();
        extract_targz(&archive, &out).unwrap();
        assert!(out.join("bin/php").is_file());
        assert!(!out.join("relocated").exists());
        fs::remove_dir_all(&base).unwrap();
    }
}
