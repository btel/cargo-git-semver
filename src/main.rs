extern crate clap;
extern crate git2;
extern crate regex;
extern crate semver;

use clap::{Parser, Subcommand};
use git2::{DescribeFormatOptions, DescribeOptions, Repository, Status, StatusOptions};
use regex::Regex;
use semver::{BuildMetadata, Prerelease, Version};
use std::{
    fs,
    io::{self, Read, Write},
};

#[derive(Parser, Debug)]
#[command(name = "git-semver")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Bump cargo version from latest tag
    Bump {
        path: Vec<String>,
        #[arg(long, action)]
        dry_run: bool,
    },
    /// Check if last release was tagged
    CheckTags {},
}

fn replace_version(path: &str, ver: &str) -> Result<(), String> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let re = Regex::new(r#"(?m)^version = ".+""#).unwrap();
            let replaced = re
                .replace(&contents, format!(r#"version = "{}""#, ver))
                .into_owned();
            match fs::write(path, replaced) {
                Ok(_) => Ok(()),
                Err(err) => Err(format!("Error writing `{}`: {}", path, err)),
            }
        }
        Err(err) => Err(format!("Error reading `{}`: {}", path, err)),
    }
}

fn parse_cargo_version(contents: &str) -> Result<Version, String> {
    let re = Regex::new(r#"(?m)^version = "(.+)""#).unwrap();
    let ver_captures = re
        .captures_iter(contents)
        .next()
        .ok_or(String::from("version number not found"))?;
    let version = &ver_captures[1];

    Version::parse(version).or(Err(format!(
        "error parsing version from Cargo.toml {}",
        version
    )))
}

fn get_cargo_version(path: &str) -> Result<Version, String> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_cargo_version(&contents),
        Err(err) => Err(format!("Error reading `{}`: {}", path, err)),
    }
}

fn open_repository(path: &str) -> Result<Repository, String> {
    match Repository::discover(path) {
        Ok(repo) => Ok(repo),
        Err(err) => Err(format!("Error openning repository: {}", err)),
    }
}

fn get_latest_tag(repo: &Repository) -> Result<Version, String> {
    let mut opts = DescribeOptions::new();
    let opts = opts.describe_tags();

    let mut format_opts = DescribeFormatOptions::new();
    let format_opts = format_opts.abbreviated_size(0);

    let version_str = repo
        .describe(&opts)
        .or(Err(format!("could not get tag")))?
        .format(Some(&format_opts))
        .unwrap();

    let version_number = if (version_str.chars().next().unwrap() == 'v') {
        &version_str[1..]
    } else {
        &version_str
    };
    Version::parse(version_number).or(Err(format!(
        "error parsing version from git tag {}",
        version_str
    )))
}

fn run_sem_ver(paths: &Vec<String>, dry_run: bool) -> Result<(), String> {
    let path = String::from("Cargo.toml");

    let repo = open_repository(&path)?;

    // let clean_dir = check_if_repo_clean(paths, &repo)?;

    let sem_ver = get_latest_tag(&repo)?;
    let cargo_ver = get_cargo_version(&path)?;
    if cargo_ver <= sem_ver {
        let new_version = Version {
            major: sem_ver.major,
            minor: sem_ver.minor + 1,
            patch: 0,
            pre: Prerelease::new("dev.1").unwrap(),
            build: BuildMetadata::EMPTY,
        };
        if (dry_run) {
            println!("Created version number {} (dry-run)", new_version);
            Ok(())
        } else {
            println!("Created version number {}", new_version);
            replace_version(&path, &format!("{}", new_version))
        }
    } else {
        println!("Version number {} is up-to-date", cargo_ver);
        Ok(())
    }
}

fn run_check_tags() -> Result<(), String> {
    let path = String::from(".");
    let repo = open_repository(&path)?;
    let obj = repo.revparse_single(&"HEAD:Cargo.toml").unwrap();
    let blob = obj.as_blob().unwrap();
    let mut content = String::new();
    blob.content()
        .read_to_string(&mut content)
        .or(Err(format!("Error reading file from index.")))?;
    let cargo_version = parse_cargo_version(&content)?;
    let sem_ver = get_latest_tag(&repo)?;
    if cargo_version.pre.is_empty() {
        if sem_ver < cargo_version {
            return Err(format!(
                "Please tag the release commit before adding new changes."
            ));
        }
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Bump { path, dry_run } => run_sem_ver(&path, dry_run),
        Commands::CheckTags {} => run_check_tags(),
    };

    let exit_code = match result {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{}", err);
            1
        }
    };
    std::process::exit(exit_code);
}
