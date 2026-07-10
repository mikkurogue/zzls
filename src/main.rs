mod bridge;
mod config;
mod diagnostics;
mod document;
mod features;
mod server;
mod workspace;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tower_lsp_server::{LspService, Server};

use server::Backend;

#[derive(Parser)]
#[command(
    name = "zzls",
    about = "A fast Zig language server written in Rust",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the LSP server
    Server {
        /// Communication method
        #[arg(long, default_value = "stdio")]
        transport: Transport,

        /// Log level (trace, debug, info, warn, error)
        #[arg(long, default_value = "info")]
        log_level: String,

        /// Log file path (defaults to stderr)
        #[arg(long)]
        log_file: Option<PathBuf>,
    },

    /// Check a file for diagnostics (CLI mode)
    Check {
        /// Path to the file to check
        file: PathBuf,

        /// Path to zig binary
        #[arg(long)]
        zig: Option<PathBuf>,
    },

    /// Format a file using zig fmt
    Format {
        /// Path to the file to format
        file: PathBuf,

        /// Check only (don't modify, return exit code)
        #[arg(long)]
        check: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum Transport {
    Stdio,
    Tcp,
    Socket,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Commands::Server { transport, log_level, log_file }) => {
            init_logging(&log_level, log_file.as_deref());
            run_server(transport).await
        }
        Some(Commands::Check { file, zig }) => {
            run_check(&file, zig.as_deref()).await
        }
        Some(Commands::Format { file, check }) => {
            run_format(&file, check).await
        }
        None => {
            init_logging("info", None);
            run_server(Transport::Stdio).await
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn init_logging(level: &str, log_file: Option<&std::path::Path>) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true);

    if let Some(path) = log_file {
        let file = std::fs::File::create(path)
            .expect("Failed to create log file");
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer.with_writer(file))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer.with_writer(std::io::stderr))
            .init();
    }
}

async fn run_server(transport: Transport) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Starting ZZLS v{}", env!("CARGO_PKG_VERSION"));

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));

    match transport {
        Transport::Stdio => {
            Server::new(stdin, stdout, socket).serve(service).await;
        }
        _ => {
            tracing::warn!("TCP/Socket transport not yet implemented, using stdio");
            Server::new(stdin, stdout, socket).serve(service).await;
        }
    }

    Ok(())
}

async fn run_check(file: &std::path::Path, zig_path: Option<&std::path::Path>) -> Result<(), Box<dyn std::error::Error>> {
    let zig = zig_path
        .map(PathBuf::from)
        .or_else(|| which::which("zig").ok())
        .ok_or("zig not found in PATH")?;

    let compiler = bridge::ZigCompiler::new(zig);
    let diagnostics = compiler.check(file).await?;

    if diagnostics.is_empty() {
        tracing::info!("No errors found");
        std::process::exit(0);
    } else {
        diagnostics::print_diagnostics(&diagnostics, file)?;
        std::process::exit(1);
    }
}

async fn run_format(file: &std::path::Path, check_only: bool) -> Result<(), Box<dyn std::error::Error>> {
    let zig = which::which("zig")
        .map_err(|_| "zig not found in PATH")?;

    let formatter = bridge::ZigFormatter::new(zig);
    let result = formatter.format_file(file, check_only).await?;

    match result {
        bridge::FormatResult::Formatted => {
            tracing::info!("Formatted {}", file.display());
            std::process::exit(0)
        }
        bridge::FormatResult::NoChanges => {
            tracing::info!("No changes needed");
            std::process::exit(0)
        }
        bridge::FormatResult::CheckFailed { .. } => {
            std::process::exit(1)
        }
        bridge::FormatResult::Error(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1)
        }
    }
}
