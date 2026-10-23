use super::make_model;
use super::whatsapp_link_device_probes_status_before_starting_link_flow::focus_settings_field;
use super::*;
#[test]
fn collaboration_sessions_inspect_field_requests_collaboration_snapshot() {
    let (mut model, mut daemon_rx) = make_model();
    focus_settings_field(
        &mut model,
        SettingsTab::Workflows,
        "collaboration_sessions_inspect",
    );
    assert_eq!(
        model.settings.current_field_name(),
        "collaboration_sessions_inspect"
    );

    model.activate_settings_field();

    assert!(matches!(model.main_pane_view, MainPaneView::Collaboration));

    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected collaboration inspect command"),
        DaemonCommand::GetCollaborationSessions
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn generated_tools_inspect_field_requests_generated_tools_snapshot() {
    let (mut model, mut daemon_rx) = make_model();
    focus_settings_field(&mut model, SettingsTab::Tools, "generated_tools_inspect");
    assert_eq!(
        model.settings.current_field_name(),
        "generated_tools_inspect"
    );

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected generated tools inspect command"),
        DaemonCommand::GetGeneratedTools
    ));
    assert!(daemon_rx.try_recv().is_err());
}
