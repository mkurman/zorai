use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use anyhow::{anyhow, bail, Context, Result};
use futures::{stream::BoxStream, StreamExt};
use reqwest_mcp::header::{HeaderName, HeaderValue};
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResult, ClientConfig, ClientJsonRpcMessage, Implementation,
        ProtocolVersion,
    },
    transport::{
        streamable_http_client::{
            StreamableHttpClient, StreamableHttpClientTransportConfig, StreamableHttpError,
            StreamableHttpPostResponse, StreamableHttpResponseLimits,
        },
        StreamableHttpClientTransport,
    },
    ServiceExt,
};
use serde_json::{json, Map, Value};
use sse_stream::{Error as SseError, Sse};
use tokio_util::sync::CancellationToken;
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
    let cancelled = CancellationToken::new();
    let _cancel_on_drop = cancelled.clone().drop_guard();
    let transport = StreamableHttpClientTransport::with_client(
        SearchHttpClient {
            http: http.clone(),
            cancelled,
        },
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

// RMCP's startup POSTs can block before its transport begins polling cancellation.
#[derive(Clone)]
struct SearchHttpClient {
    http: reqwest_mcp::Client,
    cancelled: CancellationToken,
}

type SearchHttpError = StreamableHttpError<reqwest_mcp::Error>;
type Headers = HashMap<HeaderName, HeaderValue>;
type EventStream = BoxStream<'static, Result<Sse, SseError>>;

impl StreamableHttpClient for SearchHttpClient {
    type Error = reqwest_mcp::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: Headers,
    ) -> Result<StreamableHttpPostResponse, SearchHttpError> {
        self.post_message_with_response_limits(
            uri,
            message,
            session_id,
            auth_header,
            headers,
            StreamableHttpResponseLimits::default(),
        )
        .await
    }

    async fn post_message_with_response_limits(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: Headers,
        limits: StreamableHttpResponseLimits,
    ) -> Result<StreamableHttpPostResponse, SearchHttpError> {
        let response = tokio::select! {
            biased;
            _ = self.cancelled.cancelled() => return Err(StreamableHttpError::TransportChannelClosed),
            response = self.http.post_message_with_response_limits(
                uri, message, session_id, auth_header, headers, limits,
            ) => response?,
        };
        match response {
            StreamableHttpPostResponse::Sse(stream, session_id) => {
                let stream = stream.take_until(self.cancelled.clone().cancelled_owned());
                Ok(StreamableHttpPostResponse::Sse(
                    Box::pin(stream),
                    session_id,
                ))
            }
            response => Ok(response),
        }
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        headers: Headers,
    ) -> Result<EventStream, SearchHttpError> {
        self.http
            .get_stream(uri, session_id, last_event_id, auth_header, headers)
            .await
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        headers: Headers,
        max_sse_event_size: usize,
    ) -> Result<EventStream, SearchHttpError> {
        self.http
            .get_stream_with_max_sse_event_size(
                uri,
                session_id,
                last_event_id,
                auth_header,
                headers,
                max_sse_event_size,
            )
            .await
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        headers: Headers,
    ) -> Result<(), SearchHttpError> {
        self.http
            .delete_session(uri, session_id, auth_header, headers)
            .await
    }
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
