use tower_lsp::lsp_types::Url;

/// Extract base URI directory from a full URI string
///
/// # Examples
/// - "file:///home/user/project/example.md" → "file:///home/user/project"
/// - "file:///home/user/example.md" → "file:///home/user"
pub fn extract_root_uri_base(uri_str: &str) -> &str {
    uri_str.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

/// Construct virtual document URI for a given language
///
/// # Examples
/// - ("file:///home/user/project", "forth") → "file:///home/user/project/virtual.forth"
pub fn construct_virtual_uri(root_uri_base: &str, lang: &str) -> String {
    format!("{}/virtual.{}", root_uri_base, lang)
}

/// Extract file name from URL
pub fn extract_filename(uri: &Url) -> &str {
    uri.path_segments()
        .and_then(|mut segs| segs.next_back())
        .unwrap_or("document")
}

/// Construct temporary virtual doc path for debugging
pub fn construct_temp_vdoc_path(lang: &str) -> String {
    format!("{}/virtual.{}", crate::utils::constants::VIRTUAL_DOC_DEBUG_DIR, lang)
}
