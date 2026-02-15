use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aios_model::{Capability, ToolCall, ToolOutcome, ToolRunId};
use aios_policy::{PolicyEngine, PolicyEvaluation};
use aios_sandbox::{SandboxLimits, SandboxRequest, SandboxRunner};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolKind {
    FsRead,
    FsWrite,
    ShellExec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub required_capabilities: Vec<Capability>,
    pub kind: ToolKind,
}

#[derive(Debug, Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn register(&mut self, definition: ToolDefinition) {
        self.tools.insert(definition.name.clone(), definition);
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &ToolDefinition> {
        self.tools.values()
    }

    pub fn with_core_tools() -> Self {
        let mut registry = Self::default();

        registry.register(ToolDefinition {
            name: "fs.read".to_owned(),
            description: "Read a UTF-8 text file from the session workspace".to_owned(),
            required_capabilities: vec![Capability::fs_read("/session/**")],
            kind: ToolKind::FsRead,
        });

        registry.register(ToolDefinition {
            name: "fs.write".to_owned(),
            description: "Write a UTF-8 text file to the session workspace".to_owned(),
            required_capabilities: vec![Capability::fs_write("/session/artifacts/**")],
            kind: ToolKind::FsWrite,
        });

        registry.register(ToolDefinition {
            name: "shell.exec".to_owned(),
            description: "Execute a constrained command through the sandbox runner".to_owned(),
            required_capabilities: vec![Capability::exec("*")],
            kind: ToolKind::ShellExec,
        });

        registry
    }
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionReport {
    pub tool_run_id: ToolRunId,
    pub tool_name: String,
    pub evaluation: PolicyEvaluation,
    pub exit_status: i32,
    pub outcome: ToolOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DispatchResult {
    Executed(ToolExecutionReport),
    NeedsApproval {
        tool_name: String,
        evaluation: PolicyEvaluation,
    },
}

#[derive(Clone)]
pub struct ToolDispatcher {
    registry: Arc<ToolRegistry>,
    policy: Arc<dyn PolicyEngine>,
    sandbox: Arc<dyn SandboxRunner>,
}

impl ToolDispatcher {
    pub fn new(
        registry: Arc<ToolRegistry>,
        policy: Arc<dyn PolicyEngine>,
        sandbox: Arc<dyn SandboxRunner>,
    ) -> Self {
        Self {
            registry,
            policy,
            sandbox,
        }
    }

    pub fn registry(&self) -> Arc<ToolRegistry> {
        self.registry.clone()
    }

    pub async fn dispatch(
        &self,
        session_id: aios_model::SessionId,
        context: &ToolContext,
        call: ToolCall,
    ) -> Result<DispatchResult> {
        let definition = self
            .registry
            .get(&call.tool_name)
            .with_context(|| format!("unknown tool: {}", call.tool_name))?
            .clone();

        let mut requested_capabilities = definition.required_capabilities.clone();
        requested_capabilities.extend(call.requested_capabilities.clone());

        let evaluation = self
            .policy
            .evaluate_capabilities(session_id, &requested_capabilities)
            .await;

        if !evaluation.denied.is_empty() {
            bail!("capabilities denied for tool {}", definition.name);
        }

        if !evaluation.requires_approval.is_empty() {
            return Ok(DispatchResult::NeedsApproval {
                tool_name: definition.name,
                evaluation,
            });
        }

        let tool_run_id = ToolRunId::new();
        let (exit_status, outcome) = match definition.kind {
            ToolKind::FsRead => self.execute_fs_read(context, &call.input).await?,
            ToolKind::FsWrite => self.execute_fs_write(context, &call.input).await?,
            ToolKind::ShellExec => self.execute_shell_exec(context, &call).await?,
        };

        Ok(DispatchResult::Executed(ToolExecutionReport {
            tool_run_id,
            tool_name: definition.name,
            evaluation,
            exit_status,
            outcome,
        }))
    }

    async fn execute_fs_read(
        &self,
        context: &ToolContext,
        input: &Value,
    ) -> Result<(i32, ToolOutcome)> {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .context("fs.read requires input.path")?;
        let absolute = canonical_session_path(&context.workspace_root, path)?;
        let content = fs::read_to_string(&absolute)
            .await
            .with_context(|| format!("failed reading file {absolute:?}"))?;
        Ok((
            0,
            ToolOutcome::Success {
                output: json!({ "path": path, "content": content }),
            },
        ))
    }

    async fn execute_fs_write(
        &self,
        context: &ToolContext,
        input: &Value,
    ) -> Result<(i32, ToolOutcome)> {
        let path = input
            .get("path")
            .and_then(Value::as_str)
            .context("fs.write requires input.path")?;
        let content = input
            .get("content")
            .and_then(Value::as_str)
            .context("fs.write requires input.content")?;
        let absolute = canonical_session_path(&context.workspace_root, path)?;
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&absolute, content)
            .await
            .with_context(|| format!("failed writing file {absolute:?}"))?;
        Ok((
            0,
            ToolOutcome::Success {
                output: json!({ "path": path, "bytes": content.len() }),
            },
        ))
    }

    async fn execute_shell_exec(
        &self,
        context: &ToolContext,
        call: &ToolCall,
    ) -> Result<(i32, ToolOutcome)> {
        let command = call
            .input
            .get("command")
            .and_then(Value::as_str)
            .context("shell.exec requires input.command")?
            .to_owned();

        let args = call
            .input
            .get("args")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let execution = self
            .sandbox
            .run(SandboxRequest {
                command,
                args,
                cwd: context.workspace_root.clone(),
                env: Default::default(),
                required_capabilities: call.requested_capabilities.clone(),
                limits: SandboxLimits::default(),
            })
            .await?;

        let outcome = if execution.exit_code == 0 {
            ToolOutcome::Success {
                output: json!({
                    "stdout": execution.stdout,
                    "stderr": execution.stderr,
                    "duration_ms": execution.duration_ms,
                    "timed_out": execution.timed_out,
                }),
            }
        } else {
            ToolOutcome::Failure {
                error: format!(
                    "command failed (exit={}): {}",
                    execution.exit_code, execution.stderr
                ),
            }
        };

        Ok((execution.exit_code, outcome))
    }
}

fn canonical_session_path(root: &Path, relative_path: &str) -> Result<PathBuf> {
    let normalized = relative_path.trim_start_matches('/');
    let candidate = root.join(normalized);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let parent = candidate.parent().unwrap_or(root.as_path());

    if !parent.exists() {
        return Ok(candidate);
    }

    let canonical_parent = parent
        .canonicalize()
        .with_context(|| format!("failed canonicalizing parent {parent:?}"))?;

    if !canonical_parent.starts_with(&root) {
        bail!("path escapes workspace root: {relative_path}");
    }

    Ok(candidate)
}
