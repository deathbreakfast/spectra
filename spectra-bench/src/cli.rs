use clap::{Parser, Subcommand, ValueEnum};
use spectra_testkit::{MatrixSpec, StorageAdapter, TelemetryAdapter, Topology, TransportAdapter};

#[derive(Debug, Clone, ValueEnum)]
pub enum CliStorage {
    Mem,
    Sqlite,
    Tensorbase,
    Clickhouse,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum CliTopology {
    Embedded,
    RemoteIngest,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum CliTelemetry {
    Off,
    ConsoleNdjson,
}

#[derive(Debug, Clone)]
pub struct CliMatrix {
    pub storage: CliStorage,
    pub topology: CliTopology,
    pub telemetry: CliTelemetry,
}

impl CliMatrix {
    pub fn to_matrix_spec(&self) -> MatrixSpec {
        let storage = match self.storage {
            CliStorage::Mem => StorageAdapter::Mem,
            CliStorage::Sqlite => StorageAdapter::Sqlite,
            CliStorage::Tensorbase => StorageAdapter::TensorBase,
            CliStorage::Clickhouse => StorageAdapter::ClickHouse,
        };
        let topology = match self.topology {
            CliTopology::Embedded => Topology::Embedded,
            CliTopology::RemoteIngest => Topology::RemoteIngest,
        };
        MatrixSpec {
            storage,
            transport: TransportAdapter::Direct,
            telemetry: match self.telemetry {
                CliTelemetry::Off => TelemetryAdapter::Off,
                CliTelemetry::ConsoleNdjson => TelemetryAdapter::ConsoleNdjson,
            },
            topology,
            persist_enabled: true,
        }
    }
}

#[derive(Parser)]
#[command(name = "spectra-bench", about = "Spectra matrix performance CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List benchmark experiment IDs.
    Experiments,
    /// Run one experiment against a matrix slice.
    Run {
        #[arg(long)]
        experiment: String,
        #[arg(long, value_enum, default_value_t = CliStorage::Mem)]
        storage: CliStorage,
        #[arg(long, value_enum, default_value_t = CliTopology::Embedded)]
        topology: CliTopology,
        #[arg(long, value_enum, default_value_t = CliTelemetry::Off)]
        telemetry: CliTelemetry,
        #[arg(long)]
        report: Option<std::path::PathBuf>,
        #[arg(long, help = "Prefill depth for query experiments")]
        prefill: Option<u64>,
        #[arg(
            long,
            help = "Comma-separated prefill depths (default 1000,10000,100000,1000000)"
        )]
        prefill_sweep: Option<String>,
        #[arg(long, help = "Timed query iterations (default 1000)")]
        query_iters: Option<u64>,
        #[arg(long, help = "Firehose duration in seconds (default 30)")]
        duration_secs: Option<u64>,
        #[arg(long, help = "Concurrent writer tasks (default 256)")]
        concurrency: Option<u32>,
        #[arg(long, help = "Multi-writer bench client count (BM-SW3)")]
        bench_clients: Option<u32>,
        #[arg(long, help = "L2 PersistConfig.batch_max (BM-SW7; default 32)")]
        batch_max: Option<usize>,
    },
}
