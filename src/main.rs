mod actions;
mod client;
mod daemon;
mod protocol;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

use protocol::{Command as WireCommand, Forward, Notify, Open, Port, Vscode};

#[derive(Parser)]
#[command(name = "wezcmd", version, about = "Remote-to-local command socket")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Daemon(DaemonArgs),
    Probe { socket: PathBuf },
    Send(SendArgs),
}

#[derive(Args)]
struct DaemonArgs {
    #[arg(long)]
    socket: PathBuf,
    #[arg(long)]
    no_confirm_forward: bool,
}

#[derive(Args)]
struct SendArgs {
    #[arg(long)]
    socket: PathBuf,
    #[command(subcommand)]
    command: SendCommand,
}

#[derive(Subcommand)]
enum SendCommand {
    Open {
        #[arg(long)]
        url: String,
    },
    Notify {
        #[arg(long, default_value = "Notification")]
        title: String,
        #[arg(long)]
        body: String,
    },
    Vscode {
        #[arg(long)]
        path: String,
        #[arg(long, default_value = "")]
        host: String,
    },
    Forward {
        #[arg(long)]
        port: u16,
        #[arg(long, default_value = "")]
        host: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(code) => code,
        Err(err) => {
            eprintln!("wezcmd: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Daemon(args) => {
            daemon::serve(daemon::DaemonConfig {
                socket_path: args.socket,
                confirm_forward: !args.no_confirm_forward,
            })
            .await?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Probe { socket } => Ok(if client::probe(&socket).await {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }),
        Commands::Send(args) => {
            let command = send_command(args.command);
            command.validate()?;
            let response = client::send(&args.socket, &command).await?;
            println!("{}", serde_json::to_string(&response)?);
            Ok(if response.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            })
        }
    }
}

fn send_command(command: SendCommand) -> WireCommand {
    match command {
        SendCommand::Open { url } => WireCommand::Open(Open { url }),
        SendCommand::Notify { title, body } => WireCommand::Notify(Notify { title, body }),
        SendCommand::Vscode { path, host } => WireCommand::Vscode(Vscode { path, host }),
        SendCommand::Forward { port, host } => WireCommand::Forward(Forward {
            port: Port(port),
            host,
        }),
    }
}
