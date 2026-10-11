use super::CommandItem;

pub(super) fn default_command_items() -> Vec<CommandItem> {
    vec![
        CommandItem {
            command: "provider".into(),
            description: "Switch Svarog's provider".into(),
        },
        CommandItem {
            command: "model".into(),
            description: "Switch Svarog's model".into(),
        },
        CommandItem {
            command: "image".into(),
            description: "Compose an image generation prompt".into(),
        },
        CommandItem {
            command: "tools".into(),
            description: "Toggle tool categories".into(),
        },
        CommandItem {
            command: "effort".into(),
            description: "Set Svarog's reasoning effort".into(),
        },
        CommandItem {
            command: "context".into(),
            description: "Set the current context window".into(),
        },
        CommandItem {
            command: "thread".into(),
            description: "Pick conversation thread".into(),
        },
        CommandItem {
            command: "new".into(),
            description: "New conversation".into(),
        },
        CommandItem {
            command: "new-goal".into(),
            description: "Open new goal composer".into(),
        },
        CommandItem {
            command: "workspace".into(),
            description: "Open workspace board".into(),
        },
        CommandItem {
            command: "new-workspace".into(),
            description: "Open workspace creator".into(),
        },
        CommandItem {
            command: "workspace-update".into(),
            description: "Seed workspace task update".into(),
        },
        CommandItem {
            command: "goal".into(),
            description: "Open goal picker".into(),
        },
        CommandItem {
            command: "conversation".into(),
            description: "Return to conversation view".into(),
        },
        CommandItem {
            command: "view".into(),
            description: "Switch transcript mode".into(),
        },
        CommandItem {
            command: "status".into(),
            description: "Show zorai status".into(),
        },
        CommandItem {
            command: "statistics".into(),
            description: "Show DB-backed usage statistics".into(),
        },
        CommandItem {
            command: "notifications".into(),
            description: "Open notifications center".into(),
        },
        CommandItem {
            command: "approvals".into(),
            description: "Open approvals center".into(),
        },
        CommandItem {
            command: "participants".into(),
            description: "Show thread participants".into(),
        },
        CommandItem {
            command: "handoff".into(),
            description: "Hand off or return current thread".into(),
        },
        CommandItem {
            command: "gateway".into(),
            description: "Continue current thread on a messaging gateway chat".into(),
        },
        CommandItem {
            command: "compact".into(),
            description: "Force compact current thread".into(),
        },
        CommandItem {
            command: "settings".into(),
            description: "Open settings panel".into(),
        },
        CommandItem {
            command: "prompt".into(),
            description: "Inspect assembled system prompt".into(),
        },
        CommandItem {
            command: "attach".into(),
            description: "Attach a file to the message".into(),
        },
        CommandItem {
            command: "plugins".into(),
            description: "Open plugin settings".into(),
        },
        CommandItem {
            command: "plugins install".into(),
            description: "Seed plugin install command".into(),
        },
        CommandItem {
            command: "skills install".into(),
            description: "Seed community skill install command".into(),
        },
        CommandItem {
            command: "guidelines install".into(),
            description: "Seed custom guideline install command".into(),
        },
        CommandItem {
            command: "quit".into(),
            description: "Exit TUI".into(),
        },
        CommandItem {
            command: "help".into(),
            description: "Show keyboard shortcuts".into(),
        },
        CommandItem {
            command: "explain".into(),
            description: "Explain latest goal-run decision".into(),
        },
        CommandItem {
            command: "diverge".into(),
            description: "Seed divergent session command".into(),
        },
    ]
}
