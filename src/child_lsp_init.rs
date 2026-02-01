use crate::child_lsp::ChildLspManager;
use crate::utils::logging;
use anyhow::Result;
use std::sync::Arc;
use tracing::debug;

/// Parameters required to initialize a child LSP
pub struct ChildLspInitParams {
    pub lang: String,
    pub binary_name: String,
    pub args: Vec<String>,
    pub root_uri_base: String,
    pub virtual_uri: String,
    pub virtual_doc_content: Arc<String>,
    pub init_options: Option<serde_json::Value>,
}

/// Result of child LSP initialization
pub struct ChildLspInitResult {
    pub lsp: ChildLspManager,
    pub lang: String,
}

/// Orchestrates the complete child LSP initialization sequence
pub struct ChildLspInitializer;

impl ChildLspInitializer {
    /// Initialize and spawn a child LSP with complete setup
    ///
    /// Orchestrates:
    /// 1. Spawn child process
    /// 2. Initialize LSP connection
    /// 3. Open virtual document
    ///
    /// Returns the initialized LSP and language, or an error message for user display
    /// Caller is responsible for caching completion triggers after successful initialization
    pub async fn initialize_child_lsp(
        params: ChildLspInitParams,
    ) -> Result<ChildLspInitResult, String> {
        logging::log_child_lsp_spawn(&params.binary_name);

        // Stage 1: Spawn
        let lsp = match ChildLspManager::spawn(&params.binary_name, params.args).await {
            Ok(lsp) => lsp,
            Err(e) => {
                let msg = format!("Failed to spawn child LSP for '{}': {}", params.lang, e);
                debug!("[ChildLspInit] {}", msg);
                return Err(msg);
            }
        };

        // Stage 2: Initialize
        if let Err(e) = lsp
            .initialize(params.root_uri_base, params.init_options)
            .await
        {
            let msg = format!("Failed to initialize child LSP for '{}': {}", params.lang, e);
            debug!("[ChildLspInit] {}", msg);
            return Err(msg);
        }

        // Stage 3: Open document
        if let Err(e) = lsp
            .did_open(
                params.virtual_uri,
                params.lang.clone(),
                (*params.virtual_doc_content).clone(),
            )
            .await
        {
            let msg = format!("Failed to open virtual document for '{}': {}", params.lang, e);
            debug!("[ChildLspInit] {}", msg);
            return Err(msg);
        }

        logging::log_child_lsp_initialized(&params.lang);

        Ok(ChildLspInitResult {
            lsp,
            lang: params.lang,
        })
    }
}
