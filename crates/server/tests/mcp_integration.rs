//! MCP 协议级集成测试：rmcp 官方客户端 × 真实 HTTP 全链路。
//!
//! 与 curl 手工验证互补，作为协议层回归：initialize 握手、tools/list、
//! tools/call（内容搜索 / count_only / stats）、认证拒绝、[mcp] 开关。

use std::io::Write as _;
use std::path::PathBuf;

use serde_json::{Value, json};
use tailr_core::config::McpConfig;
use tailr_core::limits::LimitsConfig;
use tailr_protocol::{LogLevelConfig, LogTimezone};
use tailr_server::app;

const TOKEN: &str = "test-token";

/// 在随机端口起真实 TCP 服务（真实 axum::serve，非 oneshot），
/// 返回 /mcp 的 URL。后台任务随测试运行时结束。
async fn spawn_app(mcp: McpConfig, log_dir: PathBuf) -> String {
    spawn_app_with_token(mcp, log_dir, TOKEN).await
}

async fn spawn_app_with_token(mcp: McpConfig, log_dir: PathBuf, global_token: &str) -> String {
    let router = app(
        vec![log_dir],
        PathBuf::from("/tmp/nonexistent.toml"),
        LogLevelConfig {
            preset: "general".to_string(),
            levels: vec![],
        },
        LogTimezone::default(),
        global_token.to_string(),
        LimitsConfig::default(),
        mcp,
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    format!("http://{addr}/mcp")
}

/// 带 Authorization 默认头的 reqwest 客户端（rmcp 0.5.0 的 worker 不传
/// auth_header，用 default_headers 让每个请求都带上 token）。
fn authed_http(token: Option<&str>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder();
    if let Some(t) = token {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Authorization",
            format!("Bearer {t}").parse().unwrap(),
        );
        builder = builder.default_headers(headers);
    }
    builder.build().unwrap()
}

async fn connect(
    url: &str,
    token: Option<&str>,
) -> Result<rmcp::service::RunningService<rmcp::RoleClient, ()>, rmcp::service::ClientInitializeError> {
    let transport = rmcp::transport::StreamableHttpClientTransport::with_client(
        authed_http(token),
        rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
            url,
        ),
    );
    rmcp::service::serve_client((), transport).await
}

fn fixture_dir() -> PathBuf {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.keep();
    let mut f = std::fs::File::create(path.join("app.log")).unwrap();
    write!(
        f,
        "2026-08-14 10:00:01 INFO started\n\
         2026-08-14 10:00:02 ERROR db refused\n\
         2026-08-14 10:00:03 INFO retry ok\n\
         2026-08-14 10:00:04 ERROR db refused again\n"
    )
    .unwrap();
    path
}

/// 从 CallToolResponse 提取文本内容并解析为 JSON。
fn tool_json(resp: &rmcp::model::CallToolResult) -> Value {
    let text = resp
        .content
        .iter()
        .flatten()
        .find_map(|c| match &c.raw {
            rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .expect("text content in tool result");
    serde_json::from_str(&text).expect("tool result is valid JSON")
}

#[tokio::test]
async fn full_protocol_roundtrip() {
    let dir = fixture_dir();
    let url = spawn_app(McpConfig::default(), dir).await;
    let client = connect(&url, Some(TOKEN)).await.expect("initialize handshake");

    // tools/list：五个 tool 全部注册
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in [
        "list_log_files",
        "tail_log",
        "search_logs",
        "read_log_range",
        "get_log_stats",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}: {names:?}");
    }

    // list_log_files：host 字段 + 文件存在
    let resp = client
        .peer()
        .call_tool(rmcp::model::CallToolRequestParam {
            name: "list_log_files".into(),
            arguments: None,
        })
        .await
        .unwrap();
    let v = tool_json(&resp);
    assert!(v["host"].is_string());
    assert!(v["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "app.log"));

    // search_logs：AND 匹配 + 上下文窗口 + host
    let log_path = v["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "app.log")
        .map(|f| f["path"].as_str().unwrap().to_string())
        .unwrap();
    let args = json!({ "path": log_path, "keywords": ["error", "db"] })
        .as_object()
        .cloned()
        .unwrap();
    let resp = client
        .peer()
        .call_tool(rmcp::model::CallToolRequestParam {
            name: "search_logs".into(),
            arguments: Some(args),
        })
        .await
        .unwrap();
    let v = tool_json(&resp);
    assert_eq!(v["matchedLines"], 2);
    assert_eq!(v["more"], false);
    let lines = v["windows"][0]["lines"].as_array().unwrap();
    let matches: Vec<&Value> = lines.iter().filter(|l| l["isMatch"] == true).collect();
    assert_eq!(matches.len(), 2);

    // count_only：只计数，无窗口输出，不受 max_matches 影响
    let args = json!({ "path": log_path, "keywords": ["error"], "count_only": true, "max_matches": 1 })
        .as_object()
        .cloned()
        .unwrap();
    let resp = client
        .peer()
        .call_tool(rmcp::model::CallToolRequestParam {
            name: "search_logs".into(),
            arguments: Some(args),
        })
        .await
        .unwrap();
    let v = tool_json(&resp);
    assert_eq!(v["matchedLines"], 2);
    assert_eq!(v["windows"].as_array().unwrap().len(), 0);

    // get_log_stats：行数与级别
    let args = json!({ "path": log_path }).as_object().cloned().unwrap();
    let resp = client
        .peer()
        .call_tool(rmcp::model::CallToolRequestParam {
            name: "get_log_stats".into(),
            arguments: Some(args),
        })
        .await
        .unwrap();
    let v = tool_json(&resp);
    assert_eq!(v["totalLines"], 4);
    assert_eq!(v["incomplete"], false);
}

#[tokio::test]
async fn unauthenticated_client_is_rejected_until_token_given() {
    let dir = fixture_dir();
    let url = spawn_app(McpConfig::default(), dir).await;

    // REST：全局 token 锁着（无凭证 401）
    let base = url.trim_end_matches("/mcp");
    let status = reqwest::Client::new()
        .get(format!("{base}/api/health"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 401);

    // MCP：token 未设置 → 继承全局 token（无凭证握手失败，全局 token 可用）
    let result = connect(&url, None).await;
    assert!(result.is_err(), "unauthenticated handshake must fail");
    let client = connect(&url, Some(TOKEN)).await.expect("inherited global token");
    let _ = client.list_all_tools().await.unwrap();
}

#[tokio::test]
async fn mcp_disabled_returns_404() {
    let dir = fixture_dir();
    let url = spawn_app(
        McpConfig {
            enabled: false,
            host_name: None,
        },
        dir,
    )
    .await;

    let status = authed_http(Some(TOKEN))
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 404);
}



