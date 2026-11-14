use super::flow::set_config_item;
use super::*;

const WEB_SEARCH_CHOICES: [(&str, &str); 7] = [
    ("Firecrawl", "firecrawl_api_key"),
    ("DuckDuckGo", "duckduckgo"),
    ("Keenable", "keenable_api_key"),
    ("You.com", "youcom_api_key"),
    ("Exa", "exa_api_key"),
    ("Tavily", "tavily_api_key"),
    ("Skip", ""),
];

pub(super) fn web_search_provider_for_key(key_name: &str) -> Option<&'static str> {
    match key_name {
        "firecrawl_api_key" => Some("firecrawl"),
        "exa_api_key" => Some("exa"),
        "tavily_api_key" => Some("tavily"),
        "keenable_api_key" => Some("keenable"),
        "youcom_api_key" => Some("youcom"),
        _ => None,
    }
}

pub(super) fn web_search_choice_items() -> &'static [(&'static str, &'static str)] {
    &WEB_SEARCH_CHOICES
}

pub(super) fn web_search_provider_for_choice(choice: &str) -> Option<&'static str> {
    match choice {
        "duckduckgo" => Some("duckduckgo"),
        key_name => web_search_provider_for_key(key_name),
    }
}

pub(super) fn web_search_api_key_prompt_for_choice(choice: &str) -> Option<&'static str> {
    match choice {
        "firecrawl_api_key" => Some("Enter Firecrawl API key"),
        "exa_api_key" => Some("Enter Exa API key"),
        "tavily_api_key" => Some("Enter Tavily API key"),
        "keenable_api_key" => {
            Some("Enter Keenable API key (optional, leave empty for the free public endpoint)")
        }
        "youcom_api_key" => {
            Some("Enter You.com API key (optional, leave empty for the free keyless endpoint)")
        }
        _ => None,
    }
}

pub(super) async fn configure_web_search(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    summary: &mut SetupSummary,
) -> Result<()> {
    println!();
    let existing_ws = if read_config_key("firecrawl_api_key").await.is_some() {
        Some("Firecrawl")
    } else if read_config_key("exa_api_key").await.is_some() {
        Some("Exa")
    } else if read_config_key("tavily_api_key").await.is_some() {
        Some("Tavily")
    } else if read_config_key("keenable_api_key").await.is_some() {
        Some("Keenable")
    } else if read_config_key("youcom_api_key").await.is_some() {
        Some("You.com")
    } else if let Some(provider) = read_config_key("search_provider").await {
        match provider.as_str() {
            "duckduckgo" => Some("DuckDuckGo"),
            "keenable" => Some("Keenable"),
            "youcom" => Some("You.com"),
            _ => None,
        }
    } else {
        None
    };

    let should_configure = if let Some(provider) = existing_ws {
        println!("Web search is already configured ({provider}).");
        match select_list(
            "Replace web search configuration?",
            &[("No, keep existing", ""), ("Yes, reconfigure", "")],
            true,
            0,
        )? {
            Some(1) => true,
            _ => {
                summary.web_search = Some(provider.to_string());
                false
            }
        }
    } else {
        true
    };

    if should_configure {
        let items = web_search_choice_items();
        match select_list(
            "Configure web search? (enables agent web browsing)",
            items,
            true,
            0,
        )? {
            Some(idx) if idx + 1 < items.len() => {
                let (provider_label, choice) = items[idx];
                let api_key = match web_search_api_key_prompt_for_choice(choice) {
                    Some(prompt) => text_input(&prompt, "", true)?,
                    None => Some(String::new()),
                };
                if let Some(key) = api_key {
                    if !key.is_empty()
                        || choice == "duckduckgo"
                        || choice == "keenable_api_key"
                        || choice == "youcom_api_key"
                    {
                        set_config_item(framed, "/tools/web_search", "true")
                            .await
                            .context("Failed to enable web search")?;
                        if let Some(provider) = web_search_provider_for_choice(choice) {
                            set_config_item(
                                framed,
                                "/search_provider",
                                format!("\"{}\"", provider),
                            )
                            .await
                            .context("Failed to set web search provider")?;
                        }
                        if !key.is_empty() {
                            set_config_item(framed, format!("/{choice}"), format!("\"{}\"", key))
                                .await
                                .context("Failed to set web search API key")?;
                        }
                        summary.web_search = Some(provider_label.to_string());
                        println!("Web search configured with {provider_label}.");
                    } else {
                        println!("Skipped -- you can add web search later with `zorai setup`.");
                    }
                } else {
                    println!("Skipped -- you can add web search later with `zorai setup`.");
                }
            }
            _ => println!("Skipped -- you can add web search later with `zorai setup`."),
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Ok(())
}

pub(super) async fn configure_gateway(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
    summary: &mut SetupSummary,
) -> Result<()> {
    println!();
    let existing_gw = if read_config_key("gateway.slack_token").await.is_some() {
        Some("Slack")
    } else if read_config_key("gateway.discord_token").await.is_some() {
        Some("Discord")
    } else if read_config_key("gateway.telegram_token").await.is_some() {
        Some("Telegram")
    } else {
        None
    };

    let should_configure = if let Some(platform) = existing_gw {
        println!("Gateway is already configured ({platform}).");
        match select_list(
            "Replace gateway configuration?",
            &[("No, keep existing", ""), ("Yes, reconfigure", "")],
            true,
            0,
        )? {
            Some(1) => true,
            _ => {
                summary.gateway = Some(platform.to_string());
                false
            }
        }
    } else {
        true
    };

    if !should_configure {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        return Ok(());
    }

    let gateway_items = gateway_choice_items();
    match select_list(
        "Configure a chat gateway? (Slack, Discord, Telegram, WhatsApp)",
        &gateway_items,
        true,
        0,
    )? {
        Some(idx) if idx + 1 < gateway_items.len() => {
            let (platform_label, platform) = gateway_items[idx];
            if platform == "whatsapp" {
                let raw = loop {
                    println!();
                    match resolve_whatsapp_allowlist_prompt(collect_whatsapp_allowlist_input()?) {
                        WhatsAppAllowlistPromptResolution::Accept(raw) => break Some(raw),
                        WhatsAppAllowlistPromptResolution::Retry(message) => println!("{message}"),
                        WhatsAppAllowlistPromptResolution::Cancel => {
                            println!(
                                "Skipped -- you can configure WhatsApp later with `zorai setup`."
                            );
                            break None;
                        }
                    }
                };
                if let Some(raw) = raw {
                    for write in whatsapp_gateway_config_writes(&raw)? {
                        set_config_item(framed, write.key_path, write.value_json)
                            .await
                            .context("Failed to save WhatsApp gateway setup")?;
                    }
                    let linked = run_whatsapp_link_subflow(framed)
                        .await
                        .context("WhatsApp link flow failed during setup")?;
                    summary.gateway = Some(platform_label.to_string());
                    summary.whatsapp_linked = linked;
                    if linked {
                        println!("WhatsApp gateway linked.");
                    } else {
                        println!("WhatsApp gateway selected (link skipped).");
                    }
                }
            } else if let Some(token) =
                text_input(&format!("Enter {platform_label} token"), "", true)?
            {
                if !token.is_empty() {
                    set_config_item(framed, "/gateway/enabled", "true")
                        .await
                        .context("Failed to enable gateway")?;
                    set_config_item(
                        framed,
                        format!("/gateway/{}_token", platform),
                        format!("\"{}\"", token),
                    )
                    .await
                    .context("Failed to set gateway token")?;
                    summary.gateway = Some(platform_label.to_string());
                    println!("{platform_label} gateway configured.");
                } else {
                    println!("Skipped -- you can configure gateways later with `zorai setup`.");
                }
            } else {
                println!("Skipped -- you can configure gateways later with `zorai setup`.");
            }
        }
        _ => println!("Skipped -- you can configure gateways later with `zorai setup`."),
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    Ok(())
}

pub(super) async fn print_summary_and_choose_action(
    provider_name: &str,
    security_level_label: &str,
    summary: &SetupSummary,
) -> Result<PostSetupAction> {
    println!();
    println!("{}", "Setup complete!".bold());
    println!();
    println!("  Provider:  {provider_name}");
    println!("  Security:  {security_level_label}");
    if let Some(ref model) = summary.model {
        println!("  Model:     {model}");
    }
    if let Some(ref ws) = summary.web_search {
        println!("  Web Search: Enabled ({ws})");
    }
    if let Some(ref gw) = summary.gateway {
        if gw == "WhatsApp" {
            if summary.whatsapp_linked {
                println!("  Gateway:   WhatsApp linked");
            } else {
                println!("  Gateway:   WhatsApp selected (link skipped)");
            }
        } else {
            println!("  Gateway:   {gw} configured");
        }
    }
    if let Some(ref concierge) = summary.concierge {
        println!("  Rarog:    {concierge}");
    }
    if let Some(ref weles) = summary.weles {
        println!("  WELES:    {weles}");
    }
    if let Some(ref compaction) = summary.compaction {
        println!("  Compaction: {compaction}");
    }
    println!();
    let launch_items = post_setup_choices();
    let launch_idx = select_list("What would you like to run now?", &launch_items, false, 0)?
        .expect("post-setup selection is required");
    Ok(post_setup_action_from_index(launch_idx))
}
