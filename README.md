Fastly Dynamic Video Watermarking in Rust
This project is a Fastly Compute@Edge application written in Rust that provides dynamic, just-in-time watermarking for HLS and DASH video segments. It selectively watermarks a small percentage of segment requests, fetching the original segment from a primary origin and sending it to a separate watermarking service.

Features
API Key & JWT Verification: All incoming requests must have a valid API key (X-Api-Key header) and an HMAC-signed JWT (Authorization: Bearer <token>).

Dual Origins: Uses two backends—one for the primary content and one for the watermarking service.

Conditional Watermarking: Watermarks approximately 1% of video segment requests.

External Configuration: All secrets and configuration are stored in Fastly KV Stores, not hardcoded.

CORS Support: Automatically handles CORS pre-flight requests and adds necessary headers.

How It Works
Request Arrives: A client requests a video manifest or segment.

Authentication: The app verifies the Authorization: Bearer <token> header. The JWT secret is pulled from a KV Store.

Routing:

If the request is for a manifest (.m3u8, .mpd, etc.), it's served directly from the primary origin's cache.

If the request is for a segment, a random number is generated.

Watermarking Logic:

99% of the time: The segment is served directly from the primary origin's cache.

1% of the time: The segment is fetched from the primary origin, sent to the watermarking service, and the watermarked version is returned to the client. The watermarking service receives encoding parameters as request headers.

Prerequisites
Rust and Cargo: The setup script will check for this, but you can install it from https://www.rust-lang.org/tools/install

Fastly CLI: The setup script will check for this, but you can install it from https://developer.fastly.com/learning/tools/cli/

An active Fastly account.

Setup and Deployment
This project uses a simple setup script to get you started. It will guide you through the process of deploying the Fastly Compute@Edge service.

For Linux and macOS:
```bash
chmod +x setup
./setup install
```

For Windows:
```bash
.\setup.bat install
```

The script will:

Build the Rust application.

Prompt you for your Fastly API Token and your StegaWave API Key.

Deploy the application to Fastly.

Create and populate the required KV Stores.

Your service will then be live and ready to handle requests.

Logging
To view the logs for your service, you can use the `tail` command:

For Linux and macOS:
```bash
./setup tail
```

For Windows:
```bash
.\setup.bat tail
```
