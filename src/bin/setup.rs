use clap::{Parser, Subcommand};
use dialoguer::{Input, Password};
use reqwest::Client;
use serde::Deserialize;
use std::process::{Command, Stdio};
use std::fs;
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
struct TailArgs {
    /// Your Fastly API token
    #[arg(long)]
    fastly_token: Option<String>,
}


#[derive(Deserialize, Debug)]
struct ApiSecret {
    secret: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install(args) => install(args).await?,
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
        let build_output = Command::new("fastly").arg("compute").arg("build").output()?;
        if !build_output.status.success() {
            println!("{}", style("Failed to build the application:").red());
            println!("{}", String::from_utf8_lossy(&build_output.stderr));
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
        let secrets_id = get_kv_store_id("secrets", &fastly_token).await?;
        let api_keys_id = get_kv_store_id("api_keys", &fastly_token).await?;
        let watermarking_config_id = get_kv_store_id("watermarking_config", &fastly_token).await?;

        populate_kv_store_entry(&secrets_id, "SECRET_KEY_HEX", &api_secret.secret, &fastly_token).await?;
        populate_kv_store_entry(&api_keys_id, "service_api_key", &stegawave_api_key, &fastly_token).await?;
        populate_kv_store_entry(&watermarking_config_id, "FMP4_AAC_PROFILE", "AAC-LC", &fastly_token).await?;
        populate_kv_store_entry(&watermarking_config_id, "FMP4_SAMPLE_RATE", "44100", &fastly_token).await?;
        populate_kv_store_entry(&watermarking_config_id, "FMP4_CHANNELS", "2", &fastly_token).await?;
        populate_kv_store_entry(&watermarking_config_id, "FMP4_TRACK_ID", "1", &fastly_token).await?;
        println!("{}", style("✓ KV stores populated.").green());


        println!("\n{}", style("Setup Complete!").bold().green());
        let service_domain_output = Command::new("fastly").arg("service").arg("describe").arg("--service-id").arg(service_id).output()?;
        let service_domain = String::from_utf8_lossy(&service_domain_output.stdout);
        let domain_line = service_domain.lines().find(|line| line.starts_with("Domain:")).unwrap_or("");
        let domain = domain_line.split_whitespace().last().unwrap_or("N/A");
        println!("Service Domain: {}", style(domain).cyan());


    } else {
        println!("{}", style("Failed to fetch master secret.").red());
        println!("Status: {}", res.status());
        let error_body = res.text().await?;
        println!("Response: {}", error_body);
    }


    Ok(())
}

async fn create_kv_store(name: &str, token: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("fastly")
        .arg("kv-store")
        .arg("create")
        .arg("--name")
        .arg(name)
        .env("FASTLY_API_TOKEN", token)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("already exists") {
            println!("Error creating KV store '{}': {}", name, stderr);
        }
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
    let json: Value = serde_json::from_slice(&output.stdout)?;
    Ok(json["id"].as_str().unwrap().to_string())
}


async fn populate_kv_store_entry(store_id: &str, key: &str, value: &str, token: &str) -> Result<(), Box<dyn std::error::Error>> {
    Command::new("fastly")
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
    Ok(())
}
