use tracing::{debug, warn};

pub fn log_code_block_detected(lang: &str) {
    debug!("[LiterateLsp] Detected code block language: '{}'", lang);
}

pub fn log_no_code_block_at_line(markdown_line: usize) {
    debug!("[LiterateLsp] No code block found at line {}", markdown_line);
}

pub fn log_self_referential_skip(lang: &str) {
    debug!("[LiterateLsp] Skipping language '{}' (self-referential)", lang);
}

pub fn log_virtual_doc_built(lang: &str, block_count: usize, content_len: usize) {
    debug!(
        "[VirtualDoc] Built virtual doc: lang='{}', blocks={}, len={}",
        lang, block_count, content_len
    );
}

pub fn log_virtual_doc_empty(_lang: &str, is_empty: bool, blocks: usize) {
    debug!("[LiterateLsp] Virtual doc empty: {}, blocks: {}", is_empty, blocks);
}

pub fn log_no_lsp_found(lang: &str) {
    warn!(
        "[LiterateLsp] No LSP found for language '{}'. Check: literate-lsp --health {}",
        lang, lang
    );
}

pub fn log_child_lsp_spawn(binary: &str) {
    debug!("[ChildLSP] Spawning: {}", binary);
}

pub fn log_child_lsp_initialized(lang: &str) {
    debug!("[ChildLSP] Initialized and ready for language: {}", lang);
}

pub fn log_request_at_position(method: &str, line: u32, character: u32) {
    debug!("[LiterateLsp] Request: {} at line:{} char:{}", method, line, character);
}

pub fn log_server_lookup(lang: &str, server_name: &str, found: bool) {
    debug!(
        "[Config] Checking server '{}' for language '{}': {}",
        server_name,
        lang,
        if found { "found" } else { "not found" }
    );
}

pub fn log_language_config_found(lang: &str, server_count: usize) {
    debug!(
        "[Config] Found language '{}' with {} servers",
        lang, server_count
    );
}

pub fn log_language_config_not_found(lang: &str) {
    debug!("[Config] Language '{}' not in configuration", lang);
}

pub fn log_server_config_not_found(server_name: &str) {
    debug!(
        "[Config] Server configuration '{}' not found in language-server section",
        server_name
    );
}
