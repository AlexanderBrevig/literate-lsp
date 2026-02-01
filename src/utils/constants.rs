/// Default completion trigger characters across languages
pub const DEFAULT_COMPLETION_TRIGGERS: &[&str] = &[" ", "."];

/// Temporary directory for virtual document debugging
pub const VIRTUAL_DOC_DEBUG_DIR: &str = "/tmp";

/// LSP response timeout in seconds
pub const LSP_RESPONSE_TIMEOUT_SECS: u64 = 5;

/// Error message for missing LSP configuration
pub const ERROR_NO_LSP_FOUND: &str =
    "**Language '{}' is not configured.**\n\n\
     To add IDE support, add this to `$root/.languages.toml`:\n\n\
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
     Find available LSPs at: https://langserver.org";

/// Error message for language configured but no LSP
pub const ERROR_LSP_NOT_CONFIGURED: &str =
    "**Language '{}' is configured but has no LSP server.**\n\n\
     To add IDE support, add this to `$root/.languages.toml`:\n\n\
     ```toml\n\
     [[language]]\n\
     name = \"{}\"\n\
     language-servers = [{}]\n\
     ```\n\n\
     Find available LSPs at: https://langserver.org";

/// Error message for self-referential code blocks
pub const ERROR_SELF_REFERENTIAL: &str =
    "Cannot provide IDE features for **{}** code blocks inside **{}** documents.\n\n\
     **Why?** This would create a recursive loop (literate-lsp acting on itself).\n\n\
     **Solution:** Move the {} code outside the {} fence, or use a different documentation format.";

/// Error message for no code blocks found
pub const ERROR_NO_CODE_BLOCKS: &str = "No code blocks found in this document";

/// Error message for language not found in document
pub const ERROR_NO_LANGUAGE_BLOCKS: &str =
    "No '{}' code blocks found.\n\n\
     Found: {}\n\n\
     **Note:** Code blocks nested inside other fences (like markdown examples) are not supported. \
     Move the {} code outside the markdown fence.";

/// Log message when no LSP is found for a language
pub const LOG_NO_LSP_FOUND: &str =
    "[LiterateLsp] No LSP found for language '{}'. Check: literate-lsp --health {}";

/// Hardcoded file extensions
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkdn", "mdx", "mmd"];
pub const TYPST_EXTENSIONS: &[&str] = &["typ"];
pub const GO_EXTENSIONS: &[&str] = &["go"];
pub const FORTH_EXTENSIONS: &[&str] = &["forth", "fth"];
