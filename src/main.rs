use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use glob::glob;

/// Pre-commit hook for running cargo fmt/check/clippy against a repo.
/// The target repo may contain multiple independent cargo projects or workspaces.
#[derive(Debug, Parser)]
struct Opts {
    #[command(subcommand)]
    cmd: Cmd,
    /// List of chaned files to target
    #[clap(global = true)]
    files: Vec<PathBuf>,
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

fn main() -> ExitCode {
    let opts = Opts::parse();

    let run_dirs = get_run_dirs(&opts.files);

    let err_count = run_dirs
        .into_iter()
        .map(|dir| match &opts.cmd {
            Cmd::Fmt { config } => run_fmt(dir, config),
            Cmd::Check {
                features,
                all_features,
            } => run_check(dir, features, *all_features),
            Cmd::Clippy => run_clippy(dir),
        })
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

const NOT_FOUND: &str = "failed to run 'cargo'";

fn run_fmt(dir: PathBuf, config: &Option<String>) -> Result<()> {
    let mut cmd = cargo();
    cmd.args(["fmt", "--"]);

    if let Some(config) = config {
        cmd.args(["--config", config]);
    }

    cmd.current_dir(dir);
    let status = cmd.status()?;
    if !status.success() {
        bail!("cargo fmt modified files");
    }
    Ok(())
}

fn run_check(dir: PathBuf, features: &Option<String>, all_features: bool) -> Result<()> {
    let mut cmd = cargo();
    cmd.arg("check");

    if all_features {
        cmd.arg("--all-features");
    } else if let Some(features) = features {
        cmd.args(["--features", features]);
    }

    cmd.current_dir(dir);
    let status = cmd.status().context(NOT_FOUND)?;
    if !status.success() {
        bail!("cargo check failed");
    }
    Ok(())
}

fn run_clippy(dir: PathBuf) -> Result<()> {
    let status = cargo()
        .args(["clippy", "--", "-D", "warnings"])
        .current_dir(dir)
        .status()
        .context(NOT_FOUND)?;
    if !status.success() {
        bail!("cargo clippy failed");
    }
    Ok(())
}

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

fn cargo() -> Command {
    /// The compile-time location of cargo. Used to access the pre-commit managed environment
    /// of cargo for subcommands;
    const CARGO_HOME: &str = std::env!("CARGO_HOME");

    let bin = PathBuf::from(CARGO_HOME).join("bin").join("cargo");
    Command::new(bin)
}
