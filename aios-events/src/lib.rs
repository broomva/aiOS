use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aios_protocol::{
    BranchId, EventRecord, EventRecordStream, EventStorePort, KernelError, SessionId,
};
use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use parking_lot::Mutex;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, instrument, warn};

fn to_kernel_error(error: anyhow::Error) -> KernelError {
    KernelError::Runtime(error.to_string())
}

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, event: &EventRecord) -> Result<()>;
    async fn read_from(
        &self,
        session_id: SessionId,
        branch_id: Option<BranchId>,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>>;
    async fn latest_sequence(
        &self,
        session_id: SessionId,
        branch_id: Option<BranchId>,
    ) -> Result<u64>;
}

#[derive(Debug)]
pub struct FileEventStore {
    root: PathBuf,
    write_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    sequence_cache: Mutex<HashMap<(String, String), u64>>,
}

impl FileEventStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            write_locks: Mutex::new(HashMap::new()),
            sequence_cache: Mutex::new(HashMap::new()),
        }
    }

    fn file_path(&self, session_id: &SessionId) -> PathBuf {
        self.root
            .join("events")
            .join(format!("{}.jsonl", session_id.as_str()))
    }

    async fn ensure_parent(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create events dir {parent:?}"))?;
        }
        Ok(())
    }

    fn lock_for(&self, session_id: &SessionId) -> Arc<tokio::sync::Mutex<()>> {
        let mut guard = self.write_locks.lock();
        guard
            .entry(session_id.as_str().to_owned())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    async fn scan_latest_sequence(path: &Path, branch_filter: Option<&BranchId>) -> Result<u64> {
        if !fs::try_exists(path).await.unwrap_or(false) {
            return Ok(0);
        }

        let file = OpenOptions::new().read(true).open(path).await?;
        let mut reader = BufReader::new(file).lines();
        let mut latest = 0_u64;

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let event: EventRecord = serde_json::from_str(&line)
                .with_context(|| format!("failed parsing event line in {path:?}"))?;
            if let Some(branch) = branch_filter
                && event.branch_id != *branch
            {
                continue;
            }
            latest = latest.max(event.sequence);
        }
        Ok(latest)
    }

    fn cached_latest_sequence(&self, session_id: &SessionId, branch_id: &BranchId) -> Option<u64> {
        self.sequence_cache
            .lock()
            .get(&(
                session_id.as_str().to_owned(),
                branch_id.as_str().to_owned(),
            ))
            .copied()
    }

    fn update_cached_sequence(&self, session_id: &SessionId, branch_id: &BranchId, latest: u64) {
        self.sequence_cache.lock().insert(
            (
                session_id.as_str().to_owned(),
                branch_id.as_str().to_owned(),
            ),
            latest,
        );
    }
}

#[async_trait]
impl EventStore for FileEventStore {
    #[instrument(
        skip(self, event),
        fields(
            session_id = %event.session_id,
            branch = %event.branch_id,
            sequence = event.sequence
        )
    )]
    async fn append(&self, event: &EventRecord) -> Result<()> {
        let path = self.file_path(&event.session_id);
        Self::ensure_parent(&path).await?;

        let lock = self.lock_for(&event.session_id);
        let _guard = lock.lock().await;

        let latest = match self.cached_latest_sequence(&event.session_id, &event.branch_id) {
            Some(latest) => latest,
            None => {
                let latest = Self::scan_latest_sequence(&path, Some(&event.branch_id)).await?;
                self.update_cached_sequence(&event.session_id, &event.branch_id, latest);
                latest
            }
        };

        let expected_sequence = latest.saturating_add(1);
        if event.sequence != expected_sequence {
            warn!(
                expected_sequence,
                actual_sequence = event.sequence,
                "sequence conflict while appending event"
            );
            bail!(
                "sequence conflict for session {}: expected {}, got {}",
                event.session_id,
                expected_sequence,
                event.sequence
            );
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .with_context(|| format!("failed opening event log {path:?}"))?;

        let line = serde_json::to_string(event).context("failed serializing event")?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        self.update_cached_sequence(&event.session_id, &event.branch_id, event.sequence);
        debug!("event appended to store");
        Ok(())
    }

    #[instrument(
        skip(self),
        fields(session_id = %session_id, branch = ?branch_id.as_ref().map(|b| b.as_str()), from_sequence, limit)
    )]
    async fn read_from(
        &self,
        session_id: SessionId,
        branch_id: Option<BranchId>,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>> {
        let path = self.file_path(&session_id);
        if !fs::try_exists(&path).await.unwrap_or(false) {
            return Ok(Vec::new());
        }

        let file = OpenOptions::new().read(true).open(&path).await?;
        let mut reader = BufReader::new(file).lines();
        let mut out = Vec::new();

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let event: EventRecord = serde_json::from_str(&line)
                .with_context(|| format!("failed parsing event line in {path:?}"))?;
            if let Some(branch) = &branch_id
                && event.branch_id != *branch
            {
                continue;
            }
            if event.sequence >= from_sequence {
                out.push(event);
            }
            if out.len() >= limit {
                break;
            }
        }
        debug!(count = out.len(), "events loaded from store");
        Ok(out)
    }

    #[instrument(
        skip(self),
        fields(session_id = %session_id, branch = ?branch_id.as_ref().map(|b| b.as_str()))
    )]
    async fn latest_sequence(
        &self,
        session_id: SessionId,
        branch_id: Option<BranchId>,
    ) -> Result<u64> {
        let branch = branch_id.unwrap_or_default();
        if let Some(latest) = self.cached_latest_sequence(&session_id, &branch) {
            return Ok(latest);
        }

        let path = self.file_path(&session_id);
        let latest = Self::scan_latest_sequence(&path, Some(&branch)).await?;
        self.update_cached_sequence(&session_id, &branch, latest);
        debug!(latest, "latest sequence resolved");
        Ok(latest)
    }
}

#[derive(Clone, Debug)]
pub struct EventStreamHub {
    sender: broadcast::Sender<EventRecord>,
}

impl EventStreamHub {
    pub fn new(buffer: usize) -> Self {
        let (sender, _) = broadcast::channel(buffer);
        Self { sender }
    }

    pub fn publish(&self, event: EventRecord) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EventRecord> {
        self.sender.subscribe()
    }

    pub fn subscribe_stream(&self) -> BroadcastStream<EventRecord> {
        BroadcastStream::new(self.sender.subscribe())
    }
}

#[derive(Clone)]
pub struct EventJournal {
    store: Arc<dyn EventStore>,
    stream: EventStreamHub,
}

impl EventJournal {
    pub fn new(store: Arc<dyn EventStore>, stream: EventStreamHub) -> Self {
        Self { store, stream }
    }

    #[instrument(
        skip(self, event),
        fields(
            session_id = %event.session_id,
            branch = %event.branch_id,
            sequence = event.sequence
        )
    )]
    pub async fn append_and_publish(&self, event: EventRecord) -> Result<()> {
        self.store.append(&event).await?;
        self.stream.publish(event);
        debug!("event published to stream");
        Ok(())
    }

    pub async fn read_from(
        &self,
        session_id: SessionId,
        branch_id: Option<BranchId>,
        from_sequence: u64,
        limit: usize,
    ) -> Result<Vec<EventRecord>> {
        self.store
            .read_from(session_id, branch_id, from_sequence, limit)
            .await
    }

    pub async fn latest_sequence(
        &self,
        session_id: SessionId,
        branch_id: Option<BranchId>,
    ) -> Result<u64> {
        self.store.latest_sequence(session_id, branch_id).await
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EventRecord> {
        self.stream.subscribe()
    }
}

#[async_trait]
impl EventStorePort for EventJournal {
    async fn append(&self, event: EventRecord) -> std::result::Result<EventRecord, KernelError> {
        self.append_and_publish(event.clone())
            .await
            .map_err(to_kernel_error)?;
        Ok(event)
    }

    async fn read(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
        from_sequence: u64,
        limit: usize,
    ) -> std::result::Result<Vec<EventRecord>, KernelError> {
        self.read_from(session_id, Some(branch_id), from_sequence, limit)
            .await
            .map_err(to_kernel_error)
    }

    async fn head(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
    ) -> std::result::Result<u64, KernelError> {
        self.latest_sequence(session_id, Some(branch_id))
            .await
            .map_err(to_kernel_error)
    }

    async fn subscribe(
        &self,
        session_id: SessionId,
        branch_id: BranchId,
        after_sequence: u64,
    ) -> std::result::Result<EventRecordStream, KernelError> {
        // Replay a bounded prefix to support resume semantics before tailing.
        let replay = self
            .read_from(
                session_id.clone(),
                Some(branch_id.clone()),
                after_sequence.saturating_add(1),
                10_000,
            )
            .await
            .map_err(to_kernel_error)?;

        let mut receiver = EventJournal::subscribe(self);
        let stream = async_stream::try_stream! {
            for event in replay {
                if event.session_id == session_id
                    && event.branch_id == branch_id
                    && event.sequence > after_sequence
                {
                    yield event;
                }
            }

            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if event.session_id == session_id
                            && event.branch_id == branch_id
                            && event.sequence > after_sequence
                        {
                            yield event;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "event subscription lagged; dropping stale frames");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use aios_protocol::{BranchId, EventKind, EventRecord, LoopPhase, SessionId};
    use anyhow::Result;
    use tokio::fs;

    use crate::{EventStore, FileEventStore};

    fn unique_test_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{name}-{nanos}"))
    }

    #[tokio::test]
    async fn file_event_store_appends_and_reads_in_sequence() -> Result<()> {
        let root = unique_test_root("aios-events");
        let store = FileEventStore::new(&root);
        let session_id = SessionId::default();

        let event1 = EventRecord::new(
            session_id.clone(),
            BranchId::main(),
            1,
            EventKind::PhaseEntered {
                phase: LoopPhase::Perceive,
            },
        );
        let event2 = EventRecord::new(
            session_id.clone(),
            BranchId::main(),
            2,
            EventKind::PhaseEntered {
                phase: LoopPhase::Deliberate,
            },
        );

        store.append(&event1).await?;
        store.append(&event2).await?;

        let from_two = store
            .read_from(session_id.clone(), Some(BranchId::main()), 2, 10)
            .await?;
        assert_eq!(from_two.len(), 1);
        assert_eq!(from_two[0].sequence, 2);

        let latest = store
            .latest_sequence(session_id, Some(BranchId::main()))
            .await?;
        assert_eq!(latest, 2);

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }

    #[tokio::test]
    async fn file_event_store_rejects_duplicate_sequence() -> Result<()> {
        let root = unique_test_root("aios-events-duplicate");
        let store = FileEventStore::new(&root);
        let session_id = SessionId::default();

        let event1 = EventRecord::new(
            session_id.clone(),
            BranchId::main(),
            1,
            EventKind::PhaseEntered {
                phase: LoopPhase::Perceive,
            },
        );
        let duplicate = EventRecord::new(
            session_id.clone(),
            BranchId::main(),
            1,
            EventKind::PhaseEntered {
                phase: LoopPhase::Deliberate,
            },
        );

        store.append(&event1).await?;
        let err = store.append(&duplicate).await.unwrap_err();
        assert!(err.to_string().contains("expected 2, got 1"));

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }

    #[tokio::test]
    async fn file_event_store_rejects_sequence_gap() -> Result<()> {
        let root = unique_test_root("aios-events-gap");
        let store = FileEventStore::new(&root);
        let session_id = SessionId::default();

        let first = EventRecord::new(
            session_id.clone(),
            BranchId::main(),
            1,
            EventKind::PhaseEntered {
                phase: LoopPhase::Perceive,
            },
        );
        let gap = EventRecord::new(
            session_id.clone(),
            BranchId::main(),
            3,
            EventKind::PhaseEntered {
                phase: LoopPhase::Deliberate,
            },
        );

        store.append(&first).await?;
        let err = store.append(&gap).await.unwrap_err();
        assert!(err.to_string().contains("expected 2, got 3"));

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }

    #[tokio::test]
    async fn file_event_store_tracks_sequences_per_branch() -> Result<()> {
        let root = unique_test_root("aios-events-branch");
        let store = FileEventStore::new(&root);
        let session_id = SessionId::default();
        let main = BranchId::main();
        let feature = BranchId::from_string("feature-x");

        let main_event = EventRecord::new(
            session_id.clone(),
            main.clone(),
            1,
            EventKind::PhaseEntered {
                phase: LoopPhase::Perceive,
            },
        );
        let feature_event = EventRecord::new(
            session_id.clone(),
            feature.clone(),
            1,
            EventKind::PhaseEntered {
                phase: LoopPhase::Deliberate,
            },
        );
        store.append(&main_event).await?;
        store.append(&feature_event).await?;

        let main_latest = store
            .latest_sequence(session_id.clone(), Some(main.clone()))
            .await?;
        let feature_latest = store
            .latest_sequence(session_id.clone(), Some(feature.clone()))
            .await?;
        assert_eq!(main_latest, 1);
        assert_eq!(feature_latest, 1);

        let main_events = store
            .read_from(session_id.clone(), Some(main), 1, 10)
            .await?;
        let feature_events = store.read_from(session_id, Some(feature), 1, 10).await?;
        assert_eq!(main_events.len(), 1);
        assert_eq!(feature_events.len(), 1);

        let _ = fs::remove_dir_all(root).await;
        Ok(())
    }
}
