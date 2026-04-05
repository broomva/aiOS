use std::collections::BTreeMap;
use std::path::PathBuf;

use aios_protocol::Capability;
use anyhow::{Result, bail};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::{Duration, timeout};
use tracing::{debug, instrument, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxLimits {
    pub max_runtime_secs: u64,
    pub max_output_bytes: usize,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            max_runtime_secs: 30,
            max_output_bytes: 64 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxRequest {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub required_capabilities: Vec<Capability>,
    pub limits: SandboxLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxExecution {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

#[async_trait]
pub trait SandboxRunner: Send + Sync {
    async fn run(&self, request: SandboxRequest) -> Result<SandboxExecution>;
}

#[derive(Debug, Clone)]
pub struct LocalSandboxRunner {
    pub allowed_commands: Vec<String>,
}

impl LocalSandboxRunner {
    pub fn new(allowed_commands: Vec<String>) -> Self {
        Self { allowed_commands }
    }

    fn command_allowed(&self, command: &str) -> bool {
        self.allowed_commands.is_empty() || self.allowed_commands.iter().any(|c| c == command)
    }

    fn truncate(output: Vec<u8>, max_output_bytes: usize) -> String {
        let mut output = output;
        if output.len() > max_output_bytes {
            output.truncate(max_output_bytes);
        }
        String::from_utf8_lossy(&output).into_owned()
    }
}

#[async_trait]
impl SandboxRunner for LocalSandboxRunner {
    #[instrument(
        skip(self, request),
        fields(
            command = %request.command,
            args_count = request.args.len(),
            cwd = %request.cwd.display()
        )
    )]
    async fn run(&self, request: SandboxRequest) -> Result<SandboxExecution> {
        if !self.command_allowed(&request.command) {
            bail!("command not allowed in sandbox: {}", request.command);
        }

        let started_at = Utc::now();

        let mut command = Command::new(&request.command);
        command.args(&request.args);
        command.current_dir(&request.cwd);
        command.env_clear();
        command.envs(&request.env);

        let output_future = command.output();
        let limit = Duration::from_secs(request.limits.max_runtime_secs.max(1));

        match timeout(limit, output_future).await {
            Ok(output_result) => {
                let output = output_result?;
                let ended_at = Utc::now();
                let stdout = Self::truncate(output.stdout, request.limits.max_output_bytes);
                let stderr = Self::truncate(output.stderr, request.limits.max_output_bytes);
                let execution = SandboxExecution {
                    started_at,
                    ended_at,
                    duration_ms: (ended_at - started_at).num_milliseconds(),
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout,
                    stderr,
                    timed_out: false,
                };
                debug!(
                    exit_code = execution.exit_code,
                    duration_ms = execution.duration_ms,
                    "sandbox command finished"
                );
                Ok(execution)
            }
            Err(_) => {
                let ended_at = Utc::now();
                warn!(
                    max_runtime_secs = request.limits.max_runtime_secs,
                    "sandbox command timed out"
                );
                Ok(SandboxExecution {
                    started_at,
                    ended_at,
                    duration_ms: (ended_at - started_at).num_milliseconds(),
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: format!(
                        "sandbox timeout after {} seconds",
                        request.limits.max_runtime_secs
                    ),
                    timed_out: true,
                })
            }
        }
    }
}
