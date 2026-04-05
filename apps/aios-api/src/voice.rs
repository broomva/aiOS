use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterTransport {
    JsonLinesStdio,
    WebsocketFrames,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonaplexProcessContract {
    pub command: String,
    pub args: Vec<String>,
    pub required_env: Vec<String>,
    pub transport: AdapterTransport,
    pub protocol_version: String,
    pub model_id: String,
    pub sample_rate_hz: u32,
}

impl Default for PersonaplexProcessContract {
    fn default() -> Self {
        Self {
            command: "python3".to_owned(),
            args: vec!["-m".to_owned(), "personaplex_adapter".to_owned()],
            required_env: vec!["HF_TOKEN".to_owned()],
            transport: AdapterTransport::JsonLinesStdio,
            protocol_version: "v1alpha1".to_owned(),
            model_id: "nvidia/personaplex-7b-v1".to_owned(),
            sample_rate_hz: 24_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSessionConfig {
    pub role_prompt: Option<String>,
    pub voice_prompt_ref: Option<String>,
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub format: String,
}

impl Default for VoiceSessionConfig {
    fn default() -> Self {
        Self {
            role_prompt: None,
            voice_prompt_ref: None,
            sample_rate_hz: 24_000,
            channels: 1,
            format: "audio/pcm;rate=24000".to_owned(),
        }
    }
}

#[derive(Clone, Default)]
pub struct StubPersonaplexAdapter {
    contract: PersonaplexProcessContract,
    active_sessions: Arc<RwLock<HashSet<Uuid>>>,
}

impl StubPersonaplexAdapter {
    pub fn new(contract: PersonaplexProcessContract) -> Self {
        Self {
            contract,
            active_sessions: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn contract(&self) -> &PersonaplexProcessContract {
        &self.contract
    }

    pub async fn start_session(
        &self,
        _session_id: Uuid,
        _config: &VoiceSessionConfig,
    ) -> Result<Uuid> {
        let voice_session_id = Uuid::new_v4();
        self.active_sessions.write().await.insert(voice_session_id);
        Ok(voice_session_id)
    }

    pub async fn process_audio_chunk(
        &self,
        voice_session_id: Uuid,
        audio_chunk: &[u8],
    ) -> Result<Vec<u8>> {
        if !self
            .active_sessions
            .read()
            .await
            .contains(&voice_session_id)
        {
            bail!("voice session not active: {voice_session_id}");
        }

        // Stub contract: passthrough audio bytes for loopback validation.
        Ok(audio_chunk.to_vec())
    }

    pub async fn stop_session(&self, voice_session_id: Uuid) -> Result<()> {
        self.active_sessions.write().await.remove(&voice_session_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::{PersonaplexProcessContract, StubPersonaplexAdapter, VoiceSessionConfig};

    #[test]
    fn process_contract_roundtrips_json() -> Result<()> {
        let contract = PersonaplexProcessContract::default();
        let encoded = serde_json::to_string(&contract)?;
        let decoded: PersonaplexProcessContract = serde_json::from_str(&encoded)?;
        assert_eq!(decoded, contract);
        Ok(())
    }

    #[tokio::test]
    async fn stub_adapter_enforces_lifecycle() -> Result<()> {
        let adapter = StubPersonaplexAdapter::new(PersonaplexProcessContract::default());
        let voice_session_id = adapter
            .start_session(uuid::Uuid::new_v4(), &VoiceSessionConfig::default())
            .await?;

        let payload = vec![1_u8, 2, 3, 4];
        let output = adapter
            .process_audio_chunk(voice_session_id, &payload)
            .await?;
        assert_eq!(output, payload);

        adapter.stop_session(voice_session_id).await?;
        let err = adapter
            .process_audio_chunk(voice_session_id, &[9_u8])
            .await
            .expect_err("session should be inactive after stop");
        assert!(err.to_string().contains("not active"));

        Ok(())
    }
}
