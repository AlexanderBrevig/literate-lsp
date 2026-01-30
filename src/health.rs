use crate::config::{Config, FORBIDDEN_FORMATS};
use std::path::Path;
use rayon::prelude::*;

/// Explanation for why documentation formats are forbidden as child LSPs
const FORBIDDEN_REASON: &str =
    "Not supported: literate-lsp cannot be a child LSP for documentation formats.\n    \
     This prevents fork bombs - literate-lsp must be the root LSP, not a child of itself.";

/// Build explanation with optional detail (e.g., which formats/languages use this LSP)
fn forbidden_explanation(detail: Option<&str>) -> String {
    match detail {
        Some(d) => format!("{}\n    ({})", FORBIDDEN_REASON, d),
        None => FORBIDDEN_REASON.to_string(),
    }
}

/// Map common language name aliases to LSP server names
fn resolve_lsp_name(name: &str) -> String {
    let lower = name.to_lowercase();
    // Try common aliases
    match lower.as_str() {
        "md" => "marksman".to_string(),
        // Otherwise use name as-is (it's either the LSP name or will be searched)
        _ => lower,
    }
}

/// Check health of configured LSPs (only show installed ones)
pub fn check_health(config: &Config, lang_filter: Option<&str>) {
    if let Some(lang) = lang_filter {
        // Check if format is forbidden first (before resolving aliases)
        if config.is_format_forbidden(lang) {
            println!("  {}", lang);
            println!("    {}", forbidden_explanation(None));
            return;
        }
        // Resolve language name alias to LSP name if needed
        let lsp_name = resolve_lsp_name(lang);
        // Check specific language (show config details when querying specific LSP)
        check_language(config, &lsp_name);
    } else {
        // Check all configured LSPs in parallel, but only show installed ones
        println!("Literate-LSP Health Check\n");
        println!("Installed LSPs:");

        let mut lsps: Vec<_> = config.language_server.keys().collect();
        lsps.sort();

        // Check all LSPs in parallel
        let results: Vec<_> = lsps
            .par_iter()
            .filter_map(|lsp_name| check_lsp_status(config, lsp_name))
            .collect();

        // Sort results back by LSP name for consistent output
        let mut results = results;
        results.sort_by(|a, b| a.0.cmp(&b.0));

        if results.is_empty() {
            println!("  (no LSPs installed)");
        } else {
            for (lsp_name, command, path) in results {
                print!("  {} ({})", lsp_name, command);
                println!(" ✓");
                println!("    Path: {}", path);
            }
        }
    }
}

/// Check status of a single LSP (returns name, command, path if installed)
fn check_lsp_status(config: &Config, lsp_name: &str) -> Option<(String, String, String)> {
    config.language_server.get(lsp_name).and_then(|lsp_cfg| {
        let command = &lsp_cfg.command;
        // Skip entries without commands (nested config sections)
        if command.is_empty() {
            return None;
        }
        which(command).ok().map(|path| (
            lsp_name.to_string(),
            command.clone(),
            path,
        ))
    })
}

/// List all configured languages and their LSPs
pub fn list_languages(config: &Config) {
    println!("Configured Languages:\n");

    if config.language.is_empty() {
        println!("  (no languages configured)");
        return;
    }

    let mut languages = config.language.clone();
    languages.sort_by(|a, b| a.name.cmp(&b.name));

    let forbidden_lsps = config.get_forbidden_lsps();

    for lang in languages {
        let server_names = lang.get_server_names();
        if server_names.is_empty() {
            continue; // Skip languages with no LSPs
        }

        let servers: Vec<String> = server_names
            .iter()
            .filter(|srv| !forbidden_lsps.contains(srv))
            .cloned()
            .collect();

        if servers.is_empty() {
            println!("  {} → (all LSPs forbidden)", lang.name);
        } else {
            println!("  {} → {}", lang.name, servers.join(", "));
        }
    }
}

fn check_language(config: &Config, lang: &str) {
    // Check if format is forbidden first
    if config.is_format_forbidden(lang) {
        println!("  {}", lang);
        println!("    {}", forbidden_explanation(None));
        return;
    }

    let forbidden_lsps = config.get_forbidden_lsps();

    // First, check if this is a language name (not LSP name)
    if let Some(language) = config.language.iter().find(|l| l.name == lang) {
        let server_names = language.get_server_names();
        if server_names.is_empty() {
            println!("  {} - (no LSPs configured)", lang);
            return;
        }

        println!("  {} - language servers:", lang);
        for server_name in server_names {
            if forbidden_lsps.contains(&server_name) {
                println!("    {} (forbidden)", server_name);
                // Find which forbidden formats this LSP serves
                let serving_formats: Vec<String> = config
                    .language
                    .iter()
                    .filter(|l| {
                        let fmt = l.name.as_str();
                        FORBIDDEN_FORMATS.contains(&fmt)
                            || FORBIDDEN_FORMATS.iter().any(|f| fmt.contains(f))
                    })
                    .filter(|l| l.get_server_names().contains(&server_name))
                    .map(|l| l.name.clone())
                    .collect();
                if serving_formats.is_empty() {
                    println!("      {}", forbidden_explanation(None));
                } else {
                    let detail_str = serving_formats.join(", ");
                    println!("      {}", forbidden_explanation(Some(&detail_str)));
                }
                continue;
            }

            if let Some(lsp_cfg) = config.language_server.get(&server_name) {
                let command = &lsp_cfg.command;
                if command.is_empty() {
                    continue; // Skip entries without commands (nested config sections)
                }

                match which(command) {
                    Ok(path) => {
                        println!("    ✓ {} ({})", server_name, command);
                        println!("      Path: {}", path);
                    }
                    Err(_) => {
                        println!("    ✗ {} ({})", server_name, command);
                    }
                }
            }
        }
        return;
    }

    // Otherwise, treat it as an LSP server name
    if let Some(lsp_cfg) = config.language_server.get(lang) {
        let command = &lsp_cfg.command;

        // Skip nested config sections that don't have commands
        if command.is_empty() {
            println!("  {} - not configured", lang);
            return;
        }

        // Find which languages use this LSP
        let using_languages: Vec<String> = config
            .language
            .iter()
            .filter(|l| l.get_server_names().contains(&lang.to_string()))
            .map(|l| l.name.clone())
            .collect();

        // Check if binary exists in PATH
        match which(command) {
            Ok(path) => {
                println!("  ✓ {} ({})", lang, command);
                println!("    Path: {}", path);
                if !using_languages.is_empty() {
                    println!("    Used by: {}", using_languages.join(", "));
                }

                // Show configuration if not empty (only when querying specific LSP)
                if !lsp_cfg.config.is_null()
                    && lsp_cfg
                        .config
                        .as_object()
                        .is_some_and(|m| !m.is_empty())
                {
                    println!(
                        "    Config: {}",
                        serde_json::to_string_pretty(&lsp_cfg.config)
                            .unwrap_or_default()
                    );
                }
            }
            Err(_) => {
                println!("  ✗ {} ({})", lang, command);
                if !using_languages.is_empty() {
                    println!("    Used by: {}", using_languages.join(", "));
                }
            }
        }
    } else {
        // Try to find similar LSP names in config
        let similar: Vec<_> = config
            .language_server
            .keys()
            .filter(|k| k.contains(lang) || k.starts_with(lang))
            .cloned()
            .collect();

        if !similar.is_empty() {
            println!("  {} - related LSPs:", lang);
            for lsp_name in similar {
                if let Some(lsp_cfg) = config.language_server.get(&lsp_name) {
                    let command = &lsp_cfg.command;

                    // Skip nested config sections that don't have commands
                    if command.is_empty() {
                        continue;
                    }

                    // Find which languages use this LSP
                    let using_languages: Vec<String> = config
                        .language
                        .iter()
                        .filter(|l| l.get_server_names().contains(&lsp_name))
                        .map(|l| l.name.clone())
                        .collect();

                    // Check if binary exists in PATH
                    match which(command) {
                        Ok(path) => {
                            println!("    ✓ {} ({})", lsp_name, command);
                            println!("      Path: {}", path);
                            if !using_languages.is_empty() {
                                println!("      Used by: {}", using_languages.join(", "));
                            }
                        }
                        Err(_) => {
                            println!("    ✗ {} ({})", lsp_name, command);
                            if !using_languages.is_empty() {
                                println!("      Used by: {}", using_languages.join(", "));
                            }
                        }
                    }
                }
            }
        } else {
            println!("  {} - not configured", lang);
        }
    }
}

/// Simple implementation of `which` to find a binary in PATH
fn which(cmd: &str) -> Result<String, ()> {
    if let Ok(path_env) = std::env::var("PATH") {
        for path_dir in path_env.split(':') {
            let full_path = format!("{}/{}", path_dir, cmd);
            if Path::new(&full_path).exists() {
                return Ok(full_path);
            }
        }
    }
    Err(())
}
