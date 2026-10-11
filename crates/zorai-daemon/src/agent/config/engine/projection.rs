#![allow(dead_code)]

use super::super::*;

impl AgentEngine {
    pub(crate) async fn current_desired_config_snapshot(&self) -> AgentConfig {
        self.get_config().await
    }

    pub(crate) async fn current_effective_config_runtime_state(
        &self,
    ) -> ConfigEffectiveRuntimeState {
        ConfigEffectiveRuntimeState {
            reconcile: self.current_config_runtime_projection().await,
            gateway_runtime_connected: self.gateway_ipc_sender.lock().await.is_some(),
        }
    }

    pub(crate) async fn current_config_runtime_projection(&self) -> ConfigRuntimeProjection {
        self.config_runtime_projection.lock().await.clone()
    }

    pub(in crate::agent) async fn persist_sanitized_config(
        &self,
        config: AgentConfig,
        collisions: Vec<SubAgentDefinition>,
    ) -> AgentConfig {
        let mut config = config;
        sanitize_weles_builtin_overrides_struct(
            &mut config.builtin_sub_agents.weles,
            &config.system_prompt,
        );
        let items = config_to_items(&config);
        if let Err(error) = self.history.replace_agent_config_items(&items).await {
            tracing::warn!("failed to persist agent config to sqlite: {error}");
        }
        crate::agent::agent_identity::set_agent_aliases(&config.agent_aliases);
        *self.config.write().await = config.clone();
        self.mlflow_tracing
            .reconfigure(config.mlflow_tracing.clone())
            .await;
        self.config_notify.notify_waiters();
        self.report_weles_collisions_once(&collisions).await;
        config
    }

    pub(in crate::agent) async fn store_config_snapshot(&self, config: AgentConfig) -> AgentConfig {
        let mut config = config;
        let collisions = sanitize_weles_collisions_from_config(&mut config);
        self.persist_sanitized_config(config, collisions).await
    }

    async fn audit_weles_collision(&self, def: &SubAgentDefinition) {
        let audit_entry = crate::history::AuditEntryRow {
            id: format!("audit-subagent-weles-collision-{}", uuid::Uuid::new_v4()),
            timestamp: now_millis_u64() as i64,
            action_type: "subagent".to_string(),
            summary: format!(
                "Excluded legacy WELES collision from effective registry: {} ({})",
                def.name, def.id
            ),
            explanation: Some(
                "A persisted user subagent collided with the daemon-owned WELES registry entry and was excluded."
                    .to_string(),
            ),
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            goal_run_id: None,
            task_id: None,
            raw_data_json: Some(
                serde_json::json!({
                    "collision_id": def.id,
                    "collision_name": def.name,
                    "reserved_id": WELES_BUILTIN_ID,
                    "reserved_name": WELES_BUILTIN_NAME,
                })
                .to_string(),
            ),
        };
        if let Err(error) = self.history.insert_action_audit(&audit_entry).await {
            tracing::warn!(error = %error, sub_agent_id = %def.id, "failed to persist WELES collision audit entry");
        }
    }

    async fn report_weles_collisions_once(&self, collisions: &[SubAgentDefinition]) {
        for collision in collisions {
            tracing::warn!(
                sub_agent_id = %collision.id,
                sub_agent_name = %collision.name,
                "excluding legacy WELES collision from effective registry"
            );
            self.audit_weles_collision(collision).await;
        }
    }

    pub(crate) async fn effective_sub_agents(&self) -> Vec<SubAgentDefinition> {
        let config = self.config.read().await;
        let (effective, _) = effective_sub_agents_from_config(&config);
        effective
    }

    pub async fn get_config(&self) -> AgentConfig {
        self.config.read().await.clone()
    }

    pub async fn set_config(&self, config: AgentConfig) {
        self.store_config_snapshot(config).await;
    }

    pub async fn get_sub_agent(&self, id: &str) -> Option<SubAgentDefinition> {
        self.list_sub_agents()
            .await
            .into_iter()
            .find(|entry| entry.id == id)
    }

    pub async fn set_config_item_json(
        &self,
        key_path: &str,
        value_json: &str,
    ) -> Result<AgentConfig> {
        let (merged, value) = self.prepare_config_item_json(key_path, value_json).await?;
        self.persist_prepared_config_item_json(key_path, &value, merged.clone())
            .await?;
        self.reconcile_config_runtime_after_commit().await?;
        Ok(self.get_config().await)
    }

    pub async fn prepare_config_item_json(
        &self,
        key_path: &str,
        value_json: &str,
    ) -> Result<(AgentConfig, Value)> {
        let value =
            serde_json::from_str::<Value>(value_json).context("invalid config item JSON")?;
        let mut merged_value = serde_json::to_value(self.get_config().await)?;
        normalize_config_keys_to_snake_case(&mut merged_value);
        set_config_value_at_pointer(&mut merged_value, key_path, value.clone())?;
        sanitize_config_value(&mut merged_value);
        let merged = serde_json::from_value::<AgentConfig>(merged_value)
            .context("updated config item could not be parsed")?;
        crate::agent::agent_identity::validate_agent_aliases(&merged.agent_aliases, &merged.sub_agents)?;
        Ok((merged, value))
    }

    pub async fn persist_prepared_config_item_json(
        &self,
        key_path: &str,
        value: &Value,
        merged: AgentConfig,
    ) -> Result<()> {
        self.history
            .upsert_agent_config_item(key_path, &value)
            .await
            .context("failed to persist config item update")?;

        let mut merged = merged;
        let collisions = sanitize_weles_collisions_from_config(&mut merged);
        self.persist_sanitized_config(merged, collisions).await;

        let mut projection = self.config_runtime_projection.lock().await;
        projection.desired_revision = projection.desired_revision.saturating_add(1);
        projection.state = ConfigReconcileState::Reconciling;
        projection.last_error = None;
        Ok(())
    }

    pub async fn reconcile_config_runtime_after_commit(&self) -> Result<()> {
        #[cfg(test)]
        if let Some(delay) = self.take_test_config_reconcile_delay().await {
            tokio::time::sleep(delay).await;
        }

        #[cfg(test)]
        if let Some(error) = self.take_test_config_reconcile_failure().await {
            let mut projection = self.config_runtime_projection.lock().await;
            projection.state = ConfigReconcileState::Error;
            projection.last_error = Some(error.clone());
            return Err(anyhow::anyhow!(error));
        }

        let degraded_reason = self.reinit_gateway().await;
        let mut projection = self.config_runtime_projection.lock().await;
        if let Some(reason) = degraded_reason {
            projection.state = ConfigReconcileState::Degraded;
            projection.last_error = Some(reason);
        } else {
            projection.effective_revision = projection.desired_revision;
            projection.state = ConfigReconcileState::Applied;
            projection.last_error = None;
        }
        Ok(())
    }

    #[cfg(test)]
    pub async fn set_test_config_reconcile_delay(&self, delay: Option<std::time::Duration>) {
        let key = self as *const _ as usize;
        let mut delays = config_reconcile_delay_map()
            .lock()
            .expect("config reconcile delay map mutex poisoned");
        if let Some(delay) = delay {
            delays.insert(key, delay);
        } else {
            delays.remove(&key);
        }
    }

    #[cfg(test)]
    pub async fn set_test_config_reconcile_failure(&self, error: Option<String>) {
        let key = self as *const _ as usize;
        let mut failures = config_reconcile_failure_map()
            .lock()
            .expect("config reconcile failure map mutex poisoned");
        if let Some(error) = error {
            failures.insert(key, error);
        } else {
            failures.remove(&key);
        }
    }

    #[cfg(test)]
    async fn take_test_config_reconcile_delay(&self) -> Option<std::time::Duration> {
        let key = self as *const _ as usize;
        config_reconcile_delay_map()
            .lock()
            .expect("config reconcile delay map mutex poisoned")
            .remove(&key)
    }

    #[cfg(test)]
    async fn take_test_config_reconcile_failure(&self) -> Option<String> {
        let key = self as *const _ as usize;
        config_reconcile_failure_map()
            .lock()
            .expect("config reconcile failure map mutex poisoned")
            .remove(&key)
    }
}
