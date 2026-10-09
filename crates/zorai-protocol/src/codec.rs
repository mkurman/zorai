use bytes::{Buf, BufMut, BytesMut};
use serde::Serialize;
use tokio_util::codec::{Decoder, Encoder};

use crate::{ClientMessage, DaemonMessage};

/// Length-prefixed bincode codec for zorai IPC.
///
/// Wire format:
/// ```text
/// [4 bytes: payload length (little-endian u32)] [payload bytes (bincode)]
/// ```
///
/// This codec can encode both `ClientMessage` and `DaemonMessage` – the
/// concrete types are selected via generics.
#[derive(Debug, Default)]
pub struct ZoraiCodec;

pub const MAX_IPC_FRAME_SIZE_BYTES: usize = 16 * 1024 * 1024;
const MAX_FRAME_SIZE: u32 = MAX_IPC_FRAME_SIZE_BYTES as u32;

fn serialize_payload<T: Serialize>(item: &T) -> std::io::Result<Vec<u8>> {
    bincode::serialize(item)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

fn oversized_frame_error(kind: &str, payload_len: usize) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!(
            "{kind} too large for IPC: {payload_len} bytes exceeds {MAX_IPC_FRAME_SIZE_BYTES} bytes"
        ),
    )
}

pub fn client_message_payload_len(item: &ClientMessage) -> std::io::Result<usize> {
    Ok(serialize_payload(item)?.len())
}

pub fn daemon_message_payload_len(item: &DaemonMessage) -> std::io::Result<usize> {
    Ok(serialize_payload(item)?.len())
}

pub fn validate_client_message_size(item: &ClientMessage) -> std::io::Result<usize> {
    let payload_len = client_message_payload_len(item)?;
    if payload_len > MAX_IPC_FRAME_SIZE_BYTES {
        return Err(oversized_frame_error("client message", payload_len));
    }
    Ok(payload_len)
}

pub fn validate_daemon_message_size(item: &DaemonMessage) -> std::io::Result<usize> {
    let payload_len = daemon_message_payload_len(item)?;
    if payload_len > MAX_IPC_FRAME_SIZE_BYTES {
        return Err(oversized_frame_error("daemon message", payload_len));
    }
    Ok(payload_len)
}

pub fn client_message_fits_ipc(item: &ClientMessage) -> bool {
    validate_client_message_size(item).is_ok()
}

pub fn daemon_message_fits_ipc(item: &DaemonMessage) -> bool {
    validate_daemon_message_size(item).is_ok()
}

impl Decoder for ZoraiCodec {
    type Item = DaemonMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_le_bytes(length_bytes) as usize;

        if length as u32 > MAX_FRAME_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame too large: {length} bytes"),
            ));
        }

        if src.len() < 4 + length {
            src.reserve(4 + length - src.len());
            return Ok(None);
        }

        src.advance(4);
        let data = src.split_to(length);
        let msg: DaemonMessage = bincode::deserialize(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(Some(msg))
    }
}

impl Encoder<ClientMessage> for ZoraiCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: ClientMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let payload = serialize_payload(&item)?;
        if payload.len() > MAX_IPC_FRAME_SIZE_BYTES {
            return Err(oversized_frame_error("client message", payload.len()));
        }
        dst.put_u32_le(payload.len() as u32);
        dst.extend_from_slice(&payload);
        Ok(())
    }
}

/// Codec used on the **daemon** side: decodes `ClientMessage`, encodes
/// `DaemonMessage`.
#[derive(Debug, Default)]
pub struct DaemonCodec;

impl Decoder for DaemonCodec {
    type Item = ClientMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            return Ok(None);
        }

        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_le_bytes(length_bytes) as usize;

        if length as u32 > MAX_FRAME_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame too large: {length} bytes"),
            ));
        }

        if src.len() < 4 + length {
            src.reserve(4 + length - src.len());
            return Ok(None);
        }

        src.advance(4);
        let data = src.split_to(length);
        let msg: ClientMessage = bincode::deserialize(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(Some(msg))
    }
}

impl Encoder<DaemonMessage> for DaemonCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: DaemonMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let payload = serialize_payload(&item)?;
        if payload.len() > MAX_IPC_FRAME_SIZE_BYTES {
            let fallback = DaemonMessage::Error {
                message: format!(
                    "daemon response too large for IPC: {} bytes exceeds {} bytes",
                    payload.len(),
                    MAX_IPC_FRAME_SIZE_BYTES
                ),
            };
            let fallback_payload = serialize_payload(&fallback)?;
            if fallback_payload.len() > MAX_IPC_FRAME_SIZE_BYTES {
                return Err(oversized_frame_error(
                    "daemon error response",
                    fallback_payload.len(),
                ));
            }
            dst.put_u32_le(fallback_payload.len() as u32);
            dst.extend_from_slice(&fallback_payload);
            return Ok(());
        }
        dst.put_u32_le(payload.len() as u32);
        dst.extend_from_slice(&payload);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_encoder_rejects_oversized_requests_before_writing() {
        let mut codec = ZoraiCodec::default();
        let mut frame = BytesMut::new();
        let msg = ClientMessage::AgentSendMessage {
            thread_id: Some("thread-oversized".to_string()),
            content: "x".repeat(MAX_IPC_FRAME_SIZE_BYTES + 1024),
            session_id: None,
            context_messages_json: None,
            content_blocks_json: None,
            client_surface: None,
            target_agent_id: None,
            workspace_context_json: None,
        };

        let err = codec
            .encode(msg, &mut frame)
            .expect_err("oversized client request should be rejected");
        assert!(err.to_string().contains("client message too large for IPC"));
        assert!(
            frame.is_empty(),
            "rejected request should not write a partial frame"
        );
    }

    #[test]
    fn daemon_encoder_downgrades_oversized_responses_to_error_frame() {
        let mut daemon_codec = DaemonCodec::default();
        let mut frame = BytesMut::new();
        let msg = DaemonMessage::AnalysisResult {
            id: uuid::Uuid::nil(),
            result: "x".repeat(MAX_IPC_FRAME_SIZE_BYTES + 1024),
        };

        daemon_codec
            .encode(msg, &mut frame)
            .expect("oversized daemon reply should degrade to an error frame");

        let mut client_codec = ZoraiCodec::default();
        match client_codec
            .decode(&mut frame)
            .expect("decode downgraded error frame")
        {
            Some(DaemonMessage::Error { message }) => {
                assert!(message.contains("daemon response too large for IPC"));
            }
            other => panic!("expected downgraded daemon error, got {other:?}"),
        }
    }

    #[test]
    fn daemon_message_size_validation_detects_oversized_payloads_without_fallback() {
        let msg = DaemonMessage::PluginApiCallResult {
            operation_id: Some("op-huge".to_string()),
            plugin_name: "plugin".to_string(),
            endpoint_name: "endpoint".to_string(),
            success: true,
            result: "x".repeat(MAX_IPC_FRAME_SIZE_BYTES + 1024),
            error_type: None,
        };

        let err = validate_daemon_message_size(&msg)
            .expect_err("raw daemon size validation should reject oversized payloads");
        assert!(err.to_string().contains("daemon message too large for IPC"));
        assert!(!daemon_message_fits_ipc(&msg));
    }

    #[test]
    fn client_codec_round_trips_internal_delegate_message() {
        let mut client_codec = ZoraiCodec::default();
        let mut daemon_codec = DaemonCodec::default();
        let mut frame = BytesMut::new();

        client_codec
            .encode(
                ClientMessage::AgentInternalDelegate {
                    thread_id: Some("thread-1".to_string()),
                    target_agent_id: "weles".to_string(),
                    content: "investigate this".to_string(),
                    session_id: Some("sess-1".to_string()),
                    client_surface: Some(crate::ClientSurface::Tui),
                },
                &mut frame,
            )
            .expect("encode internal delegate");

        match daemon_codec
            .decode(&mut frame)
            .expect("decode internal delegate")
        {
            Some(ClientMessage::AgentInternalDelegate {
                thread_id,
                target_agent_id,
                content,
                session_id,
                client_surface,
            }) => {
                assert_eq!(thread_id.as_deref(), Some("thread-1"));
                assert_eq!(target_agent_id, "weles");
                assert_eq!(content, "investigate this");
                assert_eq!(session_id.as_deref(), Some("sess-1"));
                assert_eq!(client_surface, Some(crate::ClientSurface::Tui));
            }
            other => panic!("expected internal delegate message, got {other:?}"),
        }
    }

    #[test]
    fn client_codec_round_trips_thread_participant_command_message() {
        let mut client_codec = ZoraiCodec::default();
        let mut daemon_codec = DaemonCodec::default();
        let mut frame = BytesMut::new();

        client_codec
            .encode(
                ClientMessage::AgentThreadParticipantCommand {
                    thread_id: "thread-2".to_string(),
                    target_agent_id: "rarog".to_string(),
                    action: "upsert".to_string(),
                    instruction: Some("watch performance regressions".to_string()),
                    session_id: Some("sess-2".to_string()),
                    client_surface: Some(crate::ClientSurface::Tui),
                },
                &mut frame,
            )
            .expect("encode participant command");

        match daemon_codec
            .decode(&mut frame)
            .expect("decode participant command")
        {
            Some(ClientMessage::AgentThreadParticipantCommand {
                thread_id,
                target_agent_id,
                action,
                instruction,
                session_id,
                client_surface,
            }) => {
                assert_eq!(thread_id, "thread-2");
                assert_eq!(target_agent_id, "rarog");
                assert_eq!(action, "upsert");
                assert_eq!(
                    instruction.as_deref(),
                    Some("watch performance regressions")
                );
                assert_eq!(session_id.as_deref(), Some("sess-2"));
                assert_eq!(client_surface, Some(crate::ClientSurface::Tui));
            }
            other => panic!("expected participant command message, got {other:?}"),
        }
    }
}
