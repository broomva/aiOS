//! Low-level hypervisor substrate types — shared vocabulary for VM-backed
//! execution across the Agent OS.
//!
//! These types are consumed by the [`HypervisorBackend`] trait, implemented by
//! backend adapter crates (`arcan-provider-local`, `arcan-provider-vercel`,
//! `arcan-provider-cube`, …), and surfaced to callers through the (future)
//! `KernelPort` trait.
//!
//! ## Trait contract
//!
//! [`HypervisorBackend`] is the minimum contract every backend must honour
//! (create / exec / snapshot / restore / destroy). `hibernate` and `resume`
//! have default impls that return [`BackendError::NotSupported`] so backends
//! that cannot pause VMs are not forced to implement them.
//!
//! [`HypervisorFilesystemExt`] is an optional extension trait — backends that
//! expose filesystem reads/writes implement it and advertise
//! [`BackendCapabilitySet::FILESYSTEM_EXT`] from
//! [`HypervisorBackend::capabilities`].

use std::collections::HashMap;
use std::fmt;

use async_trait::async_trait;
use bitflags::bitflags;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, SessionId};

// ── Identity ─────────────────────────────────────────────────────────────────

/// Opaque, globally unique identifier for a VM instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VmId(pub String);

impl fmt::Display for VmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for VmId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for VmId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Opaque identifier for a VM filesystem/memory snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VmSnapshotId(pub String);

impl fmt::Display for VmSnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for VmSnapshotId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for VmSnapshotId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Identifier for a registered hypervisor backend (e.g. `"local"`, `"cube"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BackendId(pub String);

impl fmt::Display for BackendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for BackendId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for BackendId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

// ── Resources ────────────────────────────────────────────────────────────────

/// Compute resource request for a new VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmResources {
    pub vcpus: u32,
    pub memory_kb: u64,
    pub disk_kb: u64,
    pub timeout_secs: u64,
}

impl Default for VmResources {
    fn default() -> Self {
        Self {
            vcpus: 1,
            memory_kb: 512 * 1024,
            disk_kb: 2048 * 1024,
            timeout_secs: 60,
        }
    }
}

// ── Runtime hints ────────────────────────────────────────────────────────────

/// Hint to the backend about what runtime the guest expects.
///
/// The backend may still reject or substitute; this is a best-effort signal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuntimeHint {
    #[default]
    Shell,
    Node {
        version: String,
    },
    Python {
        version: String,
    },
    Custom {
        image: String,
    },
}

// ── Mount ────────────────────────────────────────────────────────────────────

/// A mount of a host path / blob into the guest filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mount {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

// ── Network policy (reused from sandbox.rs) ──────────────────────────────────
//
// `VmSpec.network_policy` intentionally reuses [`crate::sandbox::NetworkPolicy`]
// so the declarative policy vocabulary stays in one place; enforcement lands
// with `NetworkIsolationPort` (BRO-849).

// ── Backend selector ─────────────────────────────────────────────────────────

/// How the kernel picks which backend runs a VM.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum BackendSelector {
    /// Use a specific backend by name.
    Explicit { backend: BackendId },
    /// Let the kernel pick from available backends based on capability match.
    #[default]
    Auto,
}

// ── Spec ─────────────────────────────────────────────────────────────────────

/// Full specification used to create a new VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSpec {
    #[serde(default)]
    pub backend_selector: BackendSelector,
    #[serde(default)]
    pub resources: VmResources,
    #[serde(default)]
    pub network_policy: crate::sandbox::NetworkPolicy,
    #[serde(default)]
    pub mounts: Vec<Mount>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub runtime_hint: RuntimeHint,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

/// Overrides applied to a VM spec during a fork.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VmSpecOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<VmResources>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

// ── Handle & status ──────────────────────────────────────────────────────────

/// Current lifecycle state of a VM instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[non_exhaustive]
pub enum VmStatus {
    Starting,
    Running,
    Hibernated,
    Snapshotted,
    Stopping,
    Stopped,
    Failed { reason: String },
}

/// Live reference to a VM returned by `create()` / `resume()` / `fork()`.
///
/// `metadata` is an opaque JSON bag so backends can stash provider-specific
/// fields without extending the ABI; callers should treat unknown keys as
/// forward-compatible. `PartialEq`/`Eq` are intentionally omitted because
/// `serde_json::Value` does not implement `Eq`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmHandle {
    pub vm_id: VmId,
    pub backend: BackendId,
    pub session_id: SessionId,
    pub agent_id: AgentId,
    pub status: VmStatus,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Handle for a named VM snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSnapshotHandle {
    pub snapshot_id: VmSnapshotId,
    pub vm_id: VmId,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
}

/// Request to fork a VM from a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkSpec {
    pub parent_snapshot: VmSnapshotId,
    #[serde(default)]
    pub overrides: VmSpecOverrides,
}

/// Lightweight summary for listing VMs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    pub vm_id: VmId,
    pub backend: BackendId,
    pub status: VmStatus,
    pub created_at: DateTime<Utc>,
}

// ── Exec (lower-level than KernelPort::dispatch) ─────────────────────────────

/// Shell-level command to execute inside a running VM.
///
/// This is the contract `HypervisorBackend` exposes (BRO-848). Higher-level
/// Tool-ABI dispatch (via `KernelPort`) translates `ToolCall` into
/// [`ExecRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<Vec<u8>>,
}

impl ExecRequest {
    /// Helper to build a POSIX shell invocation.
    pub fn shell(command: impl Into<String>) -> Self {
        Self {
            command: vec!["/bin/sh".into(), "-c".into(), command.into()],
            working_dir: None,
            env: HashMap::new(),
            timeout_secs: None,
            stdin: None,
        }
    }
}

/// Result of an [`ExecRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_code: i32,
    pub duration_ms: u64,
}

/// A single file to write into a VM filesystem via the
/// [`HypervisorFilesystemExt`] trait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWrite {
    pub path: String,
    pub content: Vec<u8>,
    pub mode: u32,
}

// ── Capabilities & errors ────────────────────────────────────────────────────

bitflags! {
    /// Capability bits advertised by a hypervisor backend.
    ///
    /// Callers inspect these bits to decide whether a backend can honour a
    /// given operation before dispatching; the kernel also uses them for
    /// backend selection when [`BackendSelector::Auto`] is requested.
    ///
    /// The bit layout is stable across minor versions — new capabilities
    /// occupy the next free bit, existing bits are never repurposed.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct BackendCapabilitySet: u32 {
        /// Backend can read files from VM guests.
        const FILESYSTEM_READ  = 1 << 0;
        /// Backend can write files to VM guests.
        const FILESYSTEM_WRITE = 1 << 1;
        /// Backend implements the filesystem extension trait.
        const FILESYSTEM_EXT   = 1 << 2;
        /// Backend permits egress networking per [`crate::sandbox::NetworkPolicy`].
        const NETWORK_EGRESS   = 1 << 3;
        /// Backend permits ingress networking per [`crate::sandbox::NetworkPolicy`].
        const NETWORK_INGRESS  = 1 << 4;
        /// Backend supports `snapshot` / `restore`.
        const PERSISTENCE      = 1 << 5;
        /// Backend supports forking from a snapshot.
        const FORK             = 1 << 6;
        /// Backend supports `hibernate` / `resume`.
        const HIBERNATE        = 1 << 7;
        /// Backend can materialise a custom [`RuntimeHint::Custom`] image.
        const CUSTOM_IMAGE     = 1 << 8;
        /// Backend preserves user-supplied [`VmSpec::labels`] (tags).
        const TAGS             = 1 << 9;
        /// Backend exposes GPU devices to guests.
        const GPU              = 1 << 10;
    }
}

/// Backend-reported error.
///
/// Emitted by the `HypervisorBackend` trait family (BRO-848). The kernel
/// converts these variants into [`crate::kernel::KernelError`] via the
/// `#[from]` bridge added in the same ticket.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BackendError {
    /// The referenced VM does not exist on this backend.
    #[error("vm not found: {0}")]
    VmNotFound(VmId),
    /// The referenced snapshot does not exist on this backend.
    #[error("snapshot not found: {0}")]
    SnapshotNotFound(VmSnapshotId),
    /// The backend does not support the requested operation.
    #[error("operation not supported by backend {backend}: {reason}")]
    NotSupported {
        backend: &'static str,
        reason: &'static str,
    },
    /// The operation requires capabilities this backend does not advertise.
    #[error("capability denied: {0:?}")]
    CapabilityDenied(BackendCapabilitySet),
    /// The operation exceeded its time budget.
    #[error("timeout after {duration_ms} ms")]
    Timeout { duration_ms: u64 },
    /// Transport-level failure (HTTP, RPC, local IPC, …).
    #[error("transport: {0}")]
    Transport(String),
    /// Catch-all for backend-internal failures.
    #[error("internal: {0}")]
    Internal(String),
}

// ── Traits ───────────────────────────────────────────────────────────────────

/// Low-level hypervisor substrate — implemented by `arcan-provider-*` crates.
///
/// Uses `#[async_trait]` so the trait is dyn-compatible; callers typically
/// hold `Arc<dyn HypervisorBackend>` inside the kernel backend registry.
#[async_trait]
pub trait HypervisorBackend: Send + Sync + 'static {
    /// Stable name used for routing + observability. Examples: `"local"`, `"cube"`, `"vercel"`.
    fn name(&self) -> &'static str;

    /// Capability bits this backend honours.
    fn capabilities(&self) -> BackendCapabilitySet;

    /// Provision a new VM from the spec. May return before the VM is ready
    /// (status [`VmStatus::Starting`]).
    async fn create(&self, spec: VmSpec) -> Result<VmHandle, BackendError>;

    /// Execute a shell-level request inside a running VM.
    async fn exec(&self, vm: &VmHandle, req: ExecRequest) -> Result<ExecResult, BackendError>;

    /// Snapshot the current VM state. Returns an opaque snapshot id.
    async fn snapshot(&self, vm: &VmHandle) -> Result<VmSnapshotId, BackendError>;

    /// Restore a VM from a snapshot, returning a handle for the new instance.
    async fn restore(&self, snapshot: &VmSnapshotId) -> Result<VmHandle, BackendError>;

    /// Destroy the VM. MUST succeed even if the VM is already stopped.
    async fn destroy(&self, vm: &VmHandle) -> Result<(), BackendError>;

    /// Hibernate the VM (pause + snapshot). Default impl returns
    /// [`BackendError::NotSupported`].
    async fn hibernate(&self, _vm: &VmHandle) -> Result<(), BackendError> {
        Err(BackendError::NotSupported {
            backend: self.name(),
            reason: "hibernate",
        })
    }

    /// Resume a hibernated VM. Default impl returns
    /// [`BackendError::NotSupported`].
    async fn resume(&self, _vm: &VmHandle) -> Result<(), BackendError> {
        Err(BackendError::NotSupported {
            backend: self.name(),
            reason: "resume",
        })
    }
}

/// Optional extension for backends that expose filesystem operations.
///
/// Backends that don't implement this trait (for example, a future aiOS-native
/// guest) will cause `KernelPort::dispatch` to return
/// [`crate::kernel::KernelError::CapabilityUnavailable`] when a tool tries to
/// invoke a filesystem-dependent operation. Implementors MUST also advertise
/// [`BackendCapabilitySet::FILESYSTEM_EXT`] from
/// [`HypervisorBackend::capabilities`].
#[async_trait]
pub trait HypervisorFilesystemExt: HypervisorBackend {
    /// Write a batch of files into the VM's guest filesystem.
    async fn write_files(&self, vm: &VmHandle, files: Vec<FileWrite>) -> Result<(), BackendError>;

    /// Read a single file from the VM's guest filesystem.
    async fn read_file(&self, vm: &VmHandle, path: &str) -> Result<Vec<u8>, BackendError>;

    /// List VMs managed by this backend (used for diagnostics + GC).
    async fn list(&self) -> Result<Vec<VmInfo>, BackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Identity ──

    #[test]
    fn vm_id_display_and_from() {
        assert_eq!(VmId::from("abc").to_string(), "abc");
        assert_eq!(VmId::from(String::from("xyz")).to_string(), "xyz");
    }

    #[test]
    fn vm_id_is_transparent() {
        let id = VmId::from("vm-42");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"vm-42\"");
        let back: VmId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn backend_id_from_str_trait() {
        let id: BackendId = "local".into();
        assert_eq!(id.to_string(), "local");
    }

    // ── Resources / hints / selector ──

    #[test]
    fn vm_resources_defaults() {
        let r = VmResources::default();
        assert_eq!(r.vcpus, 1);
        assert_eq!(r.memory_kb, 524_288);
        assert_eq!(r.disk_kb, 2_097_152);
        assert_eq!(r.timeout_secs, 60);
    }

    #[test]
    fn runtime_hint_default_is_shell() {
        assert_eq!(RuntimeHint::default(), RuntimeHint::Shell);
    }

    #[test]
    fn runtime_hint_node_serde() {
        let h = RuntimeHint::Node {
            version: "20.11".into(),
        };
        let json = serde_json::to_string(&h).unwrap();
        let back: RuntimeHint = serde_json::from_str(&json).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn backend_selector_defaults_to_auto() {
        assert_eq!(BackendSelector::default(), BackendSelector::Auto);
    }

    #[test]
    fn backend_selector_explicit_serde() {
        let s = BackendSelector::Explicit {
            backend: BackendId::from("local"),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: BackendSelector = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    // ── Status ──

    #[test]
    fn vm_status_serde_roundtrip() {
        for s in [
            VmStatus::Starting,
            VmStatus::Running,
            VmStatus::Hibernated,
            VmStatus::Snapshotted,
            VmStatus::Stopping,
            VmStatus::Stopped,
            VmStatus::Failed {
                reason: "oom".into(),
            },
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let back: VmStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(s, back);
        }
    }

    // ── Spec ──

    #[test]
    fn vm_spec_default_network_is_disabled() {
        use crate::sandbox::NetworkPolicy;
        let spec = VmSpec {
            backend_selector: BackendSelector::Auto,
            resources: VmResources::default(),
            network_policy: NetworkPolicy::default(),
            mounts: Vec::new(),
            env: HashMap::new(),
            runtime_hint: RuntimeHint::default(),
            labels: HashMap::new(),
        };
        assert_eq!(spec.network_policy, NetworkPolicy::Disabled);
    }

    #[test]
    fn vm_spec_roundtrip_minimal() {
        let spec = VmSpec {
            backend_selector: BackendSelector::Auto,
            resources: VmResources::default(),
            network_policy: crate::sandbox::NetworkPolicy::Disabled,
            mounts: Vec::new(),
            env: HashMap::new(),
            runtime_hint: RuntimeHint::Shell,
            labels: HashMap::new(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let _back: VmSpec = serde_json::from_str(&json).unwrap();
    }

    // ── Handle / snapshot / fork ──

    #[test]
    fn vm_handle_roundtrip_preserves_metadata() {
        let handle = VmHandle {
            vm_id: VmId::from("vm-42"),
            backend: BackendId::from("local"),
            session_id: SessionId::from_string("sess-1"),
            agent_id: AgentId::from_string("agent-1"),
            status: VmStatus::Running,
            created_at: Utc::now(),
            metadata: serde_json::json!({ "region": "us-east-1" }),
        };
        let json = serde_json::to_string(&handle).unwrap();
        let back: VmHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(back.vm_id, handle.vm_id);
        assert_eq!(back.metadata["region"], "us-east-1");
    }

    #[test]
    fn vm_snapshot_handle_roundtrip() {
        let snap = VmSnapshotHandle {
            snapshot_id: VmSnapshotId::from("snap-1"),
            vm_id: VmId::from("vm-1"),
            name: "pre-fork".into(),
            created_at: Utc::now(),
            size_bytes: 1024,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: VmSnapshotHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, snap.name);
    }

    #[test]
    fn fork_spec_roundtrip() {
        let spec = ForkSpec {
            parent_snapshot: VmSnapshotId::from("snap-1"),
            overrides: VmSpecOverrides::default(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: ForkSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.parent_snapshot, spec.parent_snapshot);
    }

    // ── Exec ──

    #[test]
    fn exec_request_shell_helper() {
        let r = ExecRequest::shell("echo hi");
        assert_eq!(r.command, vec!["/bin/sh", "-c", "echo hi"]);
        assert!(r.working_dir.is_none());
        assert!(r.stdin.is_none());
    }

    #[test]
    fn exec_request_roundtrip_omits_none() {
        let r = ExecRequest::shell("true");
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("working_dir"));
        assert!(!json.contains("timeout_secs"));
        assert!(!json.contains("stdin"));
    }

    #[test]
    fn exec_result_roundtrip() {
        let r = ExecResult {
            stdout: b"hello".to_vec(),
            stderr: Vec::new(),
            exit_code: 0,
            duration_ms: 12,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: ExecResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.stdout, r.stdout);
        assert_eq!(back.exit_code, 0);
    }

    #[test]
    fn file_write_equality() {
        let a = FileWrite {
            path: "/tmp/a".into(),
            content: b"x".to_vec(),
            mode: 0o644,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    // ── Capabilities ──

    #[test]
    fn capability_set_bit_combinations() {
        let cs = BackendCapabilitySet::FILESYSTEM_READ | BackendCapabilitySet::FORK;
        assert!(cs.contains(BackendCapabilitySet::FILESYSTEM_READ));
        assert!(cs.contains(BackendCapabilitySet::FORK));
        assert!(!cs.contains(BackendCapabilitySet::GPU));
    }

    #[test]
    fn backend_capability_set_serde_roundtrip() {
        let cs = BackendCapabilitySet::FILESYSTEM_READ | BackendCapabilitySet::FILESYSTEM_EXT;
        let json = serde_json::to_string(&cs).unwrap();
        let back: BackendCapabilitySet = serde_json::from_str(&json).unwrap();
        assert_eq!(cs, back);
        assert!(back.contains(BackendCapabilitySet::FILESYSTEM_READ));
        assert!(back.contains(BackendCapabilitySet::FILESYSTEM_EXT));
        assert!(!back.contains(BackendCapabilitySet::NETWORK_EGRESS));
    }

    // ── BackendError ──

    #[test]
    fn backend_error_display_includes_context() {
        let e = BackendError::NotSupported {
            backend: "test",
            reason: "hibernate",
        };
        assert!(e.to_string().contains("hibernate"));
        assert!(e.to_string().contains("test"));
    }

    #[test]
    fn backend_error_timeout_display() {
        let e = BackendError::Timeout { duration_ms: 1_500 };
        assert!(e.to_string().contains("1500"));
    }
}

#[cfg(test)]
mod trait_tests {
    use super::*;

    // Compile-time assertion that `HypervisorBackend` is dyn-compatible —
    // the whole reason we use `#[async_trait]` instead of native async fn.
    // The function is never called; its mere existence forces the compiler
    // to confirm `&dyn HypervisorBackend` is a well-formed type.
    #[allow(dead_code)]
    fn _assert_dyn_safe(_: &dyn HypervisorBackend) {}

    #[test]
    fn hypervisor_backend_is_dyn_compatible() {
        // If this test compiles, the trait is dyn-compatible.
        #[allow(dead_code)]
        fn _use_it(b: &dyn HypervisorBackend) -> &'static str {
            b.name()
        }
    }
}
