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

## Installation and Setup

This project includes an interactive setup script to configure and deploy the Fastly service.

### Prerequisites

- You must have the [Fastly CLI](https://developer.fastly.com/learning/tools/cli/#installation) installed and authenticated.

### Setup Steps

1.  **Run the setup script:**
    ```bash
    ./setup install
    ```
2.  **Provide Your Credentials:** The script will prompt you for the following information:
    *   **Fastly API Token**: Your Fastly API token with appropriate permissions to create services, backends, and KV stores.
    *   **StegaWave API Key**: Your API key for the StegaWave service.

The script will then automatically:
- Build and deploy the Rust application to Fastly Compute@Edge.
- Create the necessary KV Stores (`secrets`, `api_keys`, `watermarking_config`).
- Populate the KV stores with the required initial values.

Upon completion, it will display the domain for your newly deployed Fastly service.

## Usage

### 1. Get an Authentication Token

To play content through the Fastly service, your application backend must first obtain a JWT token from the StegaWave API. Make a GET request to:

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

## Tailing Logs

You can tail the logs for your deployed service to monitor requests and watermarking activity in real-time.

Run the following command and provide your Fastly API token when prompted:

```bash
./setup tail
```
