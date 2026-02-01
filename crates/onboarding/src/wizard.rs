//! Terminal-based onboarding wizard using the shared state machine.

use std::io::{BufRead, Write};

use moltis_config::{MoltisConfig, find_or_default_config_path, save_config};

use crate::state::WizardState;

/// Run the interactive onboarding wizard in the terminal.
pub async fn run_onboarding() -> anyhow::Result<()> {
    let config_path = find_or_default_config_path();

    // Check if already onboarded.
    if config_path.exists()
        && let Ok(cfg) = moltis_config::loader::load_config(&config_path)
        && cfg.is_onboarded()
    {
        println!(
            "Already onboarded as {} with agent {}.",
            cfg.user.name.as_deref().unwrap_or("?"),
            cfg.identity.name.as_deref().unwrap_or("?"),
        );
        return Ok(());
    }

    let mut state = WizardState::new();
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();

    while !state.is_done() {
        println!("{}", state.prompt());
        print!("> ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        reader.read_line(&mut line)?;
        state.advance(&line);
    }

    // Merge into existing config or create new one.
    let mut config = if config_path.exists() {
        moltis_config::loader::load_config(&config_path).unwrap_or_default()
    } else {
        MoltisConfig::default()
    };
    config.identity = state.identity;
    config.user = state.user;

    let path = save_config(&config)?;
    println!("Config saved to {}", path.display());
    println!("Onboarding complete!");
    Ok(())
}
