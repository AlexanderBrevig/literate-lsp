use literate_lsp::config::Config;
use literate_lsp::health;
use literate_lsp::server::LiterateLsp;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check for --health flag
    if args.len() > 1 && args[1] == "--health" {
        let config = Config::load_for_health_check();
        let lang_filter = args.get(2).map(|s| s.as_str());
        health::check_health(&config, lang_filter);
        return;
    }

    // Check for --languages flag
    if args.len() > 1 && args[1] == "--languages" {
        let config = Config::load_for_health_check();
        health::list_languages(&config);
        return;
    }

    // Normal LSP server mode
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into()),
        )
        .init();

    // Load configuration from Helix + local overrides
    let config = Config::load_with_local_overrides();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(move |client| LiterateLsp::new(client, config.clone()));
    Server::new(stdin, stdout, socket).serve(service).await;
}
