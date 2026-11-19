use super::*;
#[derive(Default)]
pub(crate) struct OperationRegistry {
    records: std::sync::Mutex<std::collections::HashMap<String, OperationRecord>>,
    dedup_index: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl OperationRegistry {
    pub(crate) fn accept_operation(&self, kind: &str, dedup: Option<String>) -> OperationRecord {
        self.accept_operation_with_dedup_status(kind, dedup).0
    }

    /// Same as `accept_operation` but also reports whether the returned record
    /// is a newly-created operation (`true`) or an existing one that was
    /// deduplicated (`false`). Callers that spawn background side-effects
    /// should skip the spawn when this returns `false` — the prior spawn is
    /// still running or pending.
    pub(crate) fn accept_operation_with_dedup_status(
        &self,
        kind: &str,
        dedup: Option<String>,
    ) -> (OperationRecord, bool) {
        if let Some(existing) = dedup.as_ref().and_then(|dedup_key| {
            let dedup_index = self
                .dedup_index
                .lock()
                .expect("operation dedup mutex poisoned");
            let operation_id = dedup_index.get(dedup_key)?.clone();
            drop(dedup_index);
            self.snapshot(&operation_id).and_then(|snapshot| {
                if matches!(
                    snapshot.state,
                    zorai_protocol::OperationLifecycleState::Accepted
                        | zorai_protocol::OperationLifecycleState::Started
                ) {
                    self.record(&operation_id)
                } else {
                    None
                }
            })
        }) {
            return (existing, false);
        }

        let record = OperationRecord {
            operation_id: uuid::Uuid::new_v4().to_string(),
            kind: kind.to_string(),
            dedup: dedup.clone(),
            state: zorai_protocol::OperationLifecycleState::Accepted,
            revision: 0,
            terminal_result: None,
        };

        {
            let mut records = self
                .records
                .lock()
                .expect("operation records mutex poisoned");
            records.insert(record.operation_id.clone(), record.clone());
        }

        subsystem_metrics().record_operation_accepted(kind, &record.operation_id);

        if let Some(dedup_key) = dedup {
            let mut dedup_index = self
                .dedup_index
                .lock()
                .expect("operation dedup mutex poisoned");
            dedup_index.insert(dedup_key, record.operation_id.clone());
        }

        (record, true)
    }

    pub(crate) fn restore_operation(&self, operation_id: &str, kind: &str) {
        let mut records = self
            .records
            .lock()
            .expect("operation records mutex poisoned");
        records
            .entry(operation_id.to_string())
            .or_insert_with(|| OperationRecord {
                operation_id: operation_id.to_string(),
                kind: kind.to_string(),
                dedup: None,
                state: zorai_protocol::OperationLifecycleState::Accepted,
                revision: 0,
                terminal_result: None,
            });
    }

    pub(crate) fn mark_started(&self, operation_id: &str) {
        self.update_state(
            operation_id,
            zorai_protocol::OperationLifecycleState::Started,
            None,
        );
    }

    pub(crate) fn mark_completed(&self, operation_id: &str) {
        self.update_state(
            operation_id,
            zorai_protocol::OperationLifecycleState::Completed,
            None,
        );
    }

    pub(crate) fn mark_failed(&self, operation_id: &str) {
        self.update_state(
            operation_id,
            zorai_protocol::OperationLifecycleState::Failed,
            None,
        );
    }

    pub(crate) fn mark_completed_with_terminal_result(
        &self,
        operation_id: &str,
        terminal_result: serde_json::Value,
    ) {
        self.update_state(
            operation_id,
            zorai_protocol::OperationLifecycleState::Completed,
            Some(terminal_result),
        );
    }

    pub(crate) fn mark_failed_with_terminal_result(
        &self,
        operation_id: &str,
        terminal_result: serde_json::Value,
    ) {
        self.update_state(
            operation_id,
            zorai_protocol::OperationLifecycleState::Failed,
            Some(terminal_result),
        );
    }

    pub(crate) fn snapshot(
        &self,
        operation_id: &str,
    ) -> Option<zorai_protocol::OperationStatusSnapshot> {
        self.record(operation_id).map(|record| record.snapshot())
    }

    pub(crate) fn terminal_result(&self, operation_id: &str) -> Option<serde_json::Value> {
        self.record(operation_id)
            .and_then(|record| record.terminal_result)
    }

    pub(crate) fn resolve_unique_id_by_first_segment(&self, query: &str) -> Option<String> {
        let first_segment = query
            .split('-')
            .next()
            .filter(|segment| segment.len() >= 8)?;
        let records = self
            .records
            .lock()
            .expect("operation records mutex poisoned");
        let mut candidates = records
            .keys()
            .filter(|id| id.split('-').next() == Some(first_segment));
        let resolved = candidates.next()?.clone();
        candidates.next().is_none().then_some(resolved)
    }

    fn record(&self, operation_id: &str) -> Option<OperationRecord> {
        let records = self
            .records
            .lock()
            .expect("operation records mutex poisoned");
        records.get(operation_id).cloned()
    }

    fn update_state(
        &self,
        operation_id: &str,
        state: zorai_protocol::OperationLifecycleState,
        terminal_result: Option<serde_json::Value>,
    ) {
        let mut dedup_to_release = None;
        let mut should_record_started = false;
        let mut terminal_failed = None;

        {
            let mut records = self
                .records
                .lock()
                .expect("operation records mutex poisoned");
            if let Some(record) = records.get_mut(operation_id) {
                let state_changed = record.state != state;
                let payload_changed = terminal_result
                    .as_ref()
                    .is_some_and(|value| record.terminal_result.as_ref() != Some(value));

                if state_changed || payload_changed {
                    record.state = state;
                    if let Some(terminal_result) = terminal_result {
                        record.terminal_result = Some(terminal_result);
                    }
                    record.revision = record.revision.saturating_add(1);

                    if state_changed
                        && matches!(state, zorai_protocol::OperationLifecycleState::Started)
                    {
                        should_record_started = true;
                    }

                    if state_changed
                        && matches!(
                            state,
                            zorai_protocol::OperationLifecycleState::Completed
                                | zorai_protocol::OperationLifecycleState::Failed
                        )
                    {
                        terminal_failed = Some(matches!(
                            state,
                            zorai_protocol::OperationLifecycleState::Failed
                        ));
                    }

                    if state_changed
                        && matches!(
                            state,
                            zorai_protocol::OperationLifecycleState::Completed
                                | zorai_protocol::OperationLifecycleState::Failed
                        )
                        && retention_policy_for_kind(&record.kind).release_dedup_on_terminal
                    {
                        dedup_to_release = record.dedup.clone();
                    }
                }
            }
        }

        if should_record_started {
            subsystem_metrics().record_operation_started(operation_id);
        }

        if let Some(failed) = terminal_failed {
            subsystem_metrics().record_operation_terminal(operation_id, failed);
        }

        if let Some(dedup_key) = dedup_to_release {
            let mut dedup_index = self
                .dedup_index
                .lock()
                .expect("operation dedup mutex poisoned");
            if dedup_index
                .get(&dedup_key)
                .map(|existing| existing == operation_id)
                .unwrap_or(false)
            {
                dedup_index.remove(&dedup_key);
            }
        }
    }
}

#[cfg(test)]
mod operation_registry_tests {
    use super::*;

    #[test]
    fn terminal_operations_remain_queryable_but_release_dedup_slot() {
        let registry = OperationRegistry::default();
        let first = registry.accept_operation(
            zorai_protocol::tool_names::PLUGIN_API_CALL,
            Some("plugin:dedup".to_string()),
        );

        registry.mark_completed(&first.operation_id);

        let completed = registry
            .snapshot(&first.operation_id)
            .expect("completed operation should remain queryable");
        assert_eq!(
            completed.state,
            zorai_protocol::OperationLifecycleState::Completed
        );

        let second = registry.accept_operation(
            zorai_protocol::tool_names::PLUGIN_API_CALL,
            Some("plugin:dedup".to_string()),
        );
        assert_ne!(first.operation_id, second.operation_id);
        assert_eq!(
            second.state,
            zorai_protocol::OperationLifecycleState::Accepted
        );
    }

    #[test]
    fn mistyped_operation_id_resolves_by_unique_first_segment() {
        let registry = OperationRegistry::default();
        let record = registry.accept_operation(zorai_protocol::tool_names::BASH_COMMAND, None);
        let first_segment = record
            .operation_id
            .split('-')
            .next()
            .expect("uuid should have a first segment");

        let mangled = format!("{first_segment}-0000-0000-0000-000000000000");
        assert_eq!(
            registry.resolve_unique_id_by_first_segment(&mangled),
            Some(record.operation_id.clone()),
            "a mangled tail with a correct leading segment should resolve to the registered operation"
        );

        assert_eq!(
            registry.resolve_unique_id_by_first_segment("abc-0000"),
            None,
            "segments shorter than a uuid segment should not resolve"
        );
    }

    #[test]
    fn ambiguous_first_segment_does_not_resolve() {
        let registry = OperationRegistry::default();
        for suffix in ["111111111111", "222222222222"] {
            let operation_id = format!("aabbccdd-0000-4000-8000-{suffix}");
            registry
                .records
                .lock()
                .expect("operation records mutex poisoned")
                .insert(
                    operation_id.clone(),
                    OperationRecord {
                        operation_id,
                        kind: zorai_protocol::tool_names::BASH_COMMAND.to_string(),
                        dedup: None,
                        state: zorai_protocol::OperationLifecycleState::Accepted,
                        revision: 0,
                        terminal_result: None,
                    },
                );
        }

        assert_eq!(
            registry.resolve_unique_id_by_first_segment("aabbccdd-9999-9999-9999-999999999999"),
            None,
            "prefix resolution must refuse to guess between multiple operations sharing a leading segment"
        );
    }

    #[test]
    fn retention_policy_is_explicit_for_migrated_operation_kinds() {
        let policy = retention_policy_for_kind(OPERATION_KIND_PLUGIN_API_CALL);
        assert_eq!(
            policy.terminal_visibility,
            OperationTerminalVisibility::UntilProcessExit
        );
        assert_eq!(
            policy.interrupted_visibility,
            InterruptedVisibility::ReconnectOnly
        );
        assert!(!policy.survives_process_restart);
        assert!(policy.release_dedup_on_terminal);

        let config_policy = retention_policy_for_kind(OPERATION_KIND_CONFIG_SET_ITEM);
        assert_eq!(
            config_policy.terminal_visibility,
            OperationTerminalVisibility::UntilProcessExit
        );
        assert_eq!(
            config_policy.interrupted_visibility,
            InterruptedVisibility::ReconnectOnly
        );
        assert!(!config_policy.survives_process_restart);
        assert!(config_policy.release_dedup_on_terminal);

        let set_sub_agent_policy = retention_policy_for_kind(OPERATION_KIND_SET_SUB_AGENT);
        assert_eq!(
            set_sub_agent_policy.terminal_visibility,
            OperationTerminalVisibility::UntilProcessExit
        );
        assert_eq!(
            set_sub_agent_policy.interrupted_visibility,
            InterruptedVisibility::ReconnectOnly
        );
        assert!(!set_sub_agent_policy.survives_process_restart);
        assert!(set_sub_agent_policy.release_dedup_on_terminal);

        let remove_sub_agent_policy = retention_policy_for_kind(OPERATION_KIND_REMOVE_SUB_AGENT);
        assert_eq!(
            remove_sub_agent_policy.terminal_visibility,
            OperationTerminalVisibility::UntilProcessExit
        );
        assert_eq!(
            remove_sub_agent_policy.interrupted_visibility,
            InterruptedVisibility::ReconnectOnly
        );
        assert!(!remove_sub_agent_policy.survives_process_restart);
        assert!(remove_sub_agent_policy.release_dedup_on_terminal);
    }

    #[test]
    fn subsystem_metrics_track_operation_lifecycle_counts_and_latencies() {
        let before = subsystem_metrics().snapshot_for(BackgroundSubsystem::PluginIo);

        let registry = OperationRegistry::default();
        let record = registry.accept_operation(OPERATION_KIND_PLUGIN_API_CALL, None);
        std::thread::sleep(std::time::Duration::from_millis(5));
        registry.mark_started(&record.operation_id);
        std::thread::sleep(std::time::Duration::from_millis(5));
        registry.mark_failed(&record.operation_id);

        let snapshot = subsystem_metrics().snapshot_for(BackgroundSubsystem::PluginIo);
        assert!(snapshot.accepted_count >= before.accepted_count.saturating_add(1));
        assert!(snapshot.started_count >= before.started_count.saturating_add(1));
        assert!(snapshot.failed_count >= before.failed_count.saturating_add(1));
        assert!(snapshot.completed_count >= before.completed_count);
        assert!(
            snapshot.accepted_to_started_samples
                >= before.accepted_to_started_samples.saturating_add(1)
        );
        assert!(
            snapshot.started_to_terminal_samples
                >= before.started_to_terminal_samples.saturating_add(1)
        );
        assert!(snapshot.last_accepted_to_started_ms.is_some());
        assert!(snapshot.last_started_to_terminal_ms.is_some());
    }

    #[test]
    fn shutdown_policy_is_explicit_for_ephemeral_and_config_reconcile_work() {
        let ephemeral = retention_policy_for_kind(OPERATION_KIND_PLUGIN_API_CALL);
        assert_eq!(ephemeral.durability, OperationDurability::Ephemeral);
        assert_eq!(
            ephemeral.accepted_shutdown,
            ShutdownDisposition::LostOnDaemonStop
        );
        assert_eq!(
            ephemeral.started_shutdown,
            ShutdownDisposition::LostOnDaemonStop
        );

        let config = retention_policy_for_kind(OPERATION_KIND_CONFIG_SET_ITEM);
        assert_eq!(config.durability, OperationDurability::DesiredStateDurable);
        assert_eq!(
            config.accepted_shutdown,
            ShutdownDisposition::DesiredStateRemainsNeedsReconcile
        );
        assert_eq!(
            config.started_shutdown,
            ShutdownDisposition::DesiredStateRemainsNeedsReconcile
        );

        let set_sub_agent = retention_policy_for_kind(OPERATION_KIND_SET_SUB_AGENT);
        assert_eq!(
            set_sub_agent.durability,
            OperationDurability::DesiredStateDurable
        );
        assert_eq!(
            set_sub_agent.accepted_shutdown,
            ShutdownDisposition::DesiredStateRemainsNeedsReconcile
        );
        assert_eq!(
            set_sub_agent.started_shutdown,
            ShutdownDisposition::DesiredStateRemainsNeedsReconcile
        );

        let remove_sub_agent = retention_policy_for_kind(OPERATION_KIND_REMOVE_SUB_AGENT);
        assert_eq!(
            remove_sub_agent.durability,
            OperationDurability::DesiredStateDurable
        );
        assert_eq!(
            remove_sub_agent.accepted_shutdown,
            ShutdownDisposition::DesiredStateRemainsNeedsReconcile
        );
        assert_eq!(
            remove_sub_agent.started_shutdown,
            ShutdownDisposition::DesiredStateRemainsNeedsReconcile
        );
    }

    #[test]
    fn superseded_state_policy_is_explicit_for_phase_two() {
        let plugin = retention_policy_for_kind(OPERATION_KIND_PLUGIN_API_CALL);
        assert_eq!(
            plugin.supersession,
            SupersessionPolicy::NotEmittedInPhaseTwo
        );

        let config = retention_policy_for_kind(OPERATION_KIND_CONFIG_SET_ITEM);
        assert_eq!(
            config.supersession,
            SupersessionPolicy::NotEmittedInPhaseTwo
        );

        let set_sub_agent = retention_policy_for_kind(OPERATION_KIND_SET_SUB_AGENT);
        assert_eq!(
            set_sub_agent.supersession,
            SupersessionPolicy::NotEmittedInPhaseTwo
        );

        let remove_sub_agent = retention_policy_for_kind(OPERATION_KIND_REMOVE_SUB_AGENT);
        assert_eq!(
            remove_sub_agent.supersession,
            SupersessionPolicy::NotEmittedInPhaseTwo
        );
    }
}
