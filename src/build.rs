use crate::relocate::relocate;
use crate::resolve::{resolve, valid_minor};
use crate::{architecture, capture, download, sha256, target_label, Result};
use flate2::{write::GzEncoder, Compression};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
};
use tar::{Archive, Builder};

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

fn extract_xz(archive: &Path, destination: &Path) -> Result<()> {
    let mut source = BufReader::new(File::open(archive)?);
    let mut decoded = Vec::new();
    lzma_rs::xz_decompress(&mut source, &mut decoded)?;
    Archive::new(decoded.as_slice()).unpack(destination)?;
    Ok(())
}

fn pkg_config_exists(name: &str) -> bool {
    Command::new("pkg-config")
        .args(["--exists", name])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
        "--with-readline".into(),
        "--with-pdo-sqlite".into(),
        "--with-sqlite3".into(),
        "--enable-mbstring".into(),
        "--enable-intl".into(),
    ]
}

pub(crate) fn build(minor: &str, root: &Path) -> Result<PathBuf> {
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
    let mut options = configure_options(&prefix);
    // bzip2 and libzip are not visible to configure through the default search
    // path on every platform, and libzip is absent from some base repositories.
    if cfg!(target_os = "macos") {
        let bzip2 = capture(Command::new("brew").args(["--prefix", "bzip2"]))?;
        options.push(format!("--with-bz2={bzip2}"));
        let iconv = capture(Command::new("brew").args(["--prefix", "libiconv"]))?;
        options.push(format!("--with-iconv={iconv}"));
        let readline = capture(Command::new("brew").args(["--prefix", "readline"]))?;
        options.push(format!("--with-readline={readline}"));
        options.push("--with-zip".into());
    } else if pkg_config_exists("libzip") {
        options.push("--with-zip".into());
    }
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
        target_label(),
        architecture()
    );
    let archive_path = root.join("dist").join(&archive_name);
    create_targz(&relocated, &archive_path)?;
    let archive_sha256 = sha256(&archive_path)?;
    let manifest = Manifest {
        php_minor: minor.into(),
        source_version: source.version,
        source_sha256: source.sha256,
        platform: target_label(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
