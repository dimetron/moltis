use {anyhow::Result, clap::Subcommand};

use moltis_tools::sandbox;

#[derive(Subcommand)]
pub enum SandboxAction {
    /// List pre-built sandbox images.
    List,
    /// Build a sandbox image from the configured base + packages.
    Build,
    /// Remove a specific sandbox image by tag.
    Remove {
        /// Image tag (e.g. moltis-sandbox:abc123).
        tag: String,
    },
    /// Remove all pre-built sandbox images.
    Clean,
}

pub async fn handle_sandbox(action: SandboxAction) -> Result<()> {
    match action {
        SandboxAction::List => list().await,
        SandboxAction::Build => build().await,
        SandboxAction::Remove { tag } => remove(&tag).await,
        SandboxAction::Clean => clean().await,
    }
}

async fn list() -> Result<()> {
    let images = sandbox::list_sandbox_images().await?;
    if images.is_empty() {
        println!("No sandbox images found.");
        return Ok(());
    }
    println!("{:<45} {:>10}  CREATED", "TAG", "SIZE");
    for img in &images {
        println!("{:<45} {:>10}  {}", img.tag, img.size, img.created);
    }
    Ok(())
}

async fn build() -> Result<()> {
    let config = moltis_config::discover_and_load();
    let sandbox_config = sandbox::SandboxConfig::from(&config.tools.exec.sandbox);

    let packages = sandbox_config.packages.clone();
    if packages.is_empty() {
        println!("No packages configured — nothing to build.");
        println!("Add packages to [tools.exec.sandbox] in your config file.");
        return Ok(());
    }

    let base = sandbox_config
        .image
        .clone()
        .unwrap_or_else(|| sandbox::DEFAULT_SANDBOX_IMAGE.to_string());
    let tag = sandbox::sandbox_image_tag(&base, &packages);
    println!("Base:     {base}");
    println!("Packages: {}", packages.join(", "));
    println!("Tag:      {tag}");
    println!();

    // Force mode to All so create_sandbox returns a real backend.
    let sandbox_config = sandbox::SandboxConfig {
        mode: sandbox::SandboxMode::All,
        ..sandbox_config
    };
    let backend = sandbox::create_sandbox(sandbox_config);
    match backend.build_image(&base, &packages).await? {
        Some(result) => {
            if result.built {
                println!("Image built successfully: {}", result.tag);
            } else {
                println!("Image already exists: {}", result.tag);
            }
        },
        None => {
            println!(
                "Backend '{}' does not support image building.",
                backend.backend_name()
            );
        },
    }
    Ok(())
}

async fn remove(tag: &str) -> Result<()> {
    sandbox::remove_sandbox_image(tag).await?;
    println!("Removed: {tag}");
    Ok(())
}

async fn clean() -> Result<()> {
    let count = sandbox::clean_sandbox_images().await?;
    if count == 0 {
        println!("No sandbox images to remove.");
    } else {
        println!(
            "Removed {count} sandbox image{}.",
            if count == 1 {
                ""
            } else {
                "s"
            }
        );
    }
    Ok(())
}
