use crate::child_lsp::ChildLspManager;
use crate::child_lsp_init::{ChildLspInitializer, ChildLspInitParams};
use crate::config::Config;
use crate::position::PositionMapper;
use crate::request_mapper;
use crate::virtual_doc::{build_virtual_document, find_code_block_at_line};
use crate::utils::{uri_helpers, constants};
use regex::Regex;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result as JsonrpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tracing::{debug, info, warn};

pub struct LiterateLsp {
    client: Client,
    config: Config,
    document: Arc<RwLock<Option<String>>>,
    document_uri: Arc<RwLock<Option<Url>>>,
    document_version: Arc<RwLock<i32>>,
    child_lsps: Arc<RwLock<std::collections::HashMap<String, ChildLspManager>>>,
    child_versions: Arc<RwLock<std::collections::HashMap<String, i32>>>,
    completion_triggers: Arc<RwLock<std::collections::HashMap<String, Vec<String>>>>,
}

impl LiterateLsp {
    pub fn new(client: Client, config: Config) -> Self {
        LiterateLsp {
            client,
            config,
            document: Arc::new(RwLock::new(None)),
            document_uri: Arc::new(RwLock::new(None)),
            document_version: Arc::new(RwLock::new(0)),
            child_lsps: Arc::new(RwLock::new(std::collections::HashMap::new())),
            child_versions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            completion_triggers: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Get the language of the document based on file extension
    fn get_document_language(uri: &Url) -> Option<String> {
        let path = uri.path();
        let ext = path.rsplit('.').next()?.to_lowercase();
        match ext.as_str() {
            // Markdown variants
            "md" | "markdown" | "mdown" | "mkdn" | "mdx" | "mmd" => Some("markdown".to_string()),
            // Typst
            "typ" => Some("typst".to_string()),
            // Other languages
            "go" => Some("go".to_string()),
            "forth" | "fth" => Some("forth".to_string()),
            _ => None,
        }
    }

    /// Check if a code block language should be skipped for this document
    /// Skips self-referential cases like markdown blocks in markdown files
    fn should_skip_language(doc_lang: Option<&str>, block_lang: &str) -> bool {
        match doc_lang {
            Some("markdown") if block_lang == "markdown" => true,
            Some("typst") if block_lang == "typst" => true,
            Some("go") if block_lang == "go" => true,
            Some("forth") if block_lang == "forth" => true,
            _ => false,
        }
    }

    /// Cache completion trigger characters from a child LSP
    async fn cache_completion_triggers(&self, lang: &str, child_lsp: &ChildLspManager) {
        if let Some(triggers) = child_lsp.get_completion_trigger_characters().await {
            let mut cache = self.completion_triggers.write().await;
            cache.insert(lang.to_string(), triggers);
        }
    }

    /// Get all known completion trigger characters
    async fn get_all_completion_triggers(&self) -> Vec<String> {
        let cache = self.completion_triggers.read().await;
        let mut triggers = std::collections::HashSet::new();
        for (_, chars) in cache.iter() {
            for c in chars {
                triggers.insert(c.clone());
            }
        }
        triggers.into_iter().collect()
    }

    /// Update all child LSPs with changed virtual documents
    async fn update_child_lsps(&self, doc_content: &str, _new_version: i32) {
        let mut child_lsps = self.child_lsps.write().await;
        let mut child_versions = self.child_versions.write().await;
        let uri = self.document_uri.read().await;
        let uri_str = match uri.as_ref() {
            Some(u) => u.to_string(),
            None => return,
        };
        drop(uri);

        let root_uri_base = uri_helpers::extract_root_uri_base(&uri_str);

        for (lang, child_lsp) in child_lsps.iter_mut() {
            let vdoc = build_virtual_document(doc_content, lang);
            let virtual_uri = uri_helpers::construct_virtual_uri(root_uri_base, lang);
            let current_version = child_versions.get(lang).copied().unwrap_or(0);

            if let Err(e) = child_lsp
                .did_change(virtual_uri, current_version + 1, vdoc.content)
                .await
            {
                warn!("Failed to update child LSP for '{}': {}", lang, e);
            } else {
                child_versions.insert(lang.clone(), current_version + 1);
            }
        }
    }

    /// Map virtual document file references in text back to markdown coordinates
    fn map_virtual_doc_references(
        text: &str,
        lang: &str,
        mapper: &PositionMapper,
        markdown_filename: &str,
    ) -> String {
        let pattern = format!(r"virtual\.{}:(\d+):(\d+)", regex::escape(lang));
        if let Ok(re) = Regex::new(&pattern) {
            re.replace_all(text, |caps: &regex::Captures| {
                if let (Ok(virtual_line), Ok(character)) =
                    (caps[1].parse::<u32>(), caps[2].parse::<u32>())
                {
                    match mapper.virtual_to_markdown(virtual_line, character) {
                        Some((markdown_line, char_pos)) => {
                            format!("{}:{}:{}", markdown_filename, markdown_line + 1, char_pos)
                        }
                        None => caps[0].to_string(),
                    }
                } else {
                    caps[0].to_string()
                }
            })
            .to_string()
        } else {
            text.to_string()
        }
    }

    /// Recursively map virtual document references in all string values
    fn map_virtual_refs_in_value(
        value: &mut serde_json::Value,
        lang: &str,
        mapper: &PositionMapper,
        markdown_filename: &str,
    ) {
        match value {
            serde_json::Value::String(s) => {
                *s = Self::map_virtual_doc_references(s, lang, mapper, markdown_filename);
            }
            serde_json::Value::Object(map) => {
                for v in map.values_mut() {
                    Self::map_virtual_refs_in_value(v, lang, mapper, markdown_filename);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr.iter_mut() {
                    Self::map_virtual_refs_in_value(v, lang, mapper, markdown_filename);
                }
            }
            _ => {}
        }
    }

    /// Generic handler for position-based LSP requests
    async fn handle_position_request(
        &self,
        method: &str,
        position: Position,
        uri: Url,
    ) -> JsonrpcResult<serde_json::Value> {
        eprintln!(
            "[Hover] Request: {} at line:{} char:{}",
            method, position.line, position.character
        );

        let doc = self.document.read().await;
        let doc_content = match doc.as_ref() {
            Some(c) => c,
            None => {
                debug!("[Hover] No document loaded");
                return Ok(json!(null));
            }
        };

        let markdown_line = position.line as usize;

        let (lang, _block_start, _block_end) =
            match find_code_block_at_line(doc_content, markdown_line) {
                Some(result) => result,
                None => {
                    debug!("[Hover] No code block found at line {}", markdown_line);
                    return Ok(json!(null));
                }
            };

        debug!("[LiterateLsp] Detected code block language: '{}'", lang);

        // Skip self-referential cases (e.g., markdown blocks in markdown files)
        let doc_lang = Self::get_document_language(&uri);
        if Self::should_skip_language(doc_lang.as_deref(), &lang) {
            eprintln!(
                "[LiterateLsp] Skipping language '{}' (self-referential)",
                lang
            );
            let message = format!(
                "Cannot provide IDE features for **{}** code blocks inside **{}** documents.\n\n\
                 **Why?** This would create a recursive loop (literate-lsp acting on itself).\n\n\
                 **Solution:** Move the {} code outside the {} fence, or use a different documentation format.",
                lang, lang, lang, lang
            );
            debug!("[Hover] Returning self-referential message: {}", message);
            let hover_response = json!({
                "result": {
                    "contents": message
                }
            });
            debug!("[Hover] Serialized response: {}", hover_response);
            return Ok(hover_response);
        }

        let vdoc = build_virtual_document(doc_content, &lang);
        let mapper = PositionMapper::new(vdoc.blocks.clone());

        // If no code blocks found for this language, provide helpful feedback
        eprintln!(
            "[Hover] Virtual doc empty: {}, blocks: {}",
            vdoc.content.is_empty(),
            vdoc.blocks.len()
        );
        if vdoc.content.is_empty() {
            debug!("[Hover] Building helpful message for missing language");
            // Find what languages were actually in the document
            let mut found_langs = std::collections::HashSet::new();
            let lines: Vec<&str> = doc_content.lines().collect();
            for line in lines {
                if let Some(pos) = line.find("```") {
                    let lang_start = pos + 3;
                    if lang_start <= line.len() {
                        let lang_part = &line[lang_start..];
                        let found_lang = lang_part
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .to_string();
                        if !found_lang.is_empty() {
                            found_langs.insert(found_lang);
                        }
                    }
                }
            }

            let message = if found_langs.is_empty() {
                "No code blocks found in this document".to_string()
            } else {
                let mut langs: Vec<_> = found_langs.iter().map(|s| s.as_str()).collect();
                langs.sort();
                format!(
                    "No '{}' code blocks found.\n\nFound: {}\n\n**Note:** Code blocks nested inside other fences (like markdown examples) are not supported. Move the {} code outside the markdown fence.",
                    lang,
                    langs.join(", "),
                    lang
                )
            };

            // Return the message as hover content
            debug!("[Hover] Returning helpful message: {}", message);
            let hover_response = json!({
                "result": {
                    "contents": message
                }
            });
            debug!("[Hover] Hover response: {}", hover_response);
            return Ok(hover_response);
        }

        // Write virtual document to /tmp for inspection
        let vdoc_path = uri_helpers::construct_temp_vdoc_path(&lang);
        if let Err(e) = std::fs::write(&vdoc_path, &vdoc.content) {
            warn!(
                "Warning: Failed to write virtual doc to {}: {}",
                vdoc_path, e
            );
        }

        // Construct the virtual document URI before building params
        let root_uri_str = uri.to_string();
        let root_uri_base = uri_helpers::extract_root_uri_base(&root_uri_str);
        let virtual_uri = uri_helpers::construct_virtual_uri(root_uri_base, &lang);

        // Build the request parameters with virtual document URI
        let mut params = json!({
            "textDocument": { "uri": virtual_uri.clone() },
            "position": { "line": position.line, "character": position.character }
        });

        // Rewrite request positions to virtual document coordinates
        request_mapper::rewrite_positions(&mut params, &mapper, true);

        let (binary_name, args) = match self.config.get_command_and_args(&lang) {
            Some((cmd, args)) => (cmd, args),
            None => {
                warn!("[LiterateLsp] No LSP found for language '{}'. Check: literate-lsp --health {}", lang, lang);

                // Check if language exists in config
                let lang_exists = self.config.language.iter().find(|l| l.name == lang);

                let available_lsps = "Find available LSPs at: https://langserver.org";
                let add_support = "To add IDE support, add this to `$root/.languages.toml`:";
                let message = if let Some(lang) = lang_exists {
                    // Language exists but no LSP configured
                    let servers = lang.get_server_names();
                    let servers_array = servers
                        .iter()
                        .map(|s| format!("\"{}\"", s))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "**Language '{}' is configured but has no LSP server.**\n\n\
                         {}\n\n\
                         ```toml\n\
                         [[language]]\n\
                         name = \"{}\"\n\
                         language-servers = [{}]\n\
                         ```\n\n\
                         {}",
                        lang.name, add_support, lang.name, servers_array, available_lsps
                    )
                } else {
                    // Language not configured at all
                    format!(
                        "**Language '{}' is not configured.**\n\n\
                         {}\n\n\
                         ```toml\n\
                         [[language]]\n\
                         name = \"{}\"\n\
                         language-servers = [\"lsp-name\"]\n\
                         ```\n\n\
                         Then find and configure an LSP for {} in the `[language-server]` section:\n\n\
                         ```toml\n\
                         [language-server.lsp-name]\n\
                         command = \"lsp-binary\"\n\
                         ```\n\n\
                         {}",
                        lang, add_support, lang, lang, available_lsps
                    )
                };

                let hover_response = json!({
                    "result": {
                        "contents": message
                    }
                });
                eprintln!(
                    "[Hover] Returning unsupported language message for '{}'",
                    lang
                );
                return Ok(hover_response);
            }
        };

        let mut child_lsps = self.child_lsps.write().await;

        if !child_lsps.contains_key(&lang) {
            let init_params = ChildLspInitParams {
                lang: lang.clone(),
                binary_name,
                args,
                root_uri_base: root_uri_base.to_string(),
                virtual_uri: virtual_uri.clone(),
                virtual_doc_content: vdoc.content.clone(),
                init_options: self.config.get_init_options(&lang),
            };

            match ChildLspInitializer::initialize_child_lsp(init_params).await {
                Ok(result) => {
                    // Cache completion triggers after successful initialization
                    self.cache_completion_triggers(&result.lang, &result.lsp).await;
                    child_lsps.insert(result.lang, result.lsp);
                }
                Err(error_msg) => {
                    self.client
                        .log_message(MessageType::ERROR, error_msg)
                        .await;
                    return Ok(json!(null));
                }
            }
        }

        let child_lsp = match child_lsps.get(&lang) {
            Some(lsp) => lsp,
            None => return Ok(json!(null)),
        };

        // Send request to child LSP
        let mut response = match child_lsp.send_request_raw(method, params.clone()).await {
            Ok(resp) => resp,
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Child LSP request failed: {}", e),
                    )
                    .await;
                return Ok(json!(null));
            }
        };

        // Rewrite response positions back to markdown coordinates
        request_mapper::rewrite_positions(&mut response, &mapper, false);

        // Get markdown filename for reference mapping
        let markdown_filename = uri
            .path_segments()
            .and_then(|mut segs| segs.next_back())
            .unwrap_or("document");

        // Map virtual document file references in text content back to markdown
        if let Some(result) = response.get_mut("result") {
            Self::map_virtual_refs_in_value(result, &lang, &mapper, markdown_filename);
        }

        // Replace virtual document URI with original markdown URI in Location responses
        if let Some(result) = response.get_mut("result") {
            match result {
                serde_json::Value::Object(loc) => {
                    // Single location - only update if it has a "uri" field
                    if loc.contains_key("uri") {
                        loc["uri"] = json!(uri.to_string());
                    }
                }
                serde_json::Value::Array(locs) => {
                    // Array of locations
                    for loc in locs.iter_mut() {
                        if let serde_json::Value::Object(loc_obj) = loc {
                            if loc_obj.contains_key("uri") {
                                loc_obj["uri"] = json!(uri.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(response)
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for LiterateLsp {
    async fn initialize(&self, _params: InitializeParams) -> JsonrpcResult<InitializeResult> {
        // Declare capabilities for all position-based LSP methods
        // These are supported as long as the underlying language has an LSP available

        // Get trigger characters that we've learned from child LSPs, or use defaults
        let triggers = self.get_all_completion_triggers().await;
        let trigger_chars = if triggers.is_empty() {
            // Default common trigger characters across languages
            Some(
                constants::DEFAULT_COMPLETION_TRIGGERS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            )
        } else {
            Some(triggers)
        };

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Text sync is required - we use FULL sync to update virtual documents
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Position-based methods - forwarded to child LSPs for supported languages
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: None,
                    trigger_characters: trigger_chars,
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                    completion_item: None,
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "LiterateMD LSP initialized")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let content = params.text_document.text.clone();

        let mut doc = self.document.write().await;
        *doc = Some(content.clone());

        let mut uri_lock = self.document_uri.write().await;
        *uri_lock = Some(uri);

        let mut version = self.document_version.write().await;
        *version = params.text_document.version;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Handle full document changes - for FULL sync, there's typically just one change
        if let Ok(change_json) = serde_json::to_value(&params.content_changes) {
            if let Some(changes_array) = change_json.as_array() {
                if let Some(change) = changes_array.first() {
                    if let Some(text) = change.get("text").and_then(|t| t.as_str()) {
                        let mut doc = self.document.write().await;
                        *doc = Some(text.to_string());
                        let mut version = self.document_version.write().await;
                        *version = params.text_document.version;
                        drop(version);

                        // Update all child LSPs with new virtual documents
                        if let Some(content) = doc.as_ref() {
                            self.update_child_lsps(content, params.text_document.version)
                                .await;
                        }
                    }
                }
            }
        }
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> JsonrpcResult<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        let response = self
            .handle_position_request("textDocument/definition", position, uri)
            .await?;
        // The response has {"result": ...}, we need to extract the result field
        if let Some(result) = response.get("result") {
            Ok(serde_json::from_value(result.clone()).ok())
        } else {
            Ok(None)
        }
    }

    async fn hover(&self, params: HoverParams) -> JsonrpcResult<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        let response = self
            .handle_position_request("textDocument/hover", position, uri)
            .await?;
        if let Some(result) = response.get("result") {
            Ok(serde_json::from_value(result.clone()).ok())
        } else {
            Ok(None)
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> JsonrpcResult<Option<DocumentSymbolResponse>> {
        let doc = self.document.read().await;
        let doc_content = match doc.as_ref() {
            Some(c) => c.clone(),
            None => return Ok(None),
        };

        // For document symbols, we return symbols from all code blocks
        // Request position 0,0 to find the first language
        if let Some((lang, _, _)) = find_code_block_at_line(&doc_content, 0) {
            let vdoc = build_virtual_document(&doc_content, &lang);
            let mapper = PositionMapper::new(vdoc.blocks.clone());

            let mut req_params =
                json!({"textDocument": {"uri": params.text_document.uri.to_string()}});
            request_mapper::rewrite_positions(&mut req_params, &mapper, true);

            let (binary_name, args) = match self.config.get_command_and_args(&lang) {
                Some((cmd, args)) => (cmd, args),
                None => {
                    warn!("[LiterateLsp] No LSP found for language '{}'. Check: literate-lsp --health {}", lang, lang);
                    return Ok(None);
                }
            };

            let mut child_lsps = self.child_lsps.write().await;
            let root_uri_str = params.text_document.uri.to_string();
            let root_uri_base = uri_helpers::extract_root_uri_base(&root_uri_str);
            let virtual_uri = uri_helpers::construct_virtual_uri(root_uri_base, &lang);

            if !child_lsps.contains_key(&lang) {
                let init_params = ChildLspInitParams {
                    lang: lang.clone(),
                    binary_name,
                    args,
                    root_uri_base: root_uri_base.to_string(),
                    virtual_uri: virtual_uri.clone(),
                    virtual_doc_content: vdoc.content.clone(),
                    init_options: self.config.get_init_options(&lang),
                };

                if let Ok(result) = ChildLspInitializer::initialize_child_lsp(init_params).await {
                    self.cache_completion_triggers(&result.lang, &result.lsp).await;
                    child_lsps.insert(result.lang, result.lsp);
                }
            }

            if let Some(child_lsp) = child_lsps.get(&lang) {
                if let Ok(mut response) = child_lsp
                    .send_request_raw("textDocument/documentSymbol", req_params)
                    .await
                {
                    request_mapper::rewrite_positions(&mut response, &mapper, false);
                    return Ok(serde_json::from_value(response).ok());
                }
            }
        }
        Ok(None)
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> JsonrpcResult<Option<CodeActionResponse>> {
        let position = params.range.start;
        let uri = params.text_document.uri.clone();

        let response = self
            .handle_position_request("textDocument/codeAction", position, uri)
            .await?;
        if let Some(result) = response.get("result") {
            Ok(serde_json::from_value(result.clone()).ok())
        } else {
            Ok(None)
        }
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> JsonrpcResult<Option<Vec<TextEdit>>> {
        let doc = self.document.read().await;
        let doc_content = match doc.as_ref() {
            Some(c) => c.clone(),
            None => return Ok(None),
        };

        // For formatting, we need to format all code blocks
        if let Some((lang, _, _)) = find_code_block_at_line(&doc_content, 0) {
            let vdoc = build_virtual_document(&doc_content, &lang);
            let mapper = PositionMapper::new(vdoc.blocks.clone());

            let mut req_params =
                json!({"textDocument": {"uri": params.text_document.uri.to_string()}});
            request_mapper::rewrite_positions(&mut req_params, &mapper, true);

            let (binary_name, args) = match self.config.get_command_and_args(&lang) {
                Some((cmd, args)) => (cmd, args),
                None => {
                    warn!("[LiterateLsp] No LSP found for language '{}'. Check: literate-lsp --health {}", lang, lang);
                    return Ok(None);
                }
            };

            let mut child_lsps = self.child_lsps.write().await;
            let root_uri_str = params.text_document.uri.to_string();
            let root_uri_base = uri_helpers::extract_root_uri_base(&root_uri_str);
            let virtual_uri = uri_helpers::construct_virtual_uri(root_uri_base, &lang);

            if !child_lsps.contains_key(&lang) {
                let init_params = ChildLspInitParams {
                    lang: lang.clone(),
                    binary_name,
                    args,
                    root_uri_base: root_uri_base.to_string(),
                    virtual_uri: virtual_uri.clone(),
                    virtual_doc_content: vdoc.content.clone(),
                    init_options: self.config.get_init_options(&lang),
                };

                if let Ok(result) = ChildLspInitializer::initialize_child_lsp(init_params).await {
                    self.cache_completion_triggers(&result.lang, &result.lsp).await;
                    child_lsps.insert(result.lang, result.lsp);
                }
            }

            if let Some(child_lsp) = child_lsps.get(&lang) {
                if let Ok(mut response) = child_lsp
                    .send_request_raw("textDocument/formatting", req_params)
                    .await
                {
                    request_mapper::rewrite_positions(&mut response, &mapper, false);
                    return Ok(serde_json::from_value(response).ok());
                }
            }
        }
        Ok(None)
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> JsonrpcResult<Option<Vec<TextEdit>>> {
        let position = params.range.start;
        let uri = params.text_document.uri.clone();

        let response = self
            .handle_position_request("textDocument/rangeFormatting", position, uri)
            .await?;
        if let Some(result) = response.get("result") {
            Ok(serde_json::from_value(result.clone()).ok())
        } else {
            Ok(None)
        }
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> JsonrpcResult<Option<CompletionResponse>> {
        let position = params.text_document_position.position;
        let uri = params.text_document_position.text_document.uri;

        let response = self
            .handle_position_request("textDocument/completion", position, uri)
            .await?;
        if let Some(result) = response.get("result") {
            Ok(serde_json::from_value(result.clone()).ok())
        } else {
            Ok(None)
        }
    }

    async fn shutdown(&self) -> JsonrpcResult<()> {
        info!("[LiterateLsp] Shutdown requested");
        // Just clear the child LSPs - Drop impl will kill processes
        // Don't try to gracefully shutdown as it can hang
        let mut child_lsps = self.child_lsps.write().await;
        child_lsps.clear();
        info!("[LiterateLsp] Child LSPs cleaned up");
        Ok(())
    }
}
