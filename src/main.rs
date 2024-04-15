//! # Incrementor
//! A simple agnostic version bumping tool.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use clap::{ArgGroup, Parser, ValueEnum};
use eyre::{eyre, Context, Result};
use figment::providers::{Format, Toml};
use regex::RegexBuilder;
use semver::Version;
use serde::Serialize;

use incrementor::{bump, Part, Placeholders};

use crate::config::{Config, FileConfig, WORKDIR_CONFIG_PATH};
use crate::git_operations::Git;

mod config;
mod git_operations;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum OutputFormat {
    Json,
    None,
}

#[derive(Debug, Clone, Serialize)]
struct FileOutput {
    contents: String,
}

#[derive(Debug, Clone, Serialize)]
struct Output<'a> {
    dry_run: bool,
    part: &'a Part,
    build_metadata: Option<String>,
    current_version: &'a Version,
    new_version: &'a Version,
    files: HashMap<&'a str, FileOutput>,
    git_tag: Option<String>,
    git_commit_message: Option<String>,
}

impl<'a> Output<'a> {
    fn print(&self, format: OutputFormat) {
        match format {
            OutputFormat::Json => {
                let json = serde_json::to_string_pretty(&self).unwrap();
                println!("{json}");
            }
            _ => {
                // Do nothing
            }
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(
    group(
        ArgGroup::new("part")
        .required(true)
        .args(["major", "minor", "patch", "prerelease", "release", "new_version"]),
    )
)]
#[command(
    group(
        ArgGroup::new("tag_group")
        .required(false)
        .multiple(false)
        .args(["tag", "no_tag"])
    )
)]
#[command(
    group(
        ArgGroup::new("commit_group")
        .required(false)
        .multiple(false)
        .args(["commit", "no_commit"])
    )
)]
struct Args {
    /// Config file
    #[arg(short = 'c', long)]
    config: Option<String>,

    /// Don't write any files, just pretend
    #[arg(short = 'd', long)]
    dry_run: bool,

    /// Increment prerelease
    #[arg(long)]
    prerelease: Option<String>,

    /// Increment patch
    #[arg(long)]
    patch: bool,

    /// Increment minor
    #[arg(long)]
    minor: bool,

    /// Increment major
    #[arg(long)]
    major: bool,

    /// Remove prerelease
    #[arg(long)]
    release: bool,

    /// Build metadata
    #[arg(long)]
    build: Option<String>,

    /// Use supplied new version
    #[arg(long)]
    new_version: Option<String>,

    /// Git Tag
    #[arg(long)]
    tag: bool,

    /// Do not tag
    #[arg(long)]
    no_tag: bool,

    /// Git commit
    #[arg(long)]
    commit: bool,

    /// Do not commit
    #[arg(long)]
    no_commit: bool,

    /// Allow dirty git index
    #[arg(long)]
    allow_dirty: bool,

    /// Git commit message
    #[arg(
        short = 'm',
        long,
        default_value = "bump {current_version} -> {new_version}"
    )]
    commit_message: String,

    #[arg(value_enum, short = 'o', long, default_value = "none")]
    output: OutputFormat,
}

/// Parse the part (minor, major etc.) from the arguments
fn parse_part_from_args(args: &Args) -> Part {
    match (
        &args.prerelease,
        &args.patch,
        &args.minor,
        &args.major,
        &args.release,
    ) {
        (Some(p), _, _, _, _) => Part::Prerelease(Some(p.clone())),
        (_, true, _, _, _) => Part::Patch,
        (_, _, true, _, _) => Part::Minor,
        (_, _, _, true, _) => Part::Major,
        (_, _, _, _, true) => Part::Prerelease(None),
        _ => Part::None,
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let args: Args = Args::parse();

    // Parse config from config path, or default to ./incrementor.toml
    let (mut config, config_path) = if let Some(ref path) = args.config {
        (Config::from(Toml::file(path.clone()))?, path.clone())
    } else {
        (
            Config::from(Config::figment())?,
            WORKDIR_CONFIG_PATH.to_string(),
        )
    };

    // Setup git related things
    let git_tag = (args.tag && !args.no_tag) || config.tag;
    let git_commit = (args.commit && !args.no_commit) || config.commit;
    let git_commit_message = config
        .commit_message
        .clone()
        .unwrap_or(args.commit_message.clone());
    let git = Git::new(args.allow_dirty)?;
    if (git_tag || git_commit) && git.is_dirty() {
        return Err(eyre!("Repository is dirty"));
    }

    // Parse part from arguments
    let part = parse_part_from_args(&args);

    // Create or use the new_version
    let current_version = config.current_version.clone();
    let maybe_new_version = args
        .new_version
        .map(|s| Version::parse(&s).expect("Invalid new_version"));
    let new_version = if let Some(version) = maybe_new_version {
        version
    } else {
        bump(&current_version, &part, args.build.clone())?
    };

    // Setup placeholders
    let placeholders = Placeholders {
        current_version: &current_version,
        new_version: &new_version,
    };

    // Setup output buffer
    let mut output = Output {
        dry_run: args.dry_run,
        build_metadata: args.build.clone(),
        part: &part,
        current_version: &current_version,
        new_version: &new_version,
        files: HashMap::new(),
        git_tag: None,
        git_commit_message: None,
    };

    for (file_path, file_config) in config.files.iter() {
        let content = fs::read_to_string(file_path)
            .context(format!("File {} not found", file_path.to_str().unwrap()))?;

        match replace_version(content, file_path, file_config, &placeholders) {
            Ok(result) => {
                output.files.insert(
                    file_path.to_str().unwrap(),
                    FileOutput {
                        contents: result.clone(),
                    },
                );
                if !args.dry_run {
                    fs::write(file_path, result)?
                }
            }
            Err(err) => {
                return Err(err);
            }
        }
    }

    // Finalize and write the config with the `new_version` as `current_version`
    config.current_version = new_version.clone();
    if !args.dry_run {
        let content = toml::to_string_pretty(&config)?;
        fs::write(config_path, content)?;
    }

    if git_commit && !args.dry_run {
        let message = placeholders.replace(&git_commit_message);
        git.commit(&message)?;
        output.git_commit_message = Some(message);
    }

    if git_tag && !args.dry_run {
        let tag = format!("v{new_version}");
        git.tag(&tag, &tag)?;
        output.git_tag = Some(tag);
    }

    output.print(args.output);

    Ok(())
}

fn replace_version(
    content: String,
    file_path: &PathBuf,
    file_config: &FileConfig,
    placeholders: &Placeholders,
) -> Result<String> {
    let search_re = RegexBuilder::new(
        &placeholders
            .replace(&file_config.search)
            // Escape + sign
            .replace('+', "\\+"),
    )
    .multi_line(true)
    .build()?;

    let replace_value = placeholders.replace(&file_config.replace);

    if search_re.is_match(&content) {
        Ok(search_re.replace_all(&content, replace_value).to_string())
    } else {
        Err(eyre!(
            "Unable to find current version ({:?}) in {:?}",
            search_re,
            file_path
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{parse_part_from_args, replace_version, Args, FileConfig, OutputFormat};
    use incrementor::{Part, Placeholders};
    use semver::Version;
    use std::path::Path;

    #[test]
    fn test_parse_part() {
        struct TestCase {
            args: Args,
            part: Part,
        }

        let cases = vec![
            TestCase {
                args: Args {
                    config: None,
                    dry_run: false,
                    prerelease: None,
                    patch: false,
                    minor: false,
                    major: false,
                    release: false,
                    build: None,
                    new_version: None,
                    tag: false,
                    no_tag: false,
                    commit: false,
                    no_commit: false,
                    allow_dirty: false,
                    commit_message: "".to_string(),
                    output: OutputFormat::Json,
                },
                part: Part::None,
            },
            TestCase {
                args: Args {
                    config: None,
                    dry_run: false,
                    prerelease: None,
                    patch: false,
                    minor: false,
                    major: true,
                    release: false,
                    build: None,
                    new_version: None,
                    tag: false,
                    no_tag: false,
                    commit: false,
                    no_commit: false,
                    allow_dirty: false,
                    commit_message: "".to_string(),
                    output: OutputFormat::Json,
                },
                part: Part::Major,
            },
            TestCase {
                args: Args {
                    config: None,
                    dry_run: false,
                    prerelease: None,
                    patch: false,
                    minor: true,
                    major: false,
                    release: false,
                    build: None,
                    new_version: None,
                    tag: false,
                    no_tag: false,
                    commit: false,
                    no_commit: false,
                    allow_dirty: false,
                    commit_message: "".to_string(),
                    output: OutputFormat::Json,
                },
                part: Part::Minor,
            },
            TestCase {
                args: Args {
                    config: None,
                    dry_run: false,
                    prerelease: None,
                    patch: true,
                    minor: false,
                    major: false,
                    release: false,
                    build: None,
                    new_version: None,
                    tag: false,
                    no_tag: false,
                    commit: false,
                    no_commit: false,
                    allow_dirty: false,
                    commit_message: "".to_string(),
                    output: OutputFormat::Json,
                },
                part: Part::Patch,
            },
            TestCase {
                args: Args {
                    config: None,
                    dry_run: false,
                    prerelease: Some("beta".to_string()),
                    patch: false,
                    minor: false,
                    major: false,
                    release: false,
                    build: None,
                    new_version: None,
                    tag: false,
                    no_tag: false,
                    commit: false,
                    no_commit: false,
                    allow_dirty: false,
                    commit_message: "".to_string(),
                    output: OutputFormat::Json,
                },
                part: Part::Prerelease(Some("beta".to_string())),
            },
            TestCase {
                args: Args {
                    config: None,
                    dry_run: false,
                    prerelease: None,
                    patch: false,
                    minor: false,
                    major: false,
                    release: true,
                    build: None,
                    new_version: None,
                    tag: false,
                    no_tag: false,
                    commit: false,
                    no_commit: false,
                    allow_dirty: false,
                    commit_message: "".to_string(),
                    output: OutputFormat::Json,
                },
                part: Part::Prerelease(None),
            },
        ];

        for case in cases {
            let part = parse_part_from_args(&case.args);
            assert_eq!(part, case.part)
        }
    }

    #[test]
    fn test_replace() {
        let placeholders = Placeholders {
            current_version: &Version::new(0, 1, 0),
            new_version: &Version::new(0, 2, 0),
        };

        let content = r#"
            version = "0.1.0"

            [dependencies]
            some-dep = { version = "0.1.0" }
        "#;

        let file_config = FileConfig {
            search: "version = \"{current_version}\"$".to_string(),
            replace: "version = \"{new_version}\"".to_string(),
        };

        let file_path = Path::new("Cargo.toml");

        let res = replace_version(
            content.to_string(),
            &file_path.to_path_buf(),
            &file_config,
            &placeholders,
        )
        .unwrap();

        assert_eq!(
            res,
            r#"
            version = "0.2.0"

            [dependencies]
            some-dep = { version = "0.1.0" }
        "#
        );
    }

    #[test]
    fn test_failure() {
        let placeholders = Placeholders {
            current_version: &Version::new(0, 1, 0),
            new_version: &Version::new(0, 2, 0),
        };

        let content = r#"
            version = "0.2.0"

            [dependencies]
            some-dep = { version = "0.1.0" }
        "#;

        let file_config = FileConfig {
            search: "version = \"{current_version}\"$".to_string(),
            replace: "version = \"{new_version}\"".to_string(),
        };

        let file_path = Path::new("Cargo.toml");

        let res = replace_version(
            content.to_string(),
            &file_path.to_path_buf(),
            &file_config,
            &placeholders,
        );

        assert!(res.is_err());
    }
}
