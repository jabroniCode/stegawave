use fastly::{
    kv_store::KVStore,
    error::Error,
    http::{header, Method, StatusCode},
    Request, Response,
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use rand::random;
use serde::{Deserialize, Serialize};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// The name of the backend for the primary origin server.
const PRIMARY_BACKEND: &str = "origin_1";

/// The name of the backend for the watermarking service.
const WATERMARKING_BACKEND: &str = "origin_2";

/// The names of the KV stores and Edge Dictionaries used for configuration.
const KV_STORE_SECRETS: &str = "secrets";  // KV store for secrets
const DICTIONARY_API_KEYS: &str = "api_keys";
const DICTIONARY_CONFIG: &str = "watermarking_config";

const WATERMARK_PROBABILITY: f64 = 0.01; // 1% chance to watermark

/// Defines the structure for JWT claims.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    #[serde(rename = "user_key")]
    session_id: String,
    exp: usize,
}

/// Main entry point for the Fastly Compute@Edge application.
#[fastly::main]
fn main(req: Request) -> Result<Response, Error> {
    // Set CORS headers for all responses, including pre-flight OPTIONS requests.
    if req.get_method() == Method::OPTIONS {
        return Ok(Response::from_status(StatusCode::OK)
            .with_header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .with_header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, POST, OPTIONS")
            .with_header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type, Authorization, X-API-Key"));
    }

    // Handle the main logic and then add CORS headers to the response.
    let mut resp = match handle_request(req) {
        Ok(res) => res,
        Err(e) => {
            println!("ERROR: Request handling failed: {}", e);
            Response::from_status(StatusCode::INTERNAL_SERVER_ERROR)
                .with_body_text_plain("An internal error occurred.\n")
        }
    };
    
    resp.set_header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");

    Ok(resp)
}

/// Derives a user-specific JWT secret from their API key and the master secret.
fn derive_jwt_secret(api_key: &str, master_secret: &[u8]) -> Result<Vec<u8>, Error> {
    let mut mac = HmacSha256::new_from_slice(master_secret)
        .map_err(|e| Error::msg(format!("Failed to create HMAC: {}", e)))?;
    
    let message = format!("jwt_secret:{}", api_key);
    mac.update(message.as_bytes());
    
    Ok(mac.finalize().into_bytes().to_vec())
}

/// Handles the main logic of the application: authentication, routing, and watermarking.
fn handle_request(mut req: Request) -> Result<Response, Error> {
    // --- Extract API Key ---
    let api_key = match req.get_header_str("X-API-Key") {
        Some(key) => key.to_string(),
        None => {
            return Ok(Response::from_status(StatusCode::UNAUTHORIZED)
                .with_body_text_plain("Missing API key.\n"));
        }
    };

    // --- JWT Verification ---
    // Token can be provided in 'Authorization: Bearer <token>' header or 'token' query param.
    let token_opt = req.get_header_str("Authorization")
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(str::to_string)
        .or_else(|| {
            req.get_url()
                .query_pairs()
                .find(|(key, _)| key == "token")
                .map(|(_, value)| value.into_owned())
        });

    let token = match token_opt {
        Some(t) => t,
        None => {
            return Ok(Response::from_status(StatusCode::UNAUTHORIZED)
                .with_body_text_plain("Missing authorization token.\n"));
        }
    };
    
    // Get the master secret key from the KV store
    let secrets_kv = KVStore::open(KV_STORE_SECRETS)?.expect("secrets KV store not found");
    let secret_key_hex = match secrets_kv.lookup("SECRET_KEY_HEX") {
        Ok(Some(entry)) => String::from_utf8_lossy(&entry.into_body_bytes()).to_string(),
        Ok(None) => {
            println!("SECRET_KEY_HEX not found in KV store");
            return Ok(Response::from_status(StatusCode::INTERNAL_SERVER_ERROR)
                .with_body_text_plain("Server configuration error.\n"));
        },
        Err(e) => {
            println!("Error accessing SECRET_KEY_HEX from KV store: {}", e);
            return Ok(Response::from_status(StatusCode::INTERNAL_SERVER_ERROR)
                .with_body_text_plain("Server configuration error.\n"));
        }
    };
    
    if secret_key_hex.trim().is_empty() {
        println!("SECRET_KEY_HEX is empty in KV store");
        return Ok(Response::from_status(StatusCode::INTERNAL_SERVER_ERROR)
            .with_body_text_plain("Server configuration error.\n"));
    }
    
    // Convert hex string to bytes
    let master_secret = match hex::decode(secret_key_hex.trim()) {
        Ok(bytes) => bytes,
        Err(e) => {
            println!("Failed to decode SECRET_KEY_HEX from hex: {}", e);
            return Ok(Response::from_status(StatusCode::INTERNAL_SERVER_ERROR)
                .with_body_text_plain("Server configuration error.\n"));
        }
    };
    
    // Derive the user-specific JWT secret
    let jwt_secret_bytes = derive_jwt_secret(&api_key, &master_secret)?;
    
    let decoding_key = DecodingKey::from_secret(&jwt_secret_bytes);

    let claims = match decode::<Claims>(&token, &decoding_key, &Validation::default()) {
        Ok(token_data) => {
            token_data.claims
        },
        Err(e) => {
            println!("JWT verification failed: {}", e);
            return Ok(Response::from_status(StatusCode::UNAUTHORIZED)
                .with_body_text_plain("Invalid JWT.\n"));
        }
    };
    
    // --- Routing Logic ---
    let path = req.get_path().to_string();

    // Serve manifest files directly from the primary origin.
    if path.ends_with(".m3u8") || path.ends_with(".mpd") || path.ends_with(".cmfv") {
        // Create a clean request without authentication headers for the origin
        let mut clean_req = Request::new(req.get_method().clone(), req.get_url().clone());
        let body = req.take_body_bytes();
        if !body.is_empty() {
            clean_req = clean_req.with_body(body);
        }
        return Ok(clean_req.send(PRIMARY_BACKEND)?);
    }

    // For segment requests, decide whether to watermark.
    let should_watermark = random::<f64>() > (1.0 - WATERMARK_PROBABILITY);

    if should_watermark {
        // Skip video segments - they're too large for Lambda processing
        if path.contains("/video/") {
            println!("WATERMARKING: Skipping video segment (too large): {}", path);
            let mut clean_req = Request::new(req.get_method().clone(), req.get_url().clone());
            let body = req.take_body_bytes();
            if !body.is_empty() {
                clean_req = clean_req.with_body(body);
            }
            return Ok(clean_req.send(PRIMARY_BACKEND)?);
        }

        // --- Watermarking Path ---
        println!("Watermarking segment: {}", path);

        // 1. Fetch the original segment from the primary origin.
        let mut clean_segment_req = Request::new(req.get_method().clone(), req.get_url().clone());
        let body = req.clone_with_body().take_body_bytes();
        if !body.is_empty() {
            clean_segment_req = clean_segment_req.with_body(body);
        }
        let original_segment_resp = clean_segment_req.send(PRIMARY_BACKEND)?;
        if !original_segment_resp.get_status().is_success() {
            println!("WATERMARKING: Failed to fetch original segment from primary backend.");
            return Ok(original_segment_resp); // Pass through error from origin
        }
        let segment_body = original_segment_resp.into_body();
        let segment_body_bytes = segment_body.into_bytes(); // Store original bytes for fallback

        // 2. Prepare a new request to the watermarking service.
        let mut watermark_url = req.get_url().clone();
        // Add user_key query parameter
        let mut query_pairs: Vec<(String, String)> = watermark_url.query_pairs().into_owned().collect();
        query_pairs.push(("user_key".to_string(), claims.session_id.clone()));
        let query_string = query_pairs.iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        watermark_url.set_query(Some(&query_string));
        
        println!("WATERMARKING: Sending segment to watermarking service for path: {}", path);
        println!("WATERMARKING: Request URL: {}", watermark_url);
        println!("WATERMARKING: Binary payload size: {} bytes", segment_body_bytes.len());
        println!("WATERMARKING: User key: {}", claims.session_id);
        
        // Send raw binary data instead of JSON with base64
        let mut watermark_req = Request::new(Method::POST, watermark_url)
            .with_body(segment_body_bytes.clone())
            .with_header("Content-Type", "application/octet-stream");
        
        // Add API key for watermarking service authentication
        let api_keys = KVStore::open(DICTIONARY_API_KEYS)?.expect("api_keys KVStore not found");
        let service_api_key = String::from_utf8_lossy(&api_keys.lookup("service_api_key")?.take_body_bytes()).to_string();
        
        // Debug: Print the exact API key for troubleshooting
        println!("WATERMARKING: API key (exact value): '{}'", service_api_key);
        println!("WATERMARKING: API key length: {} characters", service_api_key.len());
        
        // Check for any potential whitespace or special characters
        if service_api_key.contains(char::is_whitespace) {
            println!("WATERMARKING: WARNING - API key contains whitespace!");
        }
        
        if !service_api_key.is_empty() {
            println!("WATERMARKING: Adding API key to request: {}", &service_api_key[..std::cmp::min(service_api_key.len(), 30)]);
            // Trim any whitespace just in case
            watermark_req.set_header("X-API-Key", service_api_key.trim());
            
            // Add explicit Host header to ensure correct routing
            watermark_req.set_header("Host", "api.stegawave.com");
            println!("WATERMARKING: Added explicit Host header: api.stegawave.com");
        } else {
            println!("WATERMARKING: No API key found in dictionary");
        }
        
        // Add encoding configuration as headers to the watermarking request.
        let config = KVStore::open(DICTIONARY_CONFIG)?.expect("watermarking_config KVStore not found");
        watermark_req.set_header("FMP4_AAC_PROFILE", String::from_utf8_lossy(&config.lookup("FMP4_AAC_PROFILE")?.take_body_bytes()).to_string());
        watermark_req.set_header("FMP4_SAMPLE_RATE", String::from_utf8_lossy(&config.lookup("FMP4_SAMPLE_RATE")?.take_body_bytes()).to_string());
        watermark_req.set_header("FMP4_CHANNELS", String::from_utf8_lossy(&config.lookup("FMP4_CHANNELS")?.take_body_bytes()).to_string());
        watermark_req.set_header("FMP4_TRACK_ID", String::from_utf8_lossy(&config.lookup("FMP4_TRACK_ID")?.take_body_bytes()).to_string());

        // 3. Send the segment to the watermarking service.
        println!("WATERMARKING: Sending request to backend: {}", WATERMARKING_BACKEND);
        let mut watermarked_resp = match watermark_req.send(WATERMARKING_BACKEND) {
            Ok(resp) => resp,
            Err(e) => {
                println!("WATERMARKING: Failed to send request to backend: {}", e);
                println!("WATERMARKING: Falling back to original content due to backend error");
                return Ok(Response::from_status(StatusCode::OK)
                    .with_header("Content-Type", "video/mp4")
                    .with_body(segment_body_bytes));
            }
        };
        
        println!("WATERMARKING: Response status: {}", watermarked_resp.get_status());
        let headers: Vec<_> = watermarked_resp.get_headers().collect();
        println!("WATERMARKING: Response headers count: {}", headers.len());
        for (name, value) in &headers {
            println!("  Response header {}: {:?}", name, value);
        }
        
        // Check if watermarking was successful and response has content
        if watermarked_resp.get_status().is_success() {
            let response_body = watermarked_resp.clone_with_body().into_body_bytes();
            if response_body.is_empty() {
                println!("WATERMARKING: Service returned empty response, falling back to original content");
                // Return original unwatermarked content
                Ok(Response::from_status(StatusCode::OK)
                    .with_header("Content-Type", "video/mp4")
                    .with_body(segment_body_bytes))
            } else {
                println!("WATERMARKING: Service returned watermarked content ({} bytes)", response_body.len());
                // Return the watermarked response with original headers
                let mut response = Response::from_status(watermarked_resp.get_status())
                    .with_body(response_body);
                
                // Copy headers from the watermarked response
                for (name, value) in watermarked_resp.get_headers() {
                    response.set_header(name, value);
                }
                
                Ok(response)
            }
        } else {
            let response_body = watermarked_resp.clone_with_body().into_body_str();
            let status = watermarked_resp.get_status();
            println!("WATERMARKING: Error response status: {}", status);
            println!("WATERMARKING: Error response body: {}", response_body);
            
            // Provide specific guidance based on status code
            match status.as_u16() {
                403 => {
                    println!("WATERMARKING: 403 Forbidden - Check API Gateway configuration:");
                    println!("  - Verify API key is correct and active");
                    println!("  - Check API Gateway resource permissions");
                    println!("  - Verify binary media types are configured (application/octet-stream)");
                    println!("  - Check if request is hitting the correct endpoint");
                    println!("  - Verify EC2 instance is running and accessible");
                },
                413 => {
                    println!("WATERMARKING: 413 Payload Too Large - Request body too large for API Gateway");
                    println!("  - Consider reducing segment size or using direct upload");
                },
                502 | 503 | 504 => {
                    println!("WATERMARKING: {} - Backend service issue:", status);
                    println!("  - Check EC2 instance health");
                    println!("  - Verify service is running on correct port");
                    println!("  - Check API Gateway target group health");
                },
                _ => {
                    println!("WATERMARKING: Unexpected error status: {}", status);
                }
            }
            
            println!("WATERMARKING: Service failed, falling back to original content");
            // Return original unwatermarked content on error
            Ok(Response::from_status(StatusCode::OK)
                .with_header("Content-Type", "video/mp4")
                .with_body(segment_body_bytes))
        }
    } else {
        // --- Standard Path (No Watermarking) ---
        // Create a clean request without authentication headers for the origin
        let mut clean_req = Request::new(req.get_method().clone(), req.get_url().clone());
        let body = req.take_body_bytes();
        if !body.is_empty() {
            clean_req = clean_req.with_body(body);
        }
        Ok(clean_req.send(PRIMARY_BACKEND)?)
    }
}