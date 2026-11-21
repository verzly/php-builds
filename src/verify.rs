use crate::resolve::valid_minor;
use crate::{capture, download, run, sha256, Result};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const PIE_URL: &str = "https://github.com/php/pie/releases/download/1.5.5/pie.phar";
const PIE_SHA256: &str = "2ca0dff6868f1b53c095625674deef38ac698588e0a25e4321cd834a5b7553bc";

pub(crate) fn verify(package: &Path, minor: &str) -> Result<()> {
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
