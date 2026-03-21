#![deny(clippy::unwrap_used)]

use clap::Parser;
use eyre::{Context, Result};
use std::process;

mod classify;
mod cli;
mod config;
mod detect;
mod outline;
mod output;
mod skeleton;

use cli::{Cli, Layer};
use config::Config;
use detect::detect_languages;
use outline::{VisibilityFilter, build_outline};
use output::{check_budget, resolve_format, serialize};
use skeleton::build_skeleton;

fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = Config::load(cli.config.as_ref()).context("Failed to load configuration")?;

    let target = cli.effective_path();
    let target = if target.is_relative() {
        std::env::current_dir()?.join(target)
    } else {
        target.to_path_buf()
    };

    if !target.is_dir() {
        eprintln!("error: {} is not a directory", target.display());
        process::exit(2);
    }

    // Detect languages (or use CLI override)
    let detected_languages = if cli.langs.is_empty() {
        detect_languages(&target, &config)
    } else {
        cli.langs.clone()
    };

    // Resolve budget
    let budget = cli.budget.unwrap_or(config.defaults.budget);

    // Resolve output format
    let format = resolve_format(cli.format.as_deref(), &config.defaults.format);

    match &cli.layer {
        None | Some(Layer::Skeleton { .. }) => {
            let result = build_skeleton(
                &target,
                &config,
                &detected_languages,
                &cli.kinds,
                &cli.pattern,
                &cli.exclude,
                cli.hidden,
            )?;

            let total_lines = skeleton::count_output_lines(&result);

            if let Err(exceeded) = check_budget(total_lines, budget) {
                eprintln!("{exceeded}");
                process::exit(1);
            }

            let output = serialize(&result, format)?;
            print!("{output}");
        }
        Some(Layer::Outline { public, private, .. }) => {
            let vis_filter = match (public, private) {
                (true, false) => VisibilityFilter::Public,
                (false, true) => VisibilityFilter::Private,
                _ => VisibilityFilter::All,
            };

            let result = build_outline(
                &target,
                &config,
                &detected_languages,
                &cli.kinds,
                &cli.pattern,
                &cli.exclude,
                vis_filter,
                cli.hidden,
            )?;

            let total_lines = outline::count_output_lines(&result);

            if let Err(exceeded) = check_budget(total_lines, budget) {
                eprintln!("{exceeded}");
                process::exit(1);
            }

            let output = serialize(&result, format)?;
            print!("{output}");
        }
    }

    Ok(())
}
