use std::path::PathBuf;

use aios_kernel::KernelBuilder;
use aios_protocol::{Capability, PolicySet, ToolCall};
use anyhow::Result;
use clap::Parser;
use serde_json::json;
use tracing::{info, warn};

#[derive(Debug, Parser)]
#[command(name = "aiosd")]
#[command(about = "aiOS kernel demo daemon")]
struct Cli {
    #[arg(long, default_value = ".aios")]
    root: PathBuf,
    #[arg(long, default_value = "developer")]
    owner: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .compact()
        .init();

    let cli = Cli::parse();

    let kernel = KernelBuilder::new(&cli.root)
        .allowed_commands(vec!["echo".to_owned(), "git".to_owned()])
        .build();

    let policy = PolicySet {
        allow_capabilities: vec![
            Capability::fs_read("/session/**"),
            Capability::fs_write("/session/**"),
            Capability::exec("*"),
        ],
        gate_capabilities: vec![Capability::new("payments:initiate")],
        max_tool_runtime_secs: 20,
        max_events_per_turn: 512,
    };

    let session = kernel.create_session(cli.owner, policy, None).await?;
    info!(session_id = %session.session_id, workspace = %session.workspace_root, "session created");

    let mut events = kernel.subscribe_events();
    let event_task = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            let rendered = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_owned());
            info!(event = %rendered, "event.appended");
        }
    });

    let write_call = ToolCall::new(
        "fs.write",
        json!({
            "path": "artifacts/reports/bootstrap.txt",
            "content": "aiOS kernel bootstrap successful"
        }),
        vec![Capability::fs_write("/session/artifacts/**")],
    );

    let write_tick = kernel
        .tick(
            &session.session_id,
            "Bootstrap workspace with first artifact",
            Some(write_call),
        )
        .await?;
    info!(mode = ?write_tick.mode, progress = write_tick.state.progress, "tick complete");

    let shell_call = ToolCall::new(
        "shell.exec",
        json!({
            "command": "echo",
            "args": ["hello from aiOS"]
        }),
        vec![Capability::exec("echo")],
    );

    let shell_tick = kernel
        .tick(
            &session.session_id,
            "Run a bounded shell command",
            Some(shell_call),
        )
        .await?;
    info!(mode = ?shell_tick.mode, uncertainty = shell_tick.state.uncertainty, "tick complete");

    let read_call = ToolCall::new(
        "fs.read",
        json!({ "path": "artifacts/reports/bootstrap.txt" }),
        vec![Capability::fs_read("/session/artifacts/**")],
    );

    let read_tick = kernel
        .tick(
            &session.session_id,
            "Read generated artifact for verification",
            Some(read_call),
        )
        .await?;

    info!(
        mode = ?read_tick.mode,
        progress = read_tick.state.progress,
        remaining_tools = read_tick.state.budget.tool_calls_remaining,
        "tick complete"
    );

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    event_task.abort();
    if let Err(error) = event_task.await {
        warn!(%error, "event task stopped");
    }

    Ok(())
}
