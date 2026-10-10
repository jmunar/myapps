//! Prepare a Challenges dataset on a workstation, into a bundle the server
//! loads with `myapps import --app challenges --dataset <file>`.
//!
//! Everything too heavy for the server happens here; the bundle format, and
//! the checks the server applies to it, are in `myapps_challenges::bundle`.

mod fetch;
mod source;

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;

use myapps_challenges::bundle;
use myapps_challenges::dataset::Dataset;
use source::Skip;

#[derive(Parser)]
#[command(about = "Prepare a Challenges dataset bundle for `myapps import`")]
struct Cli {
    /// Dataset key (ugphysics, hendrycks-math)
    dataset: String,
    /// Where to write the bundle [default: <dataset>.sqlite]
    #[arg(long, short)]
    out: Option<PathBuf>,
}

#[derive(Default)]
struct Counts {
    kept: usize,
    no_level: usize,
    diagram: usize,
    malformed: usize,
    truncated: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let Some(dataset) = Dataset::from_key(&cli.dataset) else {
        let valid: Vec<&str> = Dataset::ALL.iter().map(|d| d.key()).collect();
        bail!(
            "Unknown dataset '{}'; expected one of: {}",
            cli.dataset,
            valid.join(", ")
        );
    };
    let out = cli
        .out
        .unwrap_or_else(|| PathBuf::from(format!("{}.sqlite", dataset.key())));

    let writer = bundle::Writer::create(
        &out,
        dataset,
        &dataset.url(),
        dataset.license(),
        concat!("myapps-challenges-prep ", env!("CARGO_PKG_VERSION")),
    )
    .await?;

    let client = fetch::client()?;
    let mut counts = Counts::default();
    for (config, split) in source::sources(dataset) {
        let mut offset = 0;
        loop {
            let page = fetch::page(&client, dataset, config, split, offset).await?;
            let mut problems = Vec::with_capacity(page.rows.len());
            for entry in page.rows {
                if !entry.truncated_cells.is_empty() {
                    tracing::warn!(
                        "{config}/{split} row {}: truncated cells {:?}, skipped",
                        entry.row_idx,
                        entry.truncated_cells
                    );
                    counts.truncated += 1;
                    continue;
                }
                match source::map_row(dataset, config, split, entry.row_idx, &entry.row) {
                    Ok(p) => problems.push(p),
                    Err(Skip::NoLevel) => counts.no_level += 1,
                    Err(Skip::Diagram) => counts.diagram += 1,
                    Err(Skip::Malformed) => {
                        tracing::warn!(
                            "{config}/{split} row {}: malformed, skipped",
                            entry.row_idx
                        );
                        counts.malformed += 1;
                    }
                }
            }
            counts.kept += problems.len();
            writer.insert(&problems).await?;

            offset += fetch::PAGE;
            if offset >= page.num_rows_total {
                break;
            }
        }
        tracing::info!("{}: {config}/{split} done", dataset.name());
    }
    writer.finish().await?;

    tracing::info!(
        "{}: {} problems written to {}; skipped {} without a level, {} with a diagram, \
         {} malformed, {} truncated",
        dataset.name(),
        counts.kept,
        out.display(),
        counts.no_level,
        counts.diagram,
        counts.malformed,
        counts.truncated,
    );
    Ok(())
}
