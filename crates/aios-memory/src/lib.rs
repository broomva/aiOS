use std::path::{Path, PathBuf};

use aios_model::{
    EventKind, EventRecord, FileProvenance, Observation, Provenance, SessionId, SoulProfile,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn load_soul(&self, session_id: SessionId) -> Result<SoulProfile>;
    async fn save_soul(&self, session_id: SessionId, soul: &SoulProfile) -> Result<()>;
    async fn append_observation(
        &self,
        session_id: SessionId,
        observation: &Observation,
    ) -> Result<()>;
    async fn list_observations(
        &self,
        session_id: SessionId,
        limit: usize,
    ) -> Result<Vec<Observation>>;
}

#[derive(Debug, Clone)]
pub struct WorkspaceMemoryStore {
    root: PathBuf,
}

impl WorkspaceMemoryStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn session_memory_dir(&self, session_id: SessionId) -> PathBuf {
        self.root
            .join(session_id.0.hyphenated().to_string())
            .join("memory")
    }

    fn soul_path(&self, session_id: SessionId) -> PathBuf {
        self.session_memory_dir(session_id).join("soul.json")
    }

    fn observations_path(&self, session_id: SessionId) -> PathBuf {
        self.session_memory_dir(session_id)
            .join("observations.jsonl")
    }

    async fn ensure_parent(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl MemoryStore for WorkspaceMemoryStore {
    async fn load_soul(&self, session_id: SessionId) -> Result<SoulProfile> {
        let path = self.soul_path(session_id);
        if !fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(SoulProfile::default());
        }

        let raw = fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed reading soul file {path:?}"))?;
        let soul = serde_json::from_str(&raw)
            .with_context(|| format!("failed parsing soul file {path:?}"))?;
        Ok(soul)
    }

    async fn save_soul(&self, session_id: SessionId, soul: &SoulProfile) -> Result<()> {
        let path = self.soul_path(session_id);
        Self::ensure_parent(&path).await?;

        let payload = serde_json::to_string_pretty(soul)?;
        fs::write(&path, payload)
            .await
            .with_context(|| format!("failed writing soul file {path:?}"))?;
        Ok(())
    }

    async fn append_observation(
        &self,
        session_id: SessionId,
        observation: &Observation,
    ) -> Result<()> {
        let path = self.observations_path(session_id);
        Self::ensure_parent(&path).await?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .with_context(|| format!("failed opening observation log {path:?}"))?;
        let line = serde_json::to_string(observation)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        Ok(())
    }

    async fn list_observations(
        &self,
        session_id: SessionId,
        limit: usize,
    ) -> Result<Vec<Observation>> {
        let path = self.observations_path(session_id);
        if !fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Vec::new());
        }

        let file = OpenOptions::new().read(true).open(&path).await?;
        let mut reader = BufReader::new(file).lines();
        let mut observations = Vec::new();

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let observation: Observation = serde_json::from_str(&line)?;
            observations.push(observation);
        }

        observations.reverse();
        observations.truncate(limit);
        observations.reverse();

        Ok(observations)
    }
}

pub fn extract_observation(event: &EventRecord) -> Option<Observation> {
    let text = match &event.kind {
        EventKind::ToolCallCompleted { outcome, .. } => {
            format!("tool call completed: {outcome:?}")
        }
        EventKind::ErrorRaised { message } => format!("error observed: {message}"),
        EventKind::CheckpointCreated { checkpoint_id, .. } => {
            format!("checkpoint created: {}", checkpoint_id.0)
        }
        _ => return None,
    };

    Some(Observation {
        observation_id: Uuid::new_v4(),
        created_at: event.timestamp,
        text,
        tags: vec!["auto".to_owned()],
        provenance: Provenance {
            event_start: event.sequence,
            event_end: event.sequence,
            files: vec![FileProvenance {
                path: format!("events/{}.jsonl", event.session_id.0.hyphenated()),
                sha256: "pending".to_owned(),
            }],
        },
    })
}
