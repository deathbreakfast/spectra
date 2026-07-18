//! Spectra matrix performance CLI (BM-S*).

mod capacity;
mod cli;
mod experiments;
mod report;
mod run;
mod stats;
mod sweep;
mod workload;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use sweep::SweepCli;

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Experiments => {
            run::list_experiments();
        }
        Command::Run {
            experiment,
            storage,
            topology,
            telemetry,
            report,
            prefill,
            prefill_sweep,
            query_iters,
            duration_secs,
            concurrency,
            bench_clients,
            batch_max,
        } => {
            run::run_experiment(run::RunArgs {
                experiment,
                matrix: cli::CliMatrix {
                    storage,
                    topology,
                    telemetry,
                },
                sweep: SweepCli {
                    prefill,
                    prefill_sweep,
                    query_iters,
                    duration_secs,
                    concurrency,
                    bench_clients,
                    batch_max,
                },
                report,
            })
            .await?;
        }
    }
    Ok(())
}
