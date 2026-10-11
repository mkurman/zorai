use super::*;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

mod basic;
mod replay;
mod runtime;
mod thread_links;

fn make_test_root(test_name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir()
        .join("zorai-daemon-test-artifacts")
        .join(format!("{test_name}-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("failed to create test root");
    root
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("daemon crate dir")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn gateway_loop_production_source() -> String {
    let root = repo_root().join("crates/zorai-daemon/src/agent/gateway_loop");
    let mut paths = fs::read_dir(root)
        .expect("read gateway_loop dir")
        .map(|entry| entry.expect("gateway_loop dir entry").path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("rs")
                && path.file_name().and_then(|name| name.to_str()) != Some("mod.rs")
                && path.file_name().and_then(|name| name.to_str()) != Some("tests.rs")
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| fs::read_to_string(path).expect("read split gateway_loop source"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(unix)]
fn make_gateway_test_binary(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join(name);
    fs::write(&path, "#!/bin/sh\nsleep 60\n").expect("failed to write gateway test binary");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("failed to mark gateway test binary executable");
    path
}

fn make_replay_gateway_config() -> super::types::GatewayConfig {
    use super::types::GatewayConfig;
    GatewayConfig {
        enabled: true,
        slack_token: String::new(),
        slack_channel_filter: String::new(),
        telegram_token: "tok".into(),
        telegram_allowed_chats: String::new(),
        discord_token: String::new(),
        discord_channel_filter: String::new(),
        discord_allowed_users: String::new(),
        whatsapp_allowed_contacts: String::new(),
        whatsapp_token: String::new(),
        whatsapp_phone_id: String::new(),
        command_prefix: "!".into(),
        default_agent: "rarog".into(),
        slack_default_agent: String::new(),
        discord_default_agent: String::new(),
        telegram_default_agent: String::new(),
        whatsapp_default_agent: String::new(),
        gateway_electron_bridges_enabled: false,
        whatsapp_link_fallback_electron: false,
    }
}

fn make_telegram_replay_envelope(
    cursor_value: &str,
    channel_id: &str,
    content: &str,
    sender: &str,
) -> super::gateway::ReplayEnvelope {
    super::gateway::ReplayEnvelope {
        message: super::gateway::IncomingMessage {
            platform: "Telegram".into(),
            sender: sender.into(),
            content: content.into(),
            channel: channel_id.into(),
            message_id: Some(format!("tg:{cursor_value}")),
            thread_context: None,
        },
        channel_id: "global".into(),
        cursor_value: cursor_value.into(),
        cursor_type: "update_id",
    }
}

fn make_telegram_replay_envelope_with_id(
    cursor_value: &str,
    message_id: &str,
    channel_id: &str,
    content: &str,
    sender: &str,
) -> super::gateway::ReplayEnvelope {
    super::gateway::ReplayEnvelope {
        message: super::gateway::IncomingMessage {
            platform: "Telegram".into(),
            sender: sender.into(),
            content: content.into(),
            channel: channel_id.into(),
            message_id: Some(message_id.into()),
            thread_context: None,
        },
        channel_id: "global".into(),
        cursor_value: cursor_value.into(),
        cursor_type: "update_id",
    }
}

fn make_malformed_telegram_replay_envelope() -> super::gateway::ReplayEnvelope {
    super::gateway::ReplayEnvelope {
        message: super::gateway::IncomingMessage {
            platform: "Telegram".into(),
            sender: "x".into(),
            content: "some content".into(),
            channel: "777".into(),
            message_id: None,
            thread_context: None,
        },
        channel_id: "global".into(),
        cursor_value: "".into(),
        cursor_type: "update_id",
    }
}
