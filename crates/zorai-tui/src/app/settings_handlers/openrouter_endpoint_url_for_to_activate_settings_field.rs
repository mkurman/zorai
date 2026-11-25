use super::*;
use crate::providers;
#[path = "activate_advanced_settings_field.rs"]
mod activate_advanced_settings_field;
#[path = "activate_compaction_settings_field.rs"]
mod activate_compaction_settings_field;
#[path = "activate_concierge_settings_field.rs"]
mod activate_concierge_settings_field;
#[path = "activate_database_settings_field.rs"]
mod activate_database_settings_field;
#[path = "activate_features_settings_field.rs"]
mod activate_features_settings_field;
#[path = "activate_gateway_settings_field.rs"]
mod activate_gateway_settings_field;
#[path = "activate_leszy_weles_settings_field.rs"]
mod activate_leszy_weles_settings_field;
#[path = "activate_provider_settings_field.rs"]
mod activate_provider_settings_field;
impl TuiModel {
    fn openrouter_endpoint_url_for(model: &str, base_url: &str) -> Option<String> {
        let (author, slug) = model.trim().split_once('/')?;
        if author.trim().is_empty() || slug.trim().is_empty() {
            return None;
        }
        let base_url = if base_url.trim().is_empty() {
            providers::find_by_id(PROVIDER_ID_OPENROUTER)
                .map(|def| def.default_base_url)
                .unwrap_or("https://openrouter.ai/api/v1")
        } else {
            base_url.trim()
        }
        .trim_end_matches('/');
        let base_url = base_url
            .strip_suffix("/chat/completions")
            .or_else(|| base_url.strip_suffix("/responses"))
            .unwrap_or(base_url);
        Some(format!("{base_url}/models/{author}/{slug}/endpoints"))
    }

    fn fetch_openrouter_endpoint_provider_slugs_for(
        model: &str,
        base_url: &str,
        api_key: &str,
    ) -> Result<Vec<String>, String> {
        let url = Self::openrouter_endpoint_url_for(model, base_url)
            .ok_or_else(|| "OpenRouter model id must look like author/model".to_string())?;
        let mut request = ureq::get(&url)
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(8)))
            .build();
        if !api_key.trim().is_empty() {
            request = request.header("Authorization", format!("Bearer {}", api_key.trim()));
        }
        let mut response = request.call().map_err(|error| error.to_string())?;
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| error.to_string())?;
        let payload: serde_json::Value =
            serde_json::from_str(&body).map_err(|error| error.to_string())?;
        let mut slugs = Vec::new();
        if let Some(endpoints) = payload
            .get("data")
            .and_then(|data| data.get("endpoints"))
            .and_then(|endpoints| endpoints.as_array())
        {
            for endpoint in endpoints {
                let Some(slug) = endpoint
                    .get("tag")
                    .and_then(|value| value.as_str())
                    .or_else(|| endpoint.get("slug").and_then(|value| value.as_str()))
                    .or_else(|| {
                        endpoint
                            .get("provider_slug")
                            .and_then(|value| value.as_str())
                    })
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                if !slugs.iter().any(|existing| existing == slug) {
                    slugs.push(slug.to_string());
                }
            }
        }
        Ok(slugs)
    }

    pub(crate) fn open_openrouter_provider_picker_for(
        &mut self,
        target: SettingsPickerTarget,
        model: String,
        base_url: String,
        api_key: String,
    ) {
        match Self::fetch_openrouter_endpoint_provider_slugs_for(&model, &base_url, &api_key) {
            Ok(slugs) if !slugs.is_empty() => {
                self.config.openrouter_endpoint_providers = slugs;
                self.settings_picker_target = Some(target);
                self.modal.reduce(modal::ModalAction::Push(
                    modal::ModalKind::OpenRouterProviderPicker,
                ));
                self.sync_openrouter_provider_picker_item_count();
                self.status_line = "OpenRouter endpoint providers loaded".to_string();
            }
            Ok(_) => {
                self.config.openrouter_endpoint_providers.clear();
                self.status_line =
                    "OpenRouter returned no endpoint providers for this model".to_string();
            }
            Err(error) => {
                self.config.openrouter_endpoint_providers.clear();
                self.status_line = format!("OpenRouter provider lookup failed: {error}");
            }
        }
    }

    fn open_openrouter_provider_picker(&mut self, target: SettingsPickerTarget) {
        if self.config.provider != PROVIDER_ID_OPENROUTER {
            self.status_line = "OpenRouter provider routing only applies to OpenRouter".to_string();
            return;
        }
        self.open_openrouter_provider_picker_for(
            target,
            self.config.model.clone(),
            self.config.base_url.clone(),
            self.config.api_key.clone(),
        );
    }

    pub(crate) fn activate_settings_field(&mut self) {
        let field = self.current_settings_field_name().to_string();
        let field = field.as_str();
        if self.activate_mlflow_settings_field(field)
            || self.activate_provider_settings_field(field)
            || self.activate_gateway_settings_field(field)
            || self.activate_features_settings_field(field)
            || self.activate_advanced_settings_field(field)
            || self.activate_compaction_settings_field(field)
            || self.activate_concierge_settings_field(field)
            || self.activate_leszy_weles_settings_field(field)
            || self.activate_database_settings_field(field)
        {
            return;
        }
        let _ = self.activate_feature_settings_field(field);
    }

    fn activate_mlflow_settings_field(&mut self, field: &str) -> bool {
        match field {
            "mlflow_tracking_uri" => {
                self.settings
                    .start_editing(field, &self.config.mlflow_tracking_uri.clone());
            }
            "mlflow_experiment_name" => {
                self.settings
                    .start_editing(field, &self.config.mlflow_experiment_name.clone());
            }
            "mlflow_experiment_id" => {
                self.settings
                    .start_editing(field, &self.config.mlflow_experiment_id.clone());
            }
            "mlflow_test_connection" => {
                self.send_daemon_command(DaemonCommand::TestMlflowTracingConnection);
                self.status_line = "Testing MLflow connection…".to_string();
            }
            "mlflow_send_test_trace" => {
                self.send_daemon_command(DaemonCommand::SendMlflowTracingTestTrace);
                self.status_line = "Sending MLflow test trace…".to_string();
            }
            _ => return false,
        }
        true
    }
}
