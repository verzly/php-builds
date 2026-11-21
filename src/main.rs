use clap::{Parser, Subcommand};
use std::{
    error::Error,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

mod build;
mod relocate;
mod resolve;
mod selfcheck;
mod verify;

use build::build;
use resolve::resolve;
use selfcheck::selfcheck;
use verify::verify;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

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
        #[arg(long)]
        print_version: bool,
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
    #[command(name = "selfcheck")]
    SelfCheck {
        #[arg(long)]
        package: PathBuf,
        #[arg(long)]
        minor: String,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Resolve {
            minor,
            json,
            print_version,
        } => {
            let source = resolve(&minor)?;
            if print_version {
                println!("{}", source.version);
            } else if json {
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
        Commands::SelfCheck { package, minor } => selfcheck(&package, &minor)?,
    }
    Ok(())
}

pub(crate) fn curl_bytes(url: &str) -> Result<Vec<u8>> {
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

pub(crate) fn run(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if !status.success() {
        return Err(format!("Command failed with status {status}: {command:?}").into());
    }
    Ok(())
}

pub(crate) fn capture(command: &mut Command) -> Result<String> {
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

pub(crate) fn sha256(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
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

pub(crate) fn download(url: &str, target: &Path) -> Result<()> {
    let status = Command::new("curl")
        .args(["-fSL", url, "-o"])
        .arg(target)
        .status()?;
    if !status.success() {
        return Err(format!("curl failed for {url}: {status}").into());
    }
    Ok(())
}

pub(crate) fn target_label() -> String {
    std::env::var("PHP_BUILD_TARGET_LABEL")
        .ok()
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| os_name().to_string())
}

pub(crate) fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

pub(crate) fn architecture() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "arm64"
    } else {
        std::env::consts::ARCH
    }
}
