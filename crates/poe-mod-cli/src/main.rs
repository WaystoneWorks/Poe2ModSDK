//! `poe-mod`: the command-line tool of the PoE2 mod SDK.

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use poe_mod_cli::{
    Kind, NewOptions, PackOptions, SdkSource, check_dir, new_project, pack, verify_zip,
};

#[derive(Parser)]
#[command(
    name = "poe-mod",
    version,
    about = "Create, pack and verify PoE2 mod packages"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new mod project from a template.
    New {
        /// client, server or both.
        kind: Kind,
        /// The mod id: [a-z0-9][a-z0-9_-]{1,63}.
        id: String,
        /// Where to create it (default: ./<id>).
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Author name for mod.toml.
        #[arg(long, default_value = "Your Name")]
        author: String,
        /// Depend on a local checkout of the SDK instead of the pinned release (for SDK development).
        #[arg(long, value_name = "PATH")]
        sdk_path: Option<PathBuf>,
        /// Overwrite files in a non-empty directory.
        #[arg(long)]
        force: bool,
    },
    /// Validate a mod folder's mod.toml and list what a package needs.
    Check {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Write <id>-<version>.zip and its .sha256 from a mod folder, reproducibly, then verify it.
    Pack {
        /// The mod folder (with mod.toml).
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Where to write the package (default: the current directory).
        #[arg(long, default_value = ".")]
        out: PathBuf,
        /// Put FILE into the package at PATH (overrides the mod folder): PATH=FILE.
        #[arg(long, value_name = "PATH=FILE")]
        add: Vec<String>,
        /// Look for files the manifest names (libraries, DLLs) by file name in this folder
        /// (a cargo build folder). Repeatable.
        #[arg(long, value_name = "DIR")]
        artifacts: Vec<PathBuf>,
    },
    /// Verify a package against its .sha256 and the package rules.
    Verify {
        /// The .zip (its .sha256 must sit next to it).
        zip: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::New {
            kind,
            id,
            dir,
            author,
            sdk_path,
            force,
        } => new_project(&NewOptions {
            kind,
            dir: dir.unwrap_or_else(|| PathBuf::from(&id)),
            id,
            author,
            sdk: sdk_path.map_or_else(SdkSource::release, SdkSource::Path),
            force,
        }),
        Command::Check { dir } => check_dir(&dir),
        Command::Pack {
            dir,
            out,
            add,
            artifacts,
        } => parse_adds(&add).and_then(|add| {
            pack(&PackOptions {
                mod_dir: dir,
                out_dir: out,
                add,
                artifact_dirs: artifacts,
            })
        }),
        Command::Verify { zip } => verify_zip(&zip),
    };
    match result {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for e in errors {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

fn parse_adds(raw: &[String]) -> Result<Vec<(String, PathBuf)>, Vec<String>> {
    raw.iter()
        .map(|a| match a.split_once('=') {
            Some((path, file)) if !path.is_empty() && !file.is_empty() => {
                Ok((path.to_owned(), PathBuf::from(file)))
            }
            _ => Err(vec![format!("--add {a:?}: expected PATH=FILE")]),
        })
        .collect()
}
