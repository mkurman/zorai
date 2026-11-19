use std::sync::LazyLock;

use anyhow::{anyhow, bail, Context, Result};
use rmcp::{
    model::{CallToolRequestParams, CallToolResult, ClientConfig, Implementation, ProtocolVersion},
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    },
    ServiceExt,
};
use serde_json::{json, Map, Value};
use url::Url;

use super::{format_result_with_metadata, safe_snippet_preview};

const ENDPOINT: &str = "https://mcp.parallel.ai/mcp";
const USER_AGENT: &str = concat!("zorai-daemon/", env!("CARGO_PKG_VERSION"));
static HTTP_CLIENT: LazyLock<Result<reqwest_mcp::Client, reqwest_mcp::Error>> =
    LazyLock::new(|| {
        reqwest_mcp::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(reqwest_mcp::redirect::Policy::none())
            .build()
    });

pub(super) async fn search(query: &str, max_results: u64) -> Result<String> {
    search_at(ENDPOINT, query, max_results).await
}

async fn search_at(endpoint: &str, query: &str, max_results: u64) -> Result<String> {
    let http = HTTP_CLIENT
        .as_ref()
        .map_err(|error| anyhow!("Could not build Parallel MCP HTTP client: {error}"))?;
    let transport = StreamableHttpClientTransport::with_client(
        http.clone(),
        StreamableHttpClientTransportConfig::with_uri(endpoint),
    );
    let client = ClientConfig::new(
        Default::default(),
        Implementation::new("zorai-daemon", env!("CARGO_PKG_VERSION")),
    )
    .with_protocol_version(ProtocolVersion::V_2025_03_26)
    .serve(transport)
    .await
    .context("Could not connect to Parallel Search MCP")?;
    let args = Map::from_iter([
        ("objective".to_string(), json!(query)),
        ("search_queries".to_string(), json!([query])),
    ]);
    let result = client
        .call_tool(CallToolRequestParams::new("web_search").with_arguments(args))
        .await
        .context("Parallel Search MCP request failed")?;
    format_response(query, result, max_results)
}

fn format_response(query: &str, response: CallToolResult, max_results: u64) -> Result<String> {
    if response.is_error == Some(true) {
        bail!("Parallel Search MCP returned a search error");
    }
    let data = match response.structured_content {
        Some(data) => data,
        None => response
            .content
            .iter()
            .filter_map(|block| block.as_text())
            .find_map(|block| serde_json::from_str::<Value>(&block.text).ok())
            .context("Parallel Search MCP returned no JSON results")?,
    };
    let results = data["results"]
        .as_array()
        .context("Parallel Search MCP returned an invalid results list")?;
    let lines = results
        .iter()
        .filter_map(|result| {
            let raw_url = result["url"].as_str()?;
            let url = Url::parse(raw_url).ok()?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || raw_url.chars().any(|c| c.is_control() || c.is_whitespace())
            {
                return None;
            }
            let title = result["title"].as_str().unwrap_or("(no title)");
            let title = safe_snippet_preview(&one_line(title), 300);
            let excerpt = result["excerpts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find(|text| !text.trim().is_empty())
                .unwrap_or_default();
            let excerpt = safe_snippet_preview(&one_line(excerpt), 300);
            Some(format_result_with_metadata(
                &title,
                raw_url,
                &excerpt,
                result["publish_date"].as_str(),
            ))
        })
        .take(usize::try_from(max_results).unwrap_or(usize::MAX))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        Ok(format!("No web results found for: {query}"))
    } else {
        Ok(format!(
            "Web results for \"{query}\" (untrusted web content):\n\n{}",
            lines.join("\n\n")
        ))
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[path = "tests/parallel_search.rs"]
mod tests;
