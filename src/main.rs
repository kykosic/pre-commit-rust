use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{anyhow, bail, Context, Error, Result};
use clap::{Args, Parser, Subcommand};
use glob::glob;
use regex::Regex;
use semver::Version;

/// Pre-commit hook for running cargo fmt/check/clippy against a repo.
/// The target repo may contain multiple independent cargo projects or workspaces.
#[derive(Debug, Parser)]
struct Opts {
    #[command(subcommand)]
    cmd: Cmd,

    /// List of chaned files to target.
    #[clap(global = true)]
    files: Vec<PathBuf>,

    #[command(flatten)]
    cargo_opts: CargoOpts,
}

/// Configuration for cargo toolchain versioning
#[derive(Debug, Args)]
struct CargoOpts {
    /// Minimum rustc version, checked before running.
    // Alternatively, you can set pre-commit `default_language_version.rust`, and a managed rust
    // environment will be created and used at the exact version specified.
    #[clap(long, global = true)]
    rust_version: Option<Version>,
    /// If `rust_version` is specified and an update is needed, automatically run `rustup update`.
    #[clap(long, global = true)]
    auto_update: bool,
    /// Override the error message printed if `cargo` or the command executable is not found.
    #[clap(long, global = true)]
    not_found_message: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Run the rustfmt (cargo fmt) hook
    Fmt {
        /// Comma-separated key=value config pairs for rustfmt
        #[clap(long)]
        config: Option<String>,
    },
    /// Run the cargo check hook
    Check {
        /// Comma-separated list of features to check
        #[clap(long)]
        features: Option<String>,
        /// Activate all available features
        #[clap(long)]
        all_features: bool,
    },
    /// Run the cargo clippy hook
    Clippy,
}

impl Cmd {
    pub fn run(&self, dir: PathBuf) -> Result<()> {
        match self {
            Cmd::Fmt { config } => {
                let mut cmd = Command::new("cargo");
                cmd.arg("fmt");

                if let Some(config) = config {
                    cmd.args(["--", "--config", config]);
                }

                cmd.current_dir(dir);
                let status = cmd.status().context("failed to exec `cargo fmt`")?;
                if !status.success() {
                    bail!("`cargo fmt` found errors");
                }
                Ok(())
            }
            Cmd::Check {
                features,
                all_features,
            } => {
                let mut cmd = Command::new("cargo");
                cmd.arg("check");

                if *all_features {
                    cmd.arg("--all-features");
                } else if let Some(features) = features {
                    cmd.args(["--features", features]);
                }

                cmd.current_dir(dir);
                let status = cmd.status().context("failed to exec `cargo check`")?;
                if !status.success() {
                    bail!("`cargo check` found errors");
                }
                Ok(())
            }
            Cmd::Clippy => {
                let status = Command::new("cargo")
                    .args(["clippy", "--", "-D", "warnings"])
                    .current_dir(dir)
                    .status()
                    .context("failed to exec `cargo clippy`")?;
                if !status.success() {
                    bail!("`cargo clippy` found errors");
                }
                Ok(())
            }
        }
    }

    /// Check the `cargo` subcommand can be run, validating `CargoOpts` are satisfied
    pub fn check_subcommand(&self) -> Result<()> {
        let sub = match self {
            Cmd::Fmt { .. } => "fmt",
            Cmd::Check { .. } => "check",
            Cmd::Clippy { .. } => "clippy",
        };

        let out = Command::new("cargo")
            .arg(sub)
            .arg("--help")
            .output()
            .map_err(|_| self.missing())?;

        if !out.status.success() {
            Err(self.missing())
        } else {
            Ok(())
        }
    }

    fn missing(&self) -> Error {
        match self {
            Cmd::Fmt { .. } => {
                anyhow!("Missing `cargo fmt`, try installing with `rustup component add rustfmt`")
            }
            Cmd::Check { .. } => {
                anyhow!("Missing `cargo check`, you may need to update or reinstall rust.")
            }
            Cmd::Clippy { .. } => {
                anyhow!("Missing `cargo clippy`, try installing with `rustup component add clippy`")
            }
        }
    }
}

/// Verify the cargo/rust toolchain exists and meets the configured requirements
fn check_toolchain(opts: &CargoOpts) -> Result<()> {
    match toolchain_version()? {
        Some(ver) => {
            if let Some(msrv) = &opts.rust_version {
                if &ver < msrv {
                    if opts.auto_update {
                        eprintln!("Rust toolchain {ver} does not meet minimum required version {msrv}, updating...");
                        update_rust()?;
                    } else {
                        bail!("Rust toolchain {} does not meet minimum required version {}. You may need to run `rustup update`.", ver, msrv);
                    }
                }
            }
        }
        None => {
            match &opts.not_found_message {
                Some(msg) => bail!("{}", msg),
                None => bail!("Could not locate `cargo` binary. See https://www.rust-lang.org/tools/install to install rust"),
            }
        }
    }
    Ok(())
}

/// Returns `Ok(None)` if cargo binary is not found / fails to run.
/// Errors when `cargo --version` runs, but the output cannot be parsed.
fn toolchain_version() -> Result<Option<Version>> {
    let Ok(out) = Command::new("cargo").arg("--version").output() else { return Ok(None) };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let version_re = Regex::new(r"cargo (\d+\.\d+\.\S+)").unwrap();
    let caps = version_re
        .captures(&stdout)
        .ok_or_else(|| anyhow!("Unexpected `cargo --version` output: {stdout}"))?;
    let version = caps[1]
        .parse()
        .context(format!("could not parse cargo version: {}", &caps[1]))?;
    Ok(Some(version))
}

fn update_rust() -> Result<()> {
    let status = Command::new("rustup")
        .arg("update")
        .status()
        .context("failed to run `rustup update`, is rust installed?")?;
    if !status.success() {
        bail!("failed to run `rustup update`, see above errors");
    }
    Ok(())
}

/// Get all root cargo workspaces that need to be checked based on changed files
fn get_run_dirs(changed_files: &[PathBuf]) -> HashSet<PathBuf> {
    let root_dirs = find_cargo_root_dirs();
    let mut run_dirs: HashSet<PathBuf> = HashSet::new();
    let current_dir = std::env::current_dir().unwrap();
    for path in changed_files {
        if !is_rust_file(path) {
            continue;
        }
        if let Some(root) = root_dirs
            .iter()
            .filter(|d| path.starts_with(d))
            .max_by_key(|path| path.components().count())
        {
            run_dirs.insert(current_dir.join(root));
        }
    }
    run_dirs
}

/// Find all root-level cargo workspaces from the current repository root
fn find_cargo_root_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for entry in glob("**/Cargo.toml").unwrap() {
        match entry {
            Ok(path) => dirs.push(path.parent().unwrap().into()),
            Err(e) => eprintln!("{e:?}"),
        }
    }
    dirs
}

/// Check if changed file path should trigger a hook run
fn is_rust_file<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref();
    if let Some(ext) = path.extension() {
        if ext == "rs" {
            return true;
        }
    }
    if let Some(name) = path.file_name() {
        let name = name.to_string_lossy();
        if ["Cargo.toml", "Cargo.lock"].contains(&name.as_ref()) {
            return true;
        }
    }
    false
}

fn main() -> ExitCode {
    let opts = Opts::parse();

    if let Err(e) = check_toolchain(&opts.cargo_opts) {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    if let Err(e) = opts.cmd.check_subcommand() {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }

    let run_dirs = get_run_dirs(&opts.files);
    let err_count = run_dirs
        .into_iter()
        .map(|dir| opts.cmd.run(dir))
        .filter(|res| match res {
            Ok(()) => false,
            Err(e) => {
                eprintln!("{}", e);
                true
            }
        })
        .count();

    if err_count > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
