# StegaWave Dynamic Audio Watermarking for Fastly Compute@Edge

This project is a Fastly Compute@Edge application that provides dynamic, session-based audio watermarking for fMP4 audio streams. It acts as a reverse proxy to your origin server, selectively watermarking a small percentage of audio segments before delivering them to the end-user.

The watermark is a 128-bit identifier derived from a `user_key` you provide, allowing you to trace content back to a specific user or session.

## How it Works

1.  **Client-Side Token Generation**: Your application backend makes a request to the StegaWave API to get a short-lived JWT token. This token contains a unique `user_key`.
2.  **Player Integration**: The client-side video player (e.g., THEOplayer, Bitmovin) is configured to append this JWT token as a query string parameter (`?token=...`) to every request it makes to the Fastly CDN.
3.  **Fastly Compute App**: The Fastly application intercepts each request.
    *   It validates the JWT token to authenticate the request.
    *   For manifest files (`.m3u8`, `.mpd`), it proxies the request directly to the origin.
    *   For fMP4 audio segments, it randomly selects about 1% of segments to be watermarked.
    *   If a segment is chosen, it is sent to the StegaWave watermarking service along with the `user_key` from the JWT. The service embeds the watermark and returns the modified segment.
    *   All other segments are passed through from the origin without modification.

This ensures that only a small, random subset of audio segments for each user contains a unique watermark, making it an efficient and secure way to protect your audio content.

## Architecture

The application consists of several components:

### Core Components

- **Fastly Compute@Edge Application**: The main Rust application that handles requests and routing
- **KV Stores**: Three key-value stores for configuration and secrets:
  - `secrets`: Stores the master secret key for JWT verification
  - `api_keys`: Stores the StegaWave API key for service authentication
  - `watermarking_config`: Stores audio encoding parameters (AAC profile, sample rate, channels, track ID)

### Setup Tool

The `setup-tool` is a comprehensive CLI application that manages:
- Initial deployment and configuration
- KV store management and updates
- Service redeployment
- Configuration file management

### Configuration System

- **CONFIG.txt**: Human-readable configuration file in the root directory
- **Interactive Setup**: Prompts for all necessary configuration during installation
- **Selective Updates**: Update specific configuration values without full redeployment
- **Persistent Storage**: Configuration values are saved to KV stores and can be updated independently

## Installation and Setup

This project includes an interactive setup script with a comprehensive configuration system to configure and deploy the Fastly service.

### Prerequisites

- You must have the [Fastly CLI](https://developer.fastly.com/learning/tools/cli/#installation) installed and authenticated.
- Rust toolchain (the setup script will check for this)

### Setup Steps

1.  **Run the setup script:**
    ```bash
    ./setup install
    ```

2.  **Provide Your Credentials:** The script will prompt you for:
    *   **Fastly API Token**: Your Fastly API token with appropriate permissions to create services, backends, and KV stores.
    *   **StegaWave API Key**: Your API key for the StegaWave service.

3.  **Configure Audio Encoding Parameters:** The script will prompt you to configure:
    *   **AAC Profile**: Audio encoding profile (default: AAC-LC)
    *   **Sample Rate**: Audio sample rate in Hz (default: 44100)
    *   **Number of Channels**: Audio channels (default: 2 for stereo)
    *   **Track ID**: Audio track identifier (default: 1)

The script will then automatically:
- Create and save your configuration to `CONFIG.txt`
- Build and deploy the Rust application to Fastly Compute@Edge
- Create the necessary KV Stores (`secrets`, `api_keys`, `watermarking_config`)
- Populate the KV stores with your configuration values

Upon completion, it will display the domain for your newly deployed Fastly service.

## Configuration Management

### CONFIG.txt File

After installation, you can edit the `CONFIG.txt` file in the root directory to modify configuration values:

```plaintext
# Audio Encoding Configuration
FMP4_AAC_PROFILE=AAC-LC
FMP4_SAMPLE_RATE=44100
FMP4_CHANNELS=2
FMP4_TRACK_ID=1

# Service Configuration
STEGAWAVE_API_KEY=your_api_key_here
FASTLY_API_TOKEN=your_token_here

# Advanced Configuration
WATERMARK_PROBABILITY=0.01
```

### Updating Configuration

After modifying `CONFIG.txt`, you can update the deployed service:

```bash
# Update all configuration values
./setup update

# Update specific configuration keys
./setup update --keys "FMP4_SAMPLE_RATE,FMP4_CHANNELS"
```

### Redeploying Code Changes

If you modify the Rust code, redeploy with:

```bash
# Build and deploy
./setup deploy

# Deploy without rebuilding (if binary is already built)
./setup deploy --skip-build
```

## Usage

### 1. Get an Authentication Token

To play content through the Fastly service, your application backend must first obtain a JWT token from the StegaWave API for each user.
Make a GET request to:

`https://api.stegawave.com/token?user_key=<user_key>`

- `user_key`: A unique identifier for the user or session (e.g., a session ID). This is the value that will be embedded as the watermark.

The API will return a JSON response containing the token:

```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzZXNzaW9uSUQiOiI1RUVEQzVDRTg5NDI2NUJDNTdERkM4NThCMTgzNzlBNiIsImV4cCI6MTc1MTU3NDQxNn0.f6PTxzk_DCMh3uevJ9OzXwvE_gpcm6sqYUeN97Dg8_k"
}
```

### 2. Client-Side Player Configuration

The retrieved token must be added as a query string parameter to all requests made by the video player to your Fastly service domain.

This repository includes two templates demonstrating how to do this with popular players:

-   `THEOplayer_template.html`: Shows how to use a `requestInterceptor` to add the `token` parameter to each request.
-   `Bitmovin_template.html`: Shows how to use the `preprocessHttpRequest` network configuration to achieve the same result.

You will need to replace the placeholder stream URLs and license keys in these templates with your own.

## Monitoring and Maintenance

### Tailing Logs

You can tail the logs for your deployed service to monitor requests and watermarking activity in real-time:

```bash
./setup tail
```

### Available Commands

The setup script supports several commands for managing your deployment:

- **`./setup install`**: Initial setup and deployment
- **`./setup update`**: Update KV store configuration values
- **`./setup deploy`**: Redeploy the service after code changes
- **`./setup tail`**: View real-time service logs

### Command Options

```bash
# Install with pre-provided credentials
./setup install --fastly-token "your_token" --stegawave-api-key "your_key"

# Update specific configuration keys
./setup update --keys "FMP4_SAMPLE_RATE,FMP4_CHANNELS"

# Deploy without rebuilding
./setup deploy --skip-build

# Tail logs with pre-provided token
./setup tail --fastly-token "your_token"
```
