//! Multimodal Content Processor
//!
//! Provides unified handling for multimodal content (text + images) across all
//! channels. Downloads images from various platforms, converts them to
//! LLM-compatible formats, and prepares messages for multimodal LLM
//! consumption.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::communication::{Message, PlatformType};
use crate::error::{AgentError, Result};

/// Image format for LLM consumption
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    /// PNG format
    Png,
    /// JPEG format
    Jpeg,
    /// GIF format
    Gif,
    /// WebP format
    Webp,
}

impl ImageFormat {
    /// Get MIME type for the format
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Webp => "image/webp",
        }
    }
}

/// Processed image ready for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedImage {
    /// Base64 encoded image data
    pub base64_data: String,
    /// Image format
    pub format: ImageFormat,
    /// MIME type
    pub mime_type: String,
    /// Original URL or key
    pub source: String,
    /// File size in bytes
    pub size_bytes: usize,
    /// Optional description
    pub description: Option<String>,
}

/// Multimodal content (text + images)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalContent {
    /// Text content
    pub text: String,
    /// Images attached to the message
    pub images: Vec<ProcessedImage>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Image download trait for different platforms
#[async_trait]
pub trait ImageDownloader: Send + Sync {
    /// Get the platform type this downloader supports
    fn platform(&self) -> PlatformType;

    /// Download image from platform-specific source
    ///
    /// # Arguments
    /// * `image_key` - Platform-specific image identifier
    /// * `access_token` - Optional access token for authenticated requests
    ///
    /// # Returns
    /// Raw image bytes
    async fn download_image(&self, image_key: &str, access_token: Option<&str>) -> Result<Vec<u8>>;

    /// Get image URL for direct access (if available)
    ///
    /// # Arguments
    /// * `image_key` - Platform-specific image identifier
    /// * `access_token` - Optional access token
    ///
    /// # Returns
    /// Direct URL to the image
    async fn get_image_url(&self, image_key: &str, access_token: Option<&str>) -> Result<String>;
}

/// Lark/Feishu image downloader
pub struct LarkImageDownloader {
    http_client: reqwest::Client,
    base_url: String,
}

impl LarkImageDownloader {
    /// Create a new Lark image downloader
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
            base_url: "https://open.feishu.cn".to_string(),
        }
    }

    /// Get access token for API calls (currently unused, kept for future use)
    #[allow(dead_code)]
    async fn get_access_token(&self, app_id: &str, app_secret: &str) -> Result<String> {
        let url = format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            self.base_url
        );

        let response = self
            .http_client
            .post(&url)
            .json(&serde_json::json!({
                "app_id": app_id,
                "app_secret": app_secret,
            }))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get access token: {}", e)))?;

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse token response: {}", e)))?;

        let token = data["tenant_access_token"]
            .as_str()
            .ok_or_else(|| AgentError::platform("Invalid token response"))?;

        Ok(token.to_string())
    }
}

#[async_trait]
impl ImageDownloader for LarkImageDownloader {
    fn platform(&self) -> PlatformType {
        PlatformType::Lark
    }

    async fn download_image(&self, image_key: &str, access_token: Option<&str>) -> Result<Vec<u8>> {
        let token = access_token
            .ok_or_else(|| AgentError::platform("Access token required for Lark image download"))?;

        let url = format!(
            "{}/open-apis/im/v1/images/{}?image_key={}",
            self.base_url, image_key, image_key
        );

        info!("Downloading Lark image: {}", image_key);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to download image: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Image download failed: HTTP {} - {}",
                status, error_text
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to read image bytes: {}", e)))?;

        info!("Downloaded image: {} bytes", bytes.len());
        Ok(bytes.to_vec())
    }

    async fn get_image_url(&self, _image_key: &str, _access_token: Option<&str>) -> Result<String> {
        // Lark doesn't provide direct URLs, we need to download
        Err(AgentError::platform(
            "Lark doesn't support direct image URLs",
        ))
    }
}

/// Generic image processor for all platforms
pub struct MultimodalProcessor {
    /// Registered image downloaders
    downloaders: HashMap<PlatformType, Arc<dyn ImageDownloader>>,
    /// Maximum image size in bytes (default: 10MB)
    max_image_size: usize,
    /// Supported image formats (currently unused, kept for future validation)
    #[allow(dead_code)]
    supported_formats: Vec<ImageFormat>,
}

impl MultimodalProcessor {
    /// Create a new multimodal processor
    pub fn new() -> Self {
        let mut downloaders: HashMap<PlatformType, Arc<dyn ImageDownloader>> = HashMap::new();

        // Register default downloaders
        downloaders.insert(
            PlatformType::Lark,
            Arc::new(LarkImageDownloader::new()) as Arc<dyn ImageDownloader>,
        );

        Self {
            downloaders,
            max_image_size: 10 * 1024 * 1024, // 10MB
            supported_formats: vec![
                ImageFormat::Png,
                ImageFormat::Jpeg,
                ImageFormat::Gif,
                ImageFormat::Webp,
            ],
        }
    }

    /// Register a custom image downloader
    pub fn register_downloader(&mut self, downloader: Arc<dyn ImageDownloader>) {
        let platform = downloader.platform();
        info!("Registering image downloader for platform: {:?}", platform);
        self.downloaders.insert(platform, downloader);
    }

    /// Process a message with a custom async download function
    ///
    /// This is useful when you have a platform-specific client (like
    /// LarkWebSocketClient) that can download images with its own
    /// authentication
    ///
    /// The download_fn receives (file_key, message_id) where message_id may be
    /// None
    pub async fn process_message_with_downloader<F, Fut>(
        &self,
        message: &Message,
        download_fn: F,
    ) -> Result<MultimodalContent>
    where
        F: Fn(&str, Option<&str>) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<Vec<u8>>> + Send,
    {
        let mut images = Vec::new();
        let mut metadata = HashMap::new();

        // Check if message contains image references in content
        let image_key = self
            .extract_image_key(&message.content)
            .or_else(|| message.metadata.get("image_url").cloned())
            .or_else(|| message.metadata.get("image_key").cloned());

        if let Some(image_key) = image_key {
            info!("Found image reference: {}", image_key);

            // Get message_id from metadata if available (needed for some platforms like
            // Lark)
            let message_id = message.metadata.get("message_id").map(|s| s.as_str());

            match self
                .process_image_bytes_with_message_id(&image_key, message_id, &download_fn)
                .await
            {
                Ok(image) => {
                    images.push(image);
                }
                Err(e) => {
                    warn!("Failed to process image {}: {}", image_key, e);
                    metadata.insert("image_error".to_string(), e.to_string());
                }
            }
        }

        // Clean text content (remove image metadata)
        let text = self.clean_text_content(&message.content);

        Ok(MultimodalContent {
            text,
            images,
            metadata,
        })
    }

    /// Process image bytes from a download function (legacy version without
    /// message_id)
    #[allow(dead_code)]
    async fn process_image_bytes<F, Fut>(
        &self,
        image_key: &str,
        download_fn: F,
    ) -> Result<ProcessedImage>
    where
        F: Fn(&str) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<Vec<u8>>> + Send,
    {
        // Download image bytes using the provided function
        let image_bytes = download_fn(image_key).await?;

        // Check size
        if image_bytes.len() > self.max_image_size {
            return Err(AgentError::platform(format!(
                "Image too large: {} bytes (max: {})",
                image_bytes.len(),
                self.max_image_size
            )));
        }

        // Detect format from magic bytes
        let format = self.detect_image_format(&image_bytes)?;

        // Encode to base64
        let base64_data = BASE64.encode(&image_bytes);

        Ok(ProcessedImage {
            base64_data,
            format,
            mime_type: format.mime_type().to_string(),
            source: image_key.to_string(),
            size_bytes: image_bytes.len(),
            description: None,
        })
    }

    /// Process image bytes from a download function with message_id support
    async fn process_image_bytes_with_message_id<F, Fut>(
        &self,
        image_key: &str,
        message_id: Option<&str>,
        download_fn: F,
    ) -> Result<ProcessedImage>
    where
        F: Fn(&str, Option<&str>) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<Vec<u8>>> + Send,
    {
        // Download image bytes using the provided function
        let image_bytes = download_fn(image_key, message_id).await?;

        // Check size
        if image_bytes.len() > self.max_image_size {
            return Err(AgentError::platform(format!(
                "Image too large: {} bytes (max: {})",
                image_bytes.len(),
                self.max_image_size
            )));
        }

        // Detect format from magic bytes
        let format = self.detect_image_format(&image_bytes)?;

        // Encode to base64
        let base64_data = BASE64.encode(&image_bytes);

        Ok(ProcessedImage {
            base64_data,
            format,
            mime_type: format.mime_type().to_string(),
            source: image_key.to_string(),
            size_bytes: image_bytes.len(),
            description: None,
        })
    }

    /// Process a message with potential image attachments
    ///
    /// # Arguments
    /// * `message` - The original message
    /// * `platform` - The platform type
    /// * `access_token` - Optional access token for authenticated downloads
    ///
    /// # Returns
    /// Multimodal content with processed images
    pub async fn process_message(
        &self,
        message: &Message,
        platform: PlatformType,
        access_token: Option<&str>,
    ) -> Result<MultimodalContent> {
        let mut images = Vec::new();
        let mut metadata = HashMap::new();

        // Check if message contains image references in content or metadata
        let image_key = self
            .extract_image_key(&message.content)
            .or_else(|| message.metadata.get("image_url").cloned())
            .or_else(|| message.metadata.get("image_key").cloned());

        if let Some(image_key) = image_key {
            info!("Found image reference: {}", image_key);

            match self
                .download_and_process_image(&image_key, platform, access_token)
                .await
            {
                Ok(image) => {
                    images.push(image);
                }
                Err(e) => {
                    warn!("Failed to process image {}: {}", image_key, e);
                    metadata.insert("image_error".to_string(), e.to_string());
                }
            }
        }

        // Clean text content (remove image metadata)
        let text = self.clean_text_content(&message.content);

        Ok(MultimodalContent {
            text,
            images,
            metadata,
        })
    }

    /// Extract image key from message content
    fn extract_image_key(&self, content: &str) -> Option<String> {
        // Check for image_key pattern: [图片] image_key: xxx
        if let Some(start) = content.find("image_key: ") {
            let start_idx = start + "image_key: ".len();
            let end_idx = content[start_idx..]
                .find(|c: char| c.is_whitespace())
                .map(|i| start_idx + i)
                .unwrap_or(content.len());
            return Some(content[start_idx..end_idx].to_string());
        }

        // Check for raw image_key (from older implementations)
        if content.starts_with("img_") {
            return Some(content.to_string());
        }

        None
    }

    /// Clean text content by removing image metadata
    fn clean_text_content(&self, content: &str) -> String {
        // Remove [图片] image_key: xxx pattern
        let cleaned = content.replace(|c: char| c == '\n' || c == '\r', " ");

        // If content is just an image reference, return placeholder
        if content.contains("image_key:") || content.starts_with("img_") {
            "[用户发送了一张图片]".to_string()
        } else {
            cleaned
        }
    }

    /// Download and process an image
    async fn download_and_process_image(
        &self,
        image_key: &str,
        platform: PlatformType,
        access_token: Option<&str>,
    ) -> Result<ProcessedImage> {
        let downloader = self.downloaders.get(&platform).ok_or_else(|| {
            AgentError::platform(format!("No image downloader for platform: {:?}", platform))
        })?;

        // Download image bytes
        let image_bytes = downloader.download_image(image_key, access_token).await?;

        // Check size
        if image_bytes.len() > self.max_image_size {
            return Err(AgentError::platform(format!(
                "Image too large: {} bytes (max: {})",
                image_bytes.len(),
                self.max_image_size
            )));
        }

        // Detect format from magic bytes
        let format = self.detect_image_format(&image_bytes)?;

        // Encode to base64
        let base64_data = BASE64.encode(&image_bytes);

        Ok(ProcessedImage {
            base64_data,
            format,
            mime_type: format.mime_type().to_string(),
            source: image_key.to_string(),
            size_bytes: image_bytes.len(),
            description: None,
        })
    }

    /// Detect image format from magic bytes
    fn detect_image_format(&self, bytes: &[u8]) -> Result<ImageFormat> {
        if bytes.len() < 8 {
            return Err(AgentError::platform("Image data too short"));
        }

        // PNG: 89 50 4E 47 0D 0A 1A 0A
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Ok(ImageFormat::Png);
        }

        // JPEG: FF D8 FF
        if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Ok(ImageFormat::Jpeg);
        }

        // GIF: GIF87a or GIF89a
        if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            return Ok(ImageFormat::Gif);
        }

        // WebP: RIFF....WEBP
        if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
            return Ok(ImageFormat::Webp);
        }

        // Default to JPEG if unknown
        warn!("Unknown image format, defaulting to JPEG");
        Ok(ImageFormat::Jpeg)
    }

    /// Convert multimodal content to OpenAI-compatible message format
    pub fn to_openai_format(&self, content: &MultimodalContent) -> Vec<serde_json::Value> {
        let mut parts = Vec::new();

        // Add text part
        if !content.text.is_empty() {
            parts.push(serde_json::json!({
                "type": "text",
                "text": content.text,
            }));
        }

        // Add image parts
        for image in &content.images {
            parts.push(serde_json::json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:{};base64,{}" , image.mime_type, image.base64_data),
                    "detail": "auto",
                },
            }));
        }

        parts
    }

    /// Convert multimodal content to Ollama-compatible format
    pub fn to_ollama_format(&self, content: &MultimodalContent) -> serde_json::Value {
        let mut message_parts = Vec::new();

        // Add text
        if !content.text.is_empty() {
            message_parts.push(content.text.clone());
        }

        // Add images as base64 strings
        for image in &content.images {
            message_parts.push(image.base64_data.clone());
        }

        // Ollama uses a simple array format for multimodal
        serde_json::json!(message_parts)
    }
}

impl Default for MultimodalProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_format_detection() {
        let processor = MultimodalProcessor::new();

        // PNG
        let png_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(
            processor.detect_image_format(&png_bytes).unwrap(),
            ImageFormat::Png
        );

        // JPEG (needs at least 8 bytes)
        let jpeg_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        assert_eq!(
            processor.detect_image_format(&jpeg_bytes).unwrap(),
            ImageFormat::Jpeg
        );

        // GIF (needs at least 8 bytes)
        let gif_bytes = b"GIF89a\x00\x00".to_vec();
        assert_eq!(
            processor.detect_image_format(&gif_bytes).unwrap(),
            ImageFormat::Gif
        );
    }

    #[test]
    fn test_extract_image_key() {
        let processor = MultimodalProcessor::new();

        let content = "[图片] image_key: img_abc123";
        assert_eq!(
            processor.extract_image_key(content),
            Some("img_abc123".to_string())
        );

        let content2 = "img_direct_key";
        assert_eq!(
            processor.extract_image_key(content2),
            Some("img_direct_key".to_string())
        );

        let content3 = "Just regular text";
        assert_eq!(processor.extract_image_key(content3), None);
    }
}
