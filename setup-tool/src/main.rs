use clap::{Parser, Subcommand};
use dialoguer::{Input, Password, Confirm};
use reqwest::Client;
use serde::Deserialize;
use std::process::{Command, Stdio};
use std::fs;
use std::collections::HashMap;
use std::io::BufRead;
use toml::Value;
use console::style;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Configure and deploy the Fastly service for the first time
    Install(InstallArgs),
    /// Update KV store values without redeploying
    Update(UpdateArgs),
    /// Redeploy the Fastly service with latest code
    Deploy(DeployArgs),
    /// Tail the logs for the deployed service
    Tail(TailArgs),
}

#[derive(Parser, Debug)]
struct InstallArgs {
    /// Your Fastly API token
    #[arg(long)]
    fastly_token: Option<String>,

    /// Your StegaWave API key
    #[arg(long)]
    stegawave_api_key: Option<String>,
}

#[derive(Parser, Debug)]
struct UpdateArgs {
    /// Your Fastly API token
    #[arg(long)]
    fastly_token: Option<String>,

    /// Update only specific keys (comma-separated)
    #[arg(long)]
    keys: Option<String>,
}

#[derive(Parser, Debug)]
struct DeployArgs {
    /// Your Fastly API token
    #[arg(long)]
    fastly_token: Option<String>,

    /// Skip building and just deploy
    #[arg(long)]
    skip_build: bool,
}

#[derive(Parser, Debug)]
struct TailArgs {
    /// Your Fastly API token
    #[arg(long)]
    fastly_token: Option<String>,
}


#[derive(Deserialize, Debug)]
struct ApiSecret {
    secret: String,
}

/// Load configuration from CONFIG.txt file
fn load_config() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut config = HashMap::new();
    
    // Set default values
    config.insert("FMP4_AAC_PROFILE".to_string(), "AAC-LC".to_string());
    config.insert("FMP4_SAMPLE_RATE".to_string(), "44100".to_string());
    config.insert("FMP4_CHANNELS".to_string(), "2".to_string());
    config.insert("FMP4_TRACK_ID".to_string(), "1".to_string());
    
    // Try to load from CONFIG.txt
    if let Ok(content) = fs::read_to_string("CONFIG.txt") {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().to_string();
                if !value.is_empty() {
                    config.insert(key, value);
                }
            }
        }
    }
    
    Ok(config)
}

/// Save configuration to CONFIG.txt file
fn save_config(config: &HashMap<String, String>) -> Result<(), Box<dyn std::error::Error>> {
    let content = format!(
        r#"# StegaWave Configuration
# This file contains default values for KV store entries.
# Edit these values as needed for your deployment.

# === Audio Encoding Configuration ===
# These values control how audio segments are processed for watermarking

# AAC Profile to use for encoding (typically AAC-LC)
FMP4_AAC_PROFILE={}

# Sample rate in Hz (44100 is standard CD quality)
FMP4_SAMPLE_RATE={}

# Number of audio channels (2 for stereo)
FMP4_CHANNELS={}

# Track ID for the audio track in the FMP4 container
FMP4_TRACK_ID={}

# === Service Configuration ===
# These values are automatically populated during setup but can be updated

# Your StegaWave API key (will be set during setup)
STEGAWAVE_API_KEY={}

# Fastly API token (will be set during setup)
FASTLY_API_TOKEN={}

# === Advanced Configuration ===
# These values typically don't need to be changed

# Watermarking probability (0.01 = 1% chance)
WATERMARK_PROBABILITY={}
"#,
        config.get("FMP4_AAC_PROFILE").unwrap_or(&"AAC-LC".to_string()),
        config.get("FMP4_SAMPLE_RATE").unwrap_or(&"44100".to_string()),
        config.get("FMP4_CHANNELS").unwrap_or(&"2".to_string()),
        config.get("FMP4_TRACK_ID").unwrap_or(&"1".to_string()),
        config.get("STEGAWAVE_API_KEY").unwrap_or(&"".to_string()),
        config.get("FASTLY_API_TOKEN").unwrap_or(&"".to_string()),
        config.get("WATERMARK_PROBABILITY").unwrap_or(&"0.01".to_string()),
    );
    
    fs::write("CONFIG.txt", content)?;
    Ok(())
}

/// Prompt user for configuration values
fn prompt_for_config_values(config: &mut HashMap<String, String>) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", style("=== Audio Encoding Configuration ===").bold());
    println!("Configure audio encoding parameters for watermarking:");
    
    let aac_profile = Input::new()
        .with_prompt("AAC Profile")
        .default(config.get("FMP4_AAC_PROFILE").unwrap_or(&"AAC-LC".to_string()).clone())
        .interact_text()?;
    config.insert("FMP4_AAC_PROFILE".to_string(), aac_profile);
    
    let sample_rate = Input::new()
        .with_prompt("Sample Rate (Hz)")
        .default(config.get("FMP4_SAMPLE_RATE").unwrap_or(&"44100".to_string()).clone())
        .interact_text()?;
    config.insert("FMP4_SAMPLE_RATE".to_string(), sample_rate);
    
    let channels = Input::new()
        .with_prompt("Number of Channels")
        .default(config.get("FMP4_CHANNELS").unwrap_or(&"2".to_string()).clone())
        .interact_text()?;
    config.insert("FMP4_CHANNELS".to_string(), channels);
    
    let track_id = Input::new()
        .with_prompt("Track ID")
        .default(config.get("FMP4_TRACK_ID").unwrap_or(&"1".to_string()).clone())
        .interact_text()?;
    config.insert("FMP4_TRACK_ID".to_string(), track_id);
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install(args) => install(args).await?,
        Commands::Update(args) => update(args).await?,
        Commands::Deploy(args) => deploy(args).await?,
        Commands::Tail(args) => tail(args).await?,
    }

    Ok(())
}

async fn tail(args: TailArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", style("Tailing logs...").bold());

    let fastly_toml_str = fs::read_to_string("fastly.toml").map_err(|_| "Failed to read fastly.toml. Have you run `setup install` first?")?;
    let toml_value: Value = toml::from_str(&fastly_toml_str)?;
    let service_id = toml_value["service_id"].as_str().ok_or("service_id not found in fastly.toml. Have you run `setup install` first?")?;

    let fastly_token = args.fastly_token.unwrap_or_else(|| {
        Password::new()
            .with_prompt("Enter your Fastly API token")
            .interact()
            .unwrap()
    });

    let mut child = Command::new("fastly")
        .arg("log-tail")
        .arg("--service-id")
        .arg(service_id)
        .env("FASTLY_API_TOKEN", fastly_token)
        .spawn()?;

    child.wait()?;

    Ok(())
}

async fn install(args: InstallArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", style("Welcome to the StegaWave Fastly Compute@Edge Setup").bold());

    // Check for Fastly CLI
    if Command::new("fastly").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_err() {
        println!("{}", style("Fastly CLI not found. Please install it first:").red());
        println!("https://developer.fastly.com/learning/tools/cli/#installation");
        return Ok(());
    }
    println!("{}", style("✓ Fastly CLI is installed.").green());

    // Load existing configuration
    let mut config = load_config()?;
    
    // Get API credentials
    let fastly_token = args.fastly_token.unwrap_or_else(|| {
        Password::new()
            .with_prompt("Enter your Fastly API token")
            .interact()
            .unwrap()
    });

    let stegawave_api_key = args.stegawave_api_key.unwrap_or_else(|| {
        Input::new()
            .with_prompt("Enter your StegaWave API key")
            .interact_text()
            .unwrap()
    });

    // Store credentials in config
    config.insert("FASTLY_API_TOKEN".to_string(), fastly_token.clone());
    config.insert("STEGAWAVE_API_KEY".to_string(), stegawave_api_key.clone());

    // Prompt for audio encoding configuration
    println!("\n{}", style("=== Configuration ===").bold());
    if Confirm::new()
        .with_prompt("Do you want to configure audio encoding parameters?")
        .default(true)
        .interact()? 
    {
        prompt_for_config_values(&mut config)?;
    }

    // Save configuration
    save_config(&config)?;
    println!("{}", style("✓ Configuration saved to CONFIG.txt").green());

    // Fetch Master Secret
    println!("Fetching master secret from StegaWave API...");
    let client = Client::new();
    let res = client
        .get("https://api.stegawave.com/getsecret")
        .header("X-API-Key", &stegawave_api_key)
        .send()
        .await?;

    if res.status().is_success() {
        let secret_body = res.text().await?;
        let api_secret: ApiSecret = serde_json::from_str(&secret_body)?;
        println!("{}", style("✓ Successfully fetched master secret.").green());

        // Build and Deploy
        println!("Building and deploying the Fastly application...");
        let mut build_command = Command::new("fastly");
        build_command.arg("compute").arg("build").arg("--verbose");
        
        let mut child = build_command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        
        if let Some(stdout) = child.stdout.take() {
            let reader = std::io::BufReader::new(stdout);
            for line in std::io::BufRead::lines(reader) {
                println!("{}", line?);
            }
        }

        if let Some(stderr) = child.stderr.take() {
            let reader = std::io::BufReader::new(stderr);
            for line in std::io::BufRead::lines(reader) {
                eprintln!("{}", line?);
            }
        }

        let build_status = child.wait()?;

        if !build_status.success() {
            println!("{}", style("Failed to build the application.").red());
            return Ok(());
        }

        let deploy_output = Command::new("fastly").arg("compute").arg("deploy").output()?;
        if !deploy_output.status.success() {
            println!("{}", style("Failed to deploy the application:").red());
            println!("{}", String::from_utf8_lossy(&deploy_output.stderr));
            return Ok(());
        }
        println!("{}", style("✓ Application deployed successfully.").green());

        let fastly_toml_str = fs::read_to_string("fastly.toml")?;
        let toml_value: Value = toml::from_str(&fastly_toml_str)?;
        let service_id = toml_value["service_id"].as_str().unwrap();

        // Create KV Stores
        println!("Creating KV stores...");
        create_kv_store("secrets", &fastly_token).await?;
        create_kv_store("api_keys", &fastly_token).await?;
        create_kv_store("watermarking_config", &fastly_token).await?;
        println!("{}", style("✓ KV stores created.").green());

        // Populate KV Stores
        println!("Populating KV stores...");
        
        println!("Getting secrets KV store ID...");
        let secrets_id = get_kv_store_id("secrets", &fastly_token).await?;
        println!("✓ Got secrets KV store ID: {}", secrets_id);
        
        println!("Getting api_keys KV store ID...");
        let api_keys_id = get_kv_store_id("api_keys", &fastly_token).await?;
        println!("✓ Got api_keys KV store ID: {}", api_keys_id);
        
        println!("Getting watermarking_config KV store ID...");
        let watermarking_config_id = get_kv_store_id("watermarking_config", &fastly_token).await?;
        println!("✓ Got watermarking_config KV store ID: {}", watermarking_config_id);

        // Populate with secrets and API keys
        populate_kv_store_entry(&secrets_id, "SECRET_KEY_HEX", &api_secret.secret, &fastly_token).await?;
        populate_kv_store_entry(&api_keys_id, "service_api_key", &stegawave_api_key, &fastly_token).await?;
        
        // Populate with configuration values
        populate_kv_store_entry(&watermarking_config_id, "FMP4_AAC_PROFILE", 
            config.get("FMP4_AAC_PROFILE").unwrap_or(&"AAC-LC".to_string()), &fastly_token).await?;
        populate_kv_store_entry(&watermarking_config_id, "FMP4_SAMPLE_RATE", 
            config.get("FMP4_SAMPLE_RATE").unwrap_or(&"44100".to_string()), &fastly_token).await?;
        populate_kv_store_entry(&watermarking_config_id, "FMP4_CHANNELS", 
            config.get("FMP4_CHANNELS").unwrap_or(&"2".to_string()), &fastly_token).await?;
        populate_kv_store_entry(&watermarking_config_id, "FMP4_TRACK_ID", 
            config.get("FMP4_TRACK_ID").unwrap_or(&"1".to_string()), &fastly_token).await?;
        
        println!("{}", style("✓ KV stores populated.").green());

        println!("\n{}", style("Setup Complete!").bold().green());
        let service_domain_output = Command::new("fastly").arg("service").arg("describe").arg("--service-id").arg(service_id).output()?;
        let service_domain = String::from_utf8_lossy(&service_domain_output.stdout);
        let domain_line = service_domain.lines().find(|line| line.starts_with("Domain:")).unwrap_or("");
        let domain = domain_line.split_whitespace().last().unwrap_or("N/A");
        println!("Service Domain: {}", style(domain).cyan());
        println!("\n{}", style("Next Steps:").bold());
        println!("• Edit CONFIG.txt to modify configuration values");
        println!("• Run 'setup-tool update' to update KV stores");
        println!("• Run 'setup-tool deploy' to redeploy after code changes");
        println!("• Run 'setup-tool tail' to view logs");

    } else {
        println!("{}", style("Failed to fetch master secret.").red());
        println!("Status: {}", res.status());
        let error_body = res.text().await?;
        println!("Response: {}", error_body);
    }

    Ok(())
}

async fn create_kv_store(name: &str, token: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating KV store: {}", name);
    
    let output = Command::new("fastly")
        .arg("kv-store")
        .arg("create")
        .arg("--name")
        .arg(name)
        .env("FASTLY_API_TOKEN", token)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        if stderr.contains("already exists") || stdout.contains("already exists") {
            println!("✓ KV store '{}' already exists", name);
        } else {
            println!("Error creating KV store '{}': {}", name, stderr);
            println!("Stdout: {}", stdout);
            return Err(format!("Failed to create KV store '{}': {}", name, stderr).into());
        }
    } else {
        println!("✓ Created KV store: {}", name);
    }
    Ok(())
}

async fn get_kv_store_id(name: &str, token: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("fastly")
        .arg("kv-store")
        .arg("describe")
        .arg(name)
        .arg("--json")
        .env("FASTLY_API_TOKEN", token)
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!("Failed to describe KV store '{}': {}\nStdout: {}", name, stderr, stdout).into());
    }
    
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    if stdout_str.trim().is_empty() {
        return Err(format!("Empty response when describing KV store '{}'", name).into());
    }
    
    let json: Value = serde_json::from_slice(&output.stdout)?;
    let id = json["id"].as_str()
        .ok_or_else(|| format!("KV store '{}' does not have an 'id' field in response", name))?;
    Ok(id.to_string())
}


async fn populate_kv_store_entry(store_id: &str, key: &str, value: &str, token: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Setting KV store entry: {} = {}", key, if key.contains("SECRET") { "[REDACTED]" } else { value });
    
    let output = Command::new("fastly")
        .arg("kv-store-entry")
        .arg("create")
        .arg("--store-id")
        .arg(store_id)
        .arg("--key")
        .arg(key)
        .arg("--value")
        .arg(value)
        .env("FASTLY_API_TOKEN", token)
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!("Failed to create KV store entry '{}': {}\nStdout: {}", key, stderr, stdout).into());
    }
    
    println!("✓ Successfully set KV store entry: {}", key);
    Ok(())
}

async fn update(args: UpdateArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", style("Updating KV store values...").bold());

    let fastly_token = args.fastly_token.unwrap_or_else(|| {
        Password::new()
            .with_prompt("Enter your Fastly API token")
            .interact()
            .unwrap()
    });

    // Load configuration
    let mut config = load_config()?;
    
    // Check if we should update specific keys only
    let keys_to_update: Vec<String> = if let Some(keys_str) = args.keys {
        keys_str.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        // Update all configuration keys
        vec![
            "FMP4_AAC_PROFILE".to_string(),
            "FMP4_SAMPLE_RATE".to_string(),
            "FMP4_CHANNELS".to_string(),
            "FMP4_TRACK_ID".to_string(),
        ]
    };

    // Get KV store IDs
    let watermarking_config_id = get_kv_store_id("watermarking_config", &fastly_token).await?;
    let api_keys_id = get_kv_store_id("api_keys", &fastly_token).await?;

    // Update specified keys
    for key in &keys_to_update {
        if let Some(value) = config.get(key) {
            match key.as_str() {
                "FMP4_AAC_PROFILE" | "FMP4_SAMPLE_RATE" | "FMP4_CHANNELS" | "FMP4_TRACK_ID" => {
                    populate_kv_store_entry(&watermarking_config_id, key, value, &fastly_token).await?;
                }
                "STEGAWAVE_API_KEY" => {
                    populate_kv_store_entry(&api_keys_id, "service_api_key", value, &fastly_token).await?;
                }
                _ => {
                    println!("Unknown configuration key: {}", key);
                }
            }
        }
    }

    println!("{}", style("✓ KV store values updated successfully.").green());
    Ok(())
}

async fn deploy(args: DeployArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", style("Deploying Fastly service...").bold());

    let fastly_token = args.fastly_token.unwrap_or_else(|| {
        Password::new()
            .with_prompt("Enter your Fastly API token")
            .interact()
            .unwrap()
    });

    if !args.skip_build {
        // Build the application
        println!("Building the Fastly application...");
        let mut build_command = Command::new("fastly");
        build_command.arg("compute").arg("build").arg("--verbose");
        
        let mut child = build_command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        
        if let Some(stdout) = child.stdout.take() {
            let reader = std::io::BufReader::new(stdout);
            for line in std::io::BufRead::lines(reader) {
                println!("{}", line?);
            }
        }

        if let Some(stderr) = child.stderr.take() {
            let reader = std::io::BufReader::new(stderr);
            for line in std::io::BufRead::lines(reader) {
                eprintln!("{}", line?);
            }
        }

        let build_status = child.wait()?;
        if !build_status.success() {
            println!("{}", style("Failed to build the application.").red());
            return Ok(());
        }
    }

    // Deploy the application
    let deploy_output = Command::new("fastly")
        .arg("compute")
        .arg("deploy")
        .env("FASTLY_API_TOKEN", fastly_token)
        .output()?;
    
    if !deploy_output.status.success() {
        println!("{}", style("Failed to deploy the application:").red());
        println!("{}", String::from_utf8_lossy(&deploy_output.stderr));
        return Ok(());
    }
    
    println!("{}", style("✓ Application deployed successfully.").green());
    Ok(())
}
