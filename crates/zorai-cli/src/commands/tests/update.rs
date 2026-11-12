use clap::Parser;

use crate::cli::{Cli, Commands, GoalAction, ThreadAction};
use crate::commands::core::should_check_for_updates;

#[test]
fn update_alias_parses_as_upgrade_command() {
    let cli = Cli::try_parse_from(["zorai", "update"]).expect("update alias should parse");

    assert!(matches!(
        cli.command,
        Some(Commands::Upgrade {
            keep_running: false
        })
    ));
}

#[test]
fn checks_updates_for_user_facing_commands() {
    assert!(should_check_for_updates(&Commands::Status));
    assert!(should_check_for_updates(&Commands::Prompt {
        agent: None,
        weles: false,
        concierge: false,
        rarog: false,
        json: false,
    }));
    assert!(should_check_for_updates(&Commands::Operation {
        id: "op-1".to_string(),
        json: false,
    }));
    assert!(should_check_for_updates(&Commands::Setup));
    assert!(should_check_for_updates(&Commands::List));
    assert!(should_check_for_updates(&Commands::Thread {
        action: ThreadAction::List {
            page: 1,
            limit: 20,
            json: false,
        },
    }));
    assert!(should_check_for_updates(&Commands::Goal {
        action: GoalAction::List {
            page: 1,
            limit: 20,
            json: false,
        },
    }));
}

#[test]
fn skips_update_checks_for_internal_and_upgrade_commands() {
    assert!(!should_check_for_updates(&Commands::Upgrade {
        keep_running: false
    }));
    assert!(!should_check_for_updates(&Commands::Stop));
    assert!(!should_check_for_updates(&Commands::Restart));
    assert!(!should_check_for_updates(&Commands::AgentBridge));
    assert!(!should_check_for_updates(&Commands::DbBridge));
}

#[test]
fn upgrade_keep_running_flag_parses() {
    let cli = Cli::try_parse_from(["zorai", "upgrade", "--keep-running"])
        .expect("keep-running flag should parse");

    assert!(matches!(
        cli.command,
        Some(Commands::Upgrade { keep_running: true })
    ));
}

#[test]
fn desktop_daemon_restart_commands_parse() {
    let stop = Cli::try_parse_from(["zorai", "stop"]).expect("the desktop app runs `zorai stop`");
    assert!(matches!(stop.command, Some(Commands::Stop)));
    let start =
        Cli::try_parse_from(["zorai", "daemon"]).expect("the desktop app runs `zorai daemon`");
    assert!(matches!(start.command, Some(Commands::StartDaemon)));
}
