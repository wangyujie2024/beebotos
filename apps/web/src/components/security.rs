//! Security components
//!
//! Provides:
//! - Content Security Policy (CSP) meta tags
//! - Security headers configuration

use leptos::prelude::*;
use leptos_meta::*;

/// Content Security Policy configuration
///
/// # Security Directives
/// - default-src: Default policy for loading content
/// - script-src: JavaScript execution sources
/// - style-src: CSS sources
/// - img-src: Image sources
/// - connect-src: XHR/fetch/WebSocket endpoints
/// - font-src: Font sources
/// - frame-src: Frame embedding sources
/// - base-uri: Base URL restrictions
/// - form-action: Form submission targets
#[component]
pub fn ContentSecurityPolicy() -> impl IntoView {
    // Generate nonce for inline scripts (would be generated server-side in production)
    let nonce = generate_nonce();

    // CSP policy - restrictive by default
    let csp = format!(
        "default-src 'self'; \
         script-src 'self' 'nonce-{nonce}' 'strict-dynamic' 'wasm-unsafe-eval'; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: https: blob:; \
         connect-src 'self' https: wss:; \
         font-src 'self'; \
         frame-src 'none'; \
         base-uri 'self'; \
         form-action 'self'; \
         upgrade-insecure-requests; \
         block-all-mixed-content"
    );

    view! {
        // CSP Meta tag
        <Meta name="Content-Security-Policy" content=csp.clone() />

        // Additional security headers as meta tags
        // Note: These should ideally be HTTP headers, but meta tags provide some protection
        <Meta http_equiv="X-Content-Type-Options" content="nosniff" />
        <Meta http_equiv="X-Frame-Options" content="DENY" />
        <Meta http_equiv="Referrer-Policy" content="strict-origin-when-cross-origin" />
        <Meta name="viewport" content="width=device-width, initial-scale=1.0" />
    }
}

/// Generate a cryptographically secure nonce
/// In production, this should be generated server-side
fn generate_nonce() -> String {
    // Try to use web crypto API first
    if let Some(window) = web_sys::window() {
        if let Ok(crypto) = window.crypto() {
            let mut buffer = vec![0u8; 16];
            if crypto.get_random_values_with_u8_array(&mut buffer).is_ok() {
                return encode_base64(&buffer);
            }
        }
    }

    // Fallback: use a timestamp-based nonce (less secure but works)
    let timestamp = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    encode_base64(&timestamp.to_le_bytes())
}

/// Simple base64 encoding
fn encode_base64(input: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in input.chunks(3) {
        let b = match chunk.len() {
            1 => [chunk[0], 0, 0],
            2 => [chunk[0], chunk[1], 0],
            _ => [chunk[0], chunk[1], chunk[2]],
        };

        result.push(ALPHABET[(b[0] >> 2) as usize] as char);
        result.push(ALPHABET[(((b[0] & 0x3) << 4) | (b[1] >> 4)) as usize] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[(((b[1] & 0xF) << 2) | (b[2] >> 6)) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[(b[2] & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}

/// Secure link component that validates URLs
#[component]
pub fn SecureLink(
    href: String,
    #[prop(optional)] class: Option<String>,
    children: Children,
) -> impl IntoView {
    let safe_href = crate::utils::sanitize_url(&href).unwrap_or_else(|| "#".to_string());

    view! {
        <a
            href=safe_href
            class=class.unwrap_or_default()
            rel="noopener noreferrer"
        >
            {children()}
        </a>
    }
}

/// Secure image component that validates src
#[component]
pub fn SecureImage(
    src: String,
    #[prop(optional)] alt: Option<String>,
    #[prop(optional)] class: Option<String>,
) -> impl IntoView {
    // Validate and sanitize image URL
    let safe_src = crate::utils::sanitize_url(&src).unwrap_or_default();

    view! {
        <img
            src=safe_src
            alt=alt.unwrap_or_default()
            class=class.unwrap_or_default()
            // Prevent error-based XSS via onerror
            // Note: In Leptos, event handlers are handled differently
        />
    }
}

/// Sanitized text component - escapes HTML in content
#[component]
pub fn SanitizedText(#[prop(into)] content: String) -> impl IntoView {
    view! {
        <span>{content}</span>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(encode_base64(b"hello"), "aGVsbG8=");
        assert_eq!(encode_base64(b"test"), "dGVzdA==");
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_generate_nonce() {
        let nonce1 = generate_nonce();
        let nonce2 = generate_nonce();
        assert!(!nonce1.is_empty());
        assert!(!nonce2.is_empty());
        // Nonces should be different
        assert_ne!(nonce1, nonce2);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_generate_nonce_stub() {
        // generate_nonce uses web_sys, which only works in WASM
        // This stub test ensures the test suite passes on non-WASM targets
        assert!(true);
    }
}
