use aios_protocol::EventRecord;
use axum::response::sse::Event;
use serde::Serialize;
use serde_json::{Value, json};

pub const VERCEL_AI_SDK_V6_STREAM_HEADER: &str = "x-vercel-ai-ui-message-stream";
pub const VERCEL_AI_SDK_V6_STREAM_VERSION: &str = "v1";

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VercelAiSdkV6Part {
    #[serde(rename = "start")]
    Start {
        #[serde(rename = "messageId")]
        message_id: String,
    },
    #[serde(rename = "start-step")]
    StartStep,
    #[serde(rename = "data-aios-event")]
    DataAiosEvent {
        id: String,
        data: Value,
        transient: bool,
    },
    #[serde(rename = "finish-step")]
    FinishStep {
        #[serde(rename = "finishReason")]
        finish_reason: String,
    },
    #[serde(rename = "finish")]
    Finish,
}

pub fn kernel_event_parts(event: &EventRecord) -> [VercelAiSdkV6Part; 5] {
    let message_id = format!("kernel-event-{}", event.event_id);
    let payload = serde_json::to_value(event).unwrap_or_else(|error| {
        json!({
            "error": error.to_string(),
            "sequence": event.sequence,
        })
    });

    [
        VercelAiSdkV6Part::Start {
            message_id: message_id.clone(),
        },
        VercelAiSdkV6Part::StartStep,
        VercelAiSdkV6Part::DataAiosEvent {
            id: event.sequence.to_string(),
            data: payload,
            transient: false,
        },
        VercelAiSdkV6Part::FinishStep {
            finish_reason: "stop".to_owned(),
        },
        VercelAiSdkV6Part::Finish,
    ]
}

pub fn part_as_sse_event(part: &VercelAiSdkV6Part) -> Event {
    let payload = serde_json::to_string(part).unwrap_or_else(|error| {
        json!({
            "type": "data-aios-stream-status",
            "data": {
                "status": "serialization_error",
                "error": error.to_string(),
            }
        })
        .to_string()
    });
    Event::default().data(payload)
}

#[cfg(test)]
mod tests {
    use aios_protocol::{BranchId, EventKind, EventRecord, SessionId};
    use serde_json::{Value, json};

    use super::{VercelAiSdkV6Part, kernel_event_parts};

    #[test]
    fn start_part_serializes_to_v6_shape() {
        let part = VercelAiSdkV6Part::Start {
            message_id: "msg_123".to_owned(),
        };
        let value = serde_json::to_value(part).expect("serialize start part");
        assert_eq!(
            value,
            json!({
                "type": "start",
                "messageId": "msg_123",
            })
        );
    }

    #[test]
    fn kernel_event_maps_to_custom_data_part() {
        let session_id = SessionId::from_string("00000000-0000-0000-0000-000000000000");
        let event = EventRecord::new(
            session_id,
            BranchId::main(),
            3,
            EventKind::Heartbeat {
                summary: "ok".to_owned(),
                checkpoint_id: None,
            },
        );

        let parts = kernel_event_parts(&event);
        let value = serde_json::to_value(&parts[2]).expect("serialize data part");

        assert_eq!(value["type"], "data-aios-event");
        assert_eq!(value["id"], "3");
        assert_eq!(value["transient"], false);
        assert!(matches!(value["data"], Value::Object(_)));
    }
}
