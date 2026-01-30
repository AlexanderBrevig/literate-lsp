use std::io::{BufRead, BufReader, Write, Read};
use std::process::{Command, Stdio};
use serde_json::json;

#[test]
fn test_goto_definition() {
    // Check if forth-lsp is available
    let forth_lsp_available = Command::new("which")
        .arg("forth-lsp")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !forth_lsp_available {
        eprintln!("forth-lsp not found, skipping test");
        return;
    }

    // Spawn the literate-lsp server
    let binary_path = std::env::var("CARGO_BIN_EXE_literate_lsp")
        .or_else(|_| {
            // Fallback: look in target/debug or target/release
            let debug_path = "target/debug/literate-lsp";
            let release_path = "target/release/literate-lsp";
            if std::path::Path::new(debug_path).exists() {
                Ok(debug_path.to_string())
            } else if std::path::Path::new(release_path).exists() {
                Ok(release_path.to_string())
            } else {
                Err(std::env::VarError::NotPresent)
            }
        })
        .expect("Could not find literate-lsp binary");
    let mut server = Command::new(&binary_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn literate-lsp");

    let stdin = server.stdin.take().expect("Failed to get stdin");
    let stdout = server.stdout.take().expect("Failed to get stdout");
    let stderr = server.stderr.take().expect("Failed to get stderr");

    // Spawn a thread to read and print server stderr
    let _stderr_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                eprintln!("[SERVER] {}", l);
            }
        }
    });

    let mut reader = BufReader::new(stdout);
    let mut writer = std::io::BufWriter::new(stdin);

    // Helper functions for LSP communication
    // Helper function to send a request (with ID)
    let send_request = |writer: &mut std::io::BufWriter<_>, method: &str, params: serde_json::Value, id: i32| -> std::io::Result<()> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let request_str = request.to_string();
        let message = format!("Content-Length: {}\r\n\r\n{}", request_str.len(), request_str);
        writer.write_all(message.as_bytes())?;
        writer.flush()?;
        Ok(())
    };

    // Helper function to send a notification (without ID)
    let send_notification = |writer: &mut std::io::BufWriter<_>, method: &str, params: serde_json::Value| -> std::io::Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let notification_str = notification.to_string();
        let message = format!("Content-Length: {}\r\n\r\n{}", notification_str.len(), notification_str);
        writer.write_all(message.as_bytes())?;
        writer.flush()?;
        Ok(())
    };

    // Helper function to read a response (skips non-response messages from server)
    let read_response = |reader: &mut BufReader<_>| -> serde_json::Value {
        loop {
            let mut headers = std::collections::HashMap::new();
            let mut line = String::new();

            loop {
                line.clear();
                reader.read_line(&mut line).expect("Failed to read line");
                if line.trim().is_empty() {
                    break;
                }
                if let Some((key, value)) = line.split_once(':') {
                    headers.insert(key.trim().to_string(), value.trim().to_string());
                }
            }

            let content_length: usize = headers
                .get("Content-Length")
                .expect("Missing Content-Length header")
                .parse()
                .expect("Invalid Content-Length");

            let mut content = vec![0u8; content_length];
            reader.read_exact(&mut content).expect("Failed to read response body");

            let msg: serde_json::Value = serde_json::from_slice(&content).expect("Failed to parse JSON response");

            // If this is a response (has "result" or "error" field), return it
            // Otherwise it's a request/notification from the server, skip it
            if msg.get("result").is_some() || msg.get("error").is_some() {
                return msg;
            }
        }
    };

    // Send initialize request
    let root_uri = "file:///home/ab/github.com/literate-lsp";
    send_request(&mut writer, "initialize", json!({
        "processId": std::process::id(),
        "rootUri": root_uri,
        "capabilities": {
            "textDocument": {
                "synchronization": {
                    "didSave": true
                }
            }
        },
        "initializationOptions": {}
    }), 1).expect("Failed to send initialize");

    let init_response = read_response(&mut reader);
    println!("Initialize response: {}", serde_json::to_string_pretty(&init_response).unwrap());
    assert!(init_response.get("result").is_some() || init_response.get("error").is_none(), "Initialize should succeed");

    // Send initialized notification (this is a notification, not a request)
    send_notification(&mut writer, "initialized", json!({})).expect("Failed to send initialized");

    // Read the example.md file
    let example_content = std::fs::read_to_string("example.md")
        .expect("Failed to read example.md");

    // Send didOpen notification
    send_notification(&mut writer, "textDocument/didOpen", json!({
        "textDocument": {
            "uri": "file:///home/ab/github.com/literate-lsp/example.md",
            "languageId": "markdown",
            "version": 1,
            "text": example_content,
        }
    })).expect("Failed to send didOpen");

    // Give the server time to process the notification
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Send definition request at line 14 (0-indexed), column 2
    // This is the `square` in `5 square .`
    send_request(&mut writer, "textDocument/definition", json!({
        "textDocument": {
            "uri": "file:///home/ab/github.com/literate-lsp/example.md",
        },
        "position": {
            "line": 14,
            "character": 2,
        }
    }), 2).expect("Failed to send definition request");

    let response = read_response(&mut reader);
    println!("Definition response: {}", serde_json::to_string_pretty(&response).unwrap());

    // Verify the response
    assert!(response.get("result").is_some(), "Response should have a result field");

    let result = &response["result"];

    // Result can be null if definition not found, or a Location/Location[] array
    if result.is_null() {
        println!("Definition not found (LSP returned null)");
        panic!("Definition should be found!");
    } else if let Some(location) = result.as_object() {
        // Single Location object
        let uri = location.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        println!("Result URI: {}", uri);
        assert!(uri.contains("example.md"), "Response URI should be example.md");

        let range = location.get("range").expect("Response should have a range");
        let line = range
            .get("start")
            .and_then(|s| s.get("line"))
            .and_then(|l| l.as_u64())
            .expect("Response should have a line number") as u32;

        // The definition should be at line 8 (0-indexed) where `square` is defined
        println!("Result line: {}", line);
        assert_eq!(line, 8, "Definition should be at line 8 (0-indexed)");
    } else if let Some(locations) = result.as_array() {
        // Array of Locations
        assert!(!locations.is_empty(), "Locations array should not be empty");
        let location = &locations[0];
        let uri = location.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        println!("Result URI: {}", uri);
        assert!(uri.contains("example.md"), "Response URI should be example.md");

        let range = location.get("range").expect("Response should have a range");
        let line = range
            .get("start")
            .and_then(|s| s.get("line"))
            .and_then(|l| l.as_u64())
            .expect("Response should have a line number") as u32;

        // The definition should be at line 8 (0-indexed) where `square` is defined
        println!("Result line: {}", line);
        assert_eq!(line, 8, "Definition should be at line 8 (0-indexed)");
    } else {
        panic!("Expected a location or null in the response, got: {}", result);
    }

    // Clean up
    drop(writer);
    let _ = server.kill();
}
