use crate::client::DaemonClient;
use anyhow::Result;
use zorai_protocol::ClientMessage;
impl DaemonClient {
    pub fn get_operator_profile_summary(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetOperatorProfileSummary)
    }

    pub fn answer_operator_question(&self, question_id: String, answer: String) -> Result<()> {
        self.send(ClientMessage::AgentAnswerQuestion {
            question_id,
            answer,
        })
    }

    pub fn get_operator_model(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetOperatorModel)
    }

    pub fn reset_operator_model(&self) -> Result<()> {
        self.send(ClientMessage::AgentResetOperatorModel)
    }

    pub fn get_collaboration_sessions(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetCollaborationSessions {
            parent_task_id: None,
        })
    }

    pub fn vote_on_collaboration_disagreement(
        &self,
        parent_task_id: String,
        disagreement_id: String,
        task_id: String,
        position: String,
        confidence: Option<f64>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentVoteOnCollaborationDisagreement {
            parent_task_id,
            disagreement_id,
            task_id,
            position,
            confidence,
        })
    }

    pub fn get_generated_tools(&self) -> Result<()> {
        self.send(ClientMessage::AgentListGeneratedTools)
    }

    pub fn speech_to_text(&self, args_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSpeechToText { args_json })
    }

    pub fn text_to_speech(&self, args_json: String) -> Result<()> {
        self.send(ClientMessage::AgentTextToSpeech { args_json })
    }

    pub fn generate_image(&self, args_json: String) -> Result<()> {
        self.send(ClientMessage::AgentGenerateImage { args_json })
    }

    pub fn set_operator_profile_consent(&self, consent_key: String, granted: bool) -> Result<()> {
        self.send(ClientMessage::AgentSetOperatorProfileConsent {
            consent_key,
            granted,
        })
    }

    pub fn dismiss_audit_entry(&self, entry_id: String) -> Result<()> {
        self.send(ClientMessage::AuditDismiss { entry_id })
    }

    pub fn plugin_list(&self) -> Result<()> {
        self.send(ClientMessage::PluginList {})
    }

    pub fn plugin_list_commands(&self) -> Result<()> {
        self.send(ClientMessage::PluginListCommands {})
    }

    pub fn plugin_get(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginGet { name })
    }

    pub fn plugin_enable(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginEnable { name })
    }

    pub fn plugin_disable(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginDisable { name })
    }

    pub fn plugin_get_settings(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginGetSettings { name })
    }

    pub fn plugin_update_setting(
        &self,
        plugin_name: String,
        key: String,
        value: String,
        is_secret: bool,
    ) -> Result<()> {
        self.send(ClientMessage::PluginUpdateSettings {
            plugin_name,
            key,
            value,
            is_secret,
        })
    }

    pub fn plugin_test_connection(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginTestConnection { name })
    }

    pub fn plugin_oauth_start(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginOAuthStart { name })
    }

    pub fn whatsapp_link_start(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkStart)
    }

    pub fn whatsapp_link_stop(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkStop)
    }

    pub fn whatsapp_link_status(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkStatus)
    }

    pub fn list_gateway_chats(&self) -> Result<()> {
        self.send(ClientMessage::AgentListGatewayChats)
    }

    pub fn link_thread_to_gateway_chat(
        &self,
        thread_id: String,
        channel_key: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentLinkThreadToGatewayChat {
            thread_id,
            channel_key,
        })
    }

    pub fn whatsapp_link_subscribe(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkSubscribe)
    }

    pub fn whatsapp_link_unsubscribe(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkUnsubscribe)
    }

    pub fn whatsapp_link_reset(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkReset)
    }

    pub fn database_sync_now(&self) -> Result<()> {
        self.send(ClientMessage::DatabaseSyncNow)
    }

    pub fn database_get_backend(&self) -> Result<()> {
        self.send(ClientMessage::DatabaseGetBackend)
    }

    pub fn list_task_approval_rules(&self) -> Result<()> {
        self.send(ClientMessage::AgentListTaskApprovalRules)
    }

    pub fn create_task_approval_rule(&self, approval_id: String) -> Result<()> {
        self.send(ClientMessage::AgentCreateTaskApprovalRule { approval_id })
    }

    pub fn revoke_task_approval_rule(&self, rule_id: String) -> Result<()> {
        self.send(ClientMessage::AgentRevokeTaskApprovalRule { rule_id })
    }

    pub fn resolve_task_approval(&self, approval_id: String, decision: String) -> Result<()> {
        let decision = match decision.as_str() {
            "allow_once" | "approve_once" => "approve-once",
            "allow_session" | "approve_session" => "approve-session",
            _ => "deny",
        };
        self.send(ClientMessage::AgentResolveTaskApproval {
            approval_id,
            decision: decision.to_string(),
        })
    }
}
