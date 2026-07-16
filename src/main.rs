mod actions;
mod client;
mod daemon;
mod protocol;
mod proxy;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

use protocol::{
    Command as WireCommand, Forward, Notify, Open, Port, ProxyListen, ProxyStop, Vscode,
};

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
    ProxyWorker(ProxyWorkerArgs),
    ProxyListen(ProxyListenArgs),
    ProxyStop(ProxyStopArgs),
}

#[derive(Args)]
struct DaemonArgs {
    #[arg(long)]
    socket: PathBuf,
    #[arg(long)]
    enable_proxy: bool,
}

#[derive(Args)]
struct SendArgs {
    #[arg(long)]
    socket: PathBuf,
    #[command(subcommand)]
    command: SendCommand,
}

#[derive(Args)]
struct ProxyWorkerArgs {
    #[arg(long)]
    socket: PathBuf,
    #[arg(long)]
    session: String,
    #[arg(long)]
    token: String,
    #[arg(long, default_value = "127.0.0.1")]
    remote_host: String,
}

#[derive(Args)]
struct ProxyListenArgs {
    #[arg(long)]
    socket: PathBuf,
    #[arg(long)]
    session: String,
    #[arg(long)]
    token: String,
    #[arg(long)]
    local_port: u16,
    #[arg(long)]
    remote_port: u16,
}

#[derive(Args)]
struct ProxyStopArgs {
    #[arg(long)]
    socket: PathBuf,
    #[arg(long)]
    session: String,
    #[arg(long)]
    token: String,
    #[arg(long)]
    local_port: u16,
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
                enable_proxy: args.enable_proxy,
            })
            .await?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Probe { socket } => Ok(if client::probe(&socket).await {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }),
        Commands::Send(args) => send(&args.socket, send_command(args.command)).await,
        Commands::ProxyWorker(args) => {
            proxy::worker(&args.socket, args.session, args.token, args.remote_host).await?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::ProxyListen(args) => {
            send(
                &args.socket,
                WireCommand::ProxyListen(ProxyListen {
                    session: args.session,
                    token: args.token,
                    local_port: Port(args.local_port),
                    remote_port: Port(args.remote_port),
                }),
            )
            .await
        }
        Commands::ProxyStop(args) => {
            send(
                &args.socket,
                WireCommand::ProxyStop(ProxyStop {
                    session: args.session,
                    token: args.token,
                    local_port: Port(args.local_port),
                }),
            )
            .await
        }
    }
}

async fn send(socket: &std::path::Path, command: WireCommand) -> Result<ExitCode> {
    command.validate()?;
    let response = client::send(socket, &command).await?;
    println!("{}", serde_json::to_string(&response)?);
    Ok(if response.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    })
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
