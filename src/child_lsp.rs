use anyhow::{anyhow, Result};
use tower_lsp::lsp_types::*;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use std::sync::Arc;
use tracing::debug;

pub struct ChildLspManager {
    _process: Arc<Mutex<Option<Child>>>,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Arc<Mutex<BufReader<ChildStdout>>>,
    next_id: Arc<Mutex<i32>>,
    capabilities: Arc<Mutex<Option<Value>>>,
}

impl ChildLspManager {
    pub async fn spawn(binary: &str, args: Vec<String>) -> Result<Self> {
        let binary = binary.to_string();
        debug!("[ChildLSP] Spawning: {} {:?}", binary, args);
        let mut child = tokio::task::spawn_blocking(move || {
            let mut cmd = Command::new(&binary);
            cmd.stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            for arg in args {
                cmd.arg(arg);
            }
            cmd.spawn()
        })
        .await??;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdout"))?;

        let reader = BufReader::new(stdout);

        Ok(ChildLspManager {
            _process: Arc::new(Mutex::new(Some(child))),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(reader)),
            next_id: Arc::new(Mutex::new(1)),
            capabilities: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn send_request_raw(&self, method: &str, params: Value) -> Result<Value> {
        let id = {
            let mut next_id = self.next_id.lock().await;
            let current_id = *next_id;
            *next_id += 1;
            current_id
        };

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let request_str = request.to_string();
        let message = format!("Content-Length: {}\r\n\r\n{}", request_str.len(), request_str);

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(message.as_bytes())?;
        stdin.flush()?;
        drop(stdin); // Release lock before reading

        self.read_response(id).await
    }

    async fn read_response(&self, expected_id: i32) -> Result<Value> {
        let stdout_arc = Arc::clone(&self.stdout);
        let task = tokio::task::spawn_blocking(move || {
            let mut stdout = stdout_arc.blocking_lock();

            loop {
                let mut headers = std::collections::HashMap::new();
                let mut line = String::new();

                // Read headers
                loop {
                    line.clear();
                    let n = stdout.read_line(&mut line)?;
                    if n == 0 {
                        return Err(anyhow!("Unexpected EOF while reading LSP response"));
                    }

                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        break;
                    }

                    if let Some((key, value)) = line.split_once(':') {
                        headers.insert(key.trim().to_string(), value.trim().to_string());
                    }
                }

                let content_length: usize = headers
                    .get("Content-Length")
                    .ok_or_else(|| anyhow!("Missing Content-Length header"))?
                    .parse()?;

                let mut content = vec![0u8; content_length];
                use std::io::Read;
                stdout.read_exact(&mut content)?;

                let msg: Value = serde_json::from_slice(&content)?;

                // If this is a response with matching ID, return it
                if let Some(id) = msg.get("id") {
                    if id.as_i64() == Some(expected_id as i64) && (msg.get("result").is_some() || msg.get("error").is_some()) {
                        return Ok(msg);
                    }
                }

                // Otherwise, it's a notification or a response we don't want - skip it and continue
                debug!("[ChildLSP] Skipping message (not response to ID {}): method={}",
                    expected_id,
                    msg.get("method").and_then(|m| m.as_str()).unwrap_or("<no method>"));
            }
        });

        match tokio::time::timeout(std::time::Duration::from_secs(5), task).await {
            Ok(Ok(value_result)) => value_result,  // Flatten nested Results
            Ok(Err(join_err)) => Err(anyhow!("Task error: {}", join_err)),
            Err(_) => Err(anyhow!("Timeout waiting for LSP response from child process")),
        }
    }

    pub async fn initialize(&self, root_uri: String, init_options: Option<serde_json::Value>) -> Result<()> {
        let init_opts = init_options.unwrap_or_else(|| json!({}));
        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "synchronization": {
                        "didSave": true
                    }
                }
            },
            "initializationOptions": init_opts
        });

        let response = self.send_request_raw("initialize", params).await?;
        // Extract and store server capabilities from the response
        if let Some(capabilities) = response.get("result").and_then(|r| r.get("capabilities")) {
            let mut caps = self.capabilities.lock().await;
            *caps = Some(capabilities.clone());
        }
        // Send initialized notification (no ID, so no response expected)
        self.send_notification("initialized", json!({})).await?;
        Ok(())
    }

    pub async fn get_capabilities(&self) -> Option<Value> {
        let caps = self.capabilities.lock().await;
        caps.as_ref().cloned()
    }

    pub async fn get_completion_trigger_characters(&self) -> Option<Vec<String>> {
        if let Some(caps) = self.get_capabilities().await {
            // Navigate to completionProvider.triggerCharacters
            if let Some(triggers) = caps
                .get("completionProvider")
                .and_then(|cp| cp.get("triggerCharacters"))
                .and_then(|tc| tc.as_array())
            {
                return Some(
                    triggers
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect(),
                );
            }
        }
        None
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let notification_str = notification.to_string();
        let message = format!("Content-Length: {}\r\n\r\n{}", notification_str.len(), notification_str);

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(message.as_bytes())?;
        stdin.flush()?;
        Ok(())
    }

    pub async fn did_open(&self, uri: String, language_id: String, content: String) -> Result<()> {

        let params = json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": 1,
                "text": content
            }
        });

        self.send_notification("textDocument/didOpen", params).await?;
        Ok(())
    }

    pub async fn did_change(&self, uri: String, version: i32, content: String) -> Result<()> {
        debug!("[ChildLSP] Changing document: uri={}, version={}, content_len={}",
            uri, version, content.len());

        let params = json!({
            "textDocument": {
                "uri": uri,
                "version": version
            },
            "contentChanges": [
                {
                    "text": content
                }
            ]
        });

        self.send_notification("textDocument/didChange", params).await?;
        Ok(())
    }

    pub async fn goto_definition(
        &self,
        uri: String,
        line: u32,
        character: u32,
    ) -> Result<Option<Location>> {
        debug!("[ChildLSP] Requesting definition: uri={}, position={}:{}",
            uri, line, character);

        let params = json!({
            "textDocument": {
                "uri": uri,
            },
            "position": {
                "line": line,
                "character": character,
            }
        });

        let response = self.send_request_raw("textDocument/definition", params).await?;

        if response.get("result").is_none() {
            return Ok(None);
        }

        let result = &response["result"];

        if result.is_null() {
            return Ok(None);
        }

        if result.is_array() {
            if let Some(first) = result.as_array().and_then(|arr| arr.first()) {
                if let Ok(location) = serde_json::from_value::<Location>(first.clone()) {
                    return Ok(Some(location));
                }
            }
            return Ok(None);
        }

        if let Ok(location) = serde_json::from_value::<Location>(result.clone()) {
            Ok(Some(location))
        } else {
            Ok(None)
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        // Send exit notification only - don't wait for responses
        let _ = self.send_notification("exit", json!({})).await;
        Ok(())
    }
}

impl Drop for ChildLspManager {
    fn drop(&mut self) {
        // Can't use blocking_lock() in Drop when inside async runtime
        // Let the OS clean up the process - it will be killed when the parent exits
        debug!("[ChildLSP] Dropped, process will be cleaned up by OS");
    }
}
