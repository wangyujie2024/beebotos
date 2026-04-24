//! Inference capability commands
//!
//! AI capabilities: text, image, audio, video, web, embedding

// io and Write not used - removed
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use futures::StreamExt;

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct InferArgs {
    #[command(subcommand)]
    pub command: InferCommand,
}

#[derive(Subcommand)]
pub enum InferCommand {
    /// List available capabilities
    List {
        /// Filter by category
        #[arg(short, long)]
        category: Option<String>,
        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Inspect a specific capability
    Inspect {
        /// Capability ID
        id: String,
    },

    /// Text inference/completion
    #[command(subcommand)]
    Text(TextCommand),

    /// Image generation and editing
    #[command(subcommand)]
    Image(ImageCommand),

    /// Audio transcription and generation
    #[command(subcommand)]
    Audio(AudioCommand),

    /// Text-to-speech
    #[command(subcommand)]
    Tts(TtsCommand),

    /// Video generation and analysis
    #[command(subcommand)]
    Video(VideoCommand),

    /// Web search and fetch
    #[command(subcommand)]
    Web(WebCommand),

    /// Embedding operations
    #[command(subcommand)]
    Embedding(EmbeddingCommand),

    /// Code assistance
    #[command(subcommand)]
    Code(CodeCommand),

    /// Multi-modal operations
    #[command(subcommand)]
    Multimodal(MultimodalCommand),
}

#[derive(Subcommand)]
pub enum TextCommand {
    /// Run a text completion
    Run {
        /// Input prompt
        prompt: String,
        /// Model to use
        #[arg(short, long)]
        model: Option<String>,
        /// Max tokens
        #[arg(long, default_value = "1000")]
        max_tokens: u32,
        /// Temperature
        #[arg(short, long)]
        temperature: Option<f32>,
        /// Stream output
        #[arg(long)]
        stream: bool,
    },
    /// Summarize text
    Summarize {
        /// Input text or file path
        input: String,
        /// Summary length
        #[arg(short, long, value_enum, default_value = "medium")]
        length: SummaryLength,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Analyze sentiment
    Sentiment {
        /// Input text
        text: String,
    },
    /// Extract entities
    Entities {
        /// Input text
        text: String,
        /// Entity types
        #[arg(short, long)]
        types: Vec<String>,
    },
    /// Classify text
    Classify {
        /// Input text
        text: String,
        /// Categories
        #[arg(short, long, required = true)]
        categories: Vec<String>,
    },
    /// Translate text
    Translate {
        /// Input text
        text: String,
        /// Target language
        #[arg(short, long)]
        to: String,
        /// Source language (auto-detect if not specified)
        #[arg(short = 's', long)]
        from: Option<String>,
    },
    /// Answer questions
    Ask {
        /// Question
        question: String,
        /// Context document
        #[arg(short, long)]
        context: Option<PathBuf>,
        /// Search web for context
        #[arg(long)]
        web_search: bool,
    },
}

#[derive(Subcommand)]
pub enum ImageCommand {
    /// Generate an image from prompt
    Generate {
        /// Prompt
        prompt: String,
        /// Output file
        #[arg(short, long, default_value = "generated.png")]
        output: PathBuf,
        /// Image size
        #[arg(short, long, value_enum, default_value = "1024x1024")]
        size: ImageSize,
        /// Number of images
        #[arg(short, long, default_value = "1")]
        n: u32,
        /// Model
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Edit an image
    Edit {
        /// Image file
        image: PathBuf,
        /// Edit prompt
        prompt: String,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Mask file (optional)
        #[arg(long)]
        mask: Option<PathBuf>,
    },
    /// Create image variation
    Vary {
        /// Image file
        image: PathBuf,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Number of variations
        #[arg(short, long, default_value = "1")]
        n: u32,
    },
    /// Describe/analyze image
    Describe {
        /// Image file or URL
        image: String,
        /// Detail level
        #[arg(short, long, value_enum, default_value = "auto")]
        detail: DetailLevel,
        /// Specific question about image
        #[arg(short, long)]
        question: Option<String>,
    },
    /// Extract text from image (OCR)
    Ocr {
        /// Image file
        image: PathBuf,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum AudioCommand {
    /// Transcribe audio to text
    Transcribe {
        /// Audio file
        file: PathBuf,
        /// Language hint
        #[arg(short, long)]
        language: Option<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: TranscriptFormat,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Enable timestamps
        #[arg(long)]
        timestamps: bool,
    },
    /// Translate audio to English
    Translate {
        /// Audio file
        file: PathBuf,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate audio (sound effects)
    Generate {
        /// Description of sound
        prompt: String,
        /// Output file
        #[arg(short, long, default_value = "generated.wav")]
        output: PathBuf,
        /// Duration in seconds
        #[arg(short, long)]
        duration: Option<f32>,
    },
}

#[derive(Subcommand)]
pub enum TtsCommand {
    /// Convert text to speech
    Convert {
        /// Input text or file path
        input: String,
        /// Output file
        #[arg(short, long, default_value = "output.mp3")]
        output: PathBuf,
        /// Voice ID
        #[arg(short = 'i', long)]
        voice: Option<String>,
        /// Speed (0.5 - 2.0)
        #[arg(long)]
        speed: Option<f32>,
    },
    /// List available voices
    Voices {
        /// Filter by language
        #[arg(short, long)]
        language: Option<String>,
    },
    /// Preview a voice
    Preview {
        /// Voice ID
        voice: String,
        /// Preview text
        #[arg(short, long, default_value = "Hello, this is a voice preview.")]
        text: String,
    },
}

#[derive(Subcommand)]
pub enum VideoCommand {
    /// Generate video from prompt
    Generate {
        /// Prompt
        prompt: String,
        /// Output file
        #[arg(short, long, default_value = "generated.mp4")]
        output: PathBuf,
        /// Duration in seconds
        #[arg(short, long, default_value = "5")]
        duration: u32,
        /// Resolution
        #[arg(long, value_enum, default_value = "720p")]
        resolution: VideoResolution,
    },
    /// Extend video
    Extend {
        /// Video file
        video: PathBuf,
        /// Extension prompt
        prompt: String,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Describe video content
    Describe {
        /// Video file or URL
        video: String,
        /// Sample frames
        #[arg(long, default_value = "5")]
        frames: u32,
    },
    /// Extract frames from video
    Frames {
        /// Video file
        video: PathBuf,
        /// Output directory
        #[arg(short, long, default_value = "frames")]
        output: PathBuf,
        /// Frame interval in seconds
        #[arg(short, long, default_value = "1")]
        interval: f32,
    },
}

#[derive(Subcommand)]
pub enum WebCommand {
    /// Search the web
    Search {
        /// Search query
        query: String,
        /// Number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,
        /// Include full content
        #[arg(long)]
        full_content: bool,
        /// Site filter
        #[arg(long)]
        site: Option<String>,
    },
    /// Fetch a webpage
    Fetch {
        /// URL
        url: String,
        /// Extract main content only
        #[arg(long)]
        extract: bool,
        /// Output format
        #[arg(short, long, value_enum, default_value = "text")]
        format: FetchFormat,
    },
    /// Crawl a website
    Crawl {
        /// Starting URL
        url: String,
        /// Max pages
        #[arg(short, long, default_value = "10")]
        max_pages: usize,
        /// Stay within domain
        #[arg(long)]
        same_domain: bool,
        /// Output directory
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum EmbeddingCommand {
    /// Create embeddings for text
    Create {
        /// Input text or file
        input: String,
        /// Model
        #[arg(short, long)]
        model: Option<String>,
        /// Batch size (for files)
        #[arg(long, default_value = "100")]
        batch_size: usize,
    },
    /// Calculate similarity between two texts
    Similarity {
        /// First text
        text1: String,
        /// Second text
        text2: String,
        /// Model
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Cluster texts
    Cluster {
        /// Input file (one text per line)
        file: PathBuf,
        /// Number of clusters
        #[arg(short, long, default_value = "5")]
        clusters: usize,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum CodeCommand {
    /// Complete code
    Complete {
        /// Code snippet or file
        input: String,
        /// Language
        #[arg(short, long)]
        language: Option<String>,
        /// Context file
        #[arg(short, long)]
        context: Vec<PathBuf>,
    },
    /// Explain code
    Explain {
        /// Code file or snippet
        input: String,
        /// Detail level
        #[arg(short, long, value_enum, default_value = "medium")]
        detail: DetailLevel,
    },
    /// Review code
    Review {
        /// Code file
        file: PathBuf,
        /// Focus areas
        #[arg(short = 'F', long)]
        focus: Vec<String>,
    },
    /// Fix code issues
    Fix {
        /// Code file
        file: PathBuf,
        /// Issue description
        #[arg(short, long)]
        issue: Option<String>,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate tests
    Test {
        /// Code file
        file: PathBuf,
        /// Test framework
        #[arg(short = 'w', long)]
        framework: Option<String>,
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate documentation
    Doc {
        /// Code file
        file: PathBuf,
        /// Output format
        #[arg(short, long, value_enum, default_value = "markdown")]
        format: DocFormat,
    },
}

#[derive(Subcommand)]
pub enum MultimodalCommand {
    /// Chat with vision (image + text)
    Chat {
        /// Image file(s)
        #[arg(short, long)]
        image: Vec<PathBuf>,
        /// Text prompt
        prompt: String,
        /// Model
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Analyze document
    Document {
        /// Document file
        file: PathBuf,
        /// Questions to ask
        #[arg(short, long)]
        question: Vec<String>,
        /// Extract structured data
        #[arg(long)]
        structured: bool,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum SummaryLength {
    Short,
    Medium,
    Long,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ImageSize {
    #[value(name = "256x256")]
    _256x256,
    #[value(name = "512x512")]
    _512x512,
    #[value(name = "1024x1024")]
    _1024x1024,
    #[value(name = "1792x1024")]
    _1792x1024,
    #[value(name = "1024x1792")]
    _1024x1792,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum DetailLevel {
    Low,
    Auto,
    High,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum TranscriptFormat {
    Text,
    Json,
    Srt,
    Vtt,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum VideoResolution {
    #[value(name = "480p")]
    _480p,
    #[value(name = "720p")]
    _720p,
    #[value(name = "1080p")]
    _1080p,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum FetchFormat {
    Text,
    Markdown,
    Html,
    Json,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum DocFormat {
    Markdown,
    Html,
    Plain,
}

pub async fn execute(args: InferArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        InferCommand::List { category, verbose } => {
            let progress = TaskProgress::new("Fetching capabilities");
            let caps = client.list_capabilities(category.as_deref()).await?;
            progress.finish_success(Some(&format!("{} capabilities", caps.len())));

            if verbose {
                println!("{:<25} {:<15} {:<20} Status", "ID", "Category", "Name");
                println!("{}", "-".repeat(90));
                for cap in caps {
                    let status_icon = if cap.available { "✓" } else { "✗" };
                    println!(
                        "{:<25} {:<15} {:<20} {}",
                        cap.id, cap.category, cap.name, status_icon
                    );
                }
            } else {
                println!("{:<25} {:<15} Status", "ID", "Category");
                println!("{}", "-".repeat(50));
                for cap in caps {
                    let status_icon = if cap.available { "✓" } else { "✗" };
                    println!("{:<25} {:<15} {}", cap.id, cap.category, status_icon);
                }
            }
        }

        InferCommand::Inspect { id } => {
            let progress = TaskProgress::new("Inspecting capability");
            let cap = client.get_capability(&id).await?;
            progress.finish_success(None);

            println!("🔍 Capability: {}", cap.name);
            println!("============================");
            println!("ID: {}", cap.id);
            println!("Category: {}", cap.category);
            println!("Description: {}", cap.description);
            println!(
                "Status: {}",
                if cap.available {
                    "Available ✓"
                } else {
                    "Unavailable ✗"
                }
            );

            if !cap.models.is_empty() {
                println!("\nSupported Models:");
                for model in cap.models {
                    println!("  • {}", model);
                }
            }

            if let Some(pricing) = cap.pricing {
                println!("\nPricing: ${:.4} per request", pricing);
            }
        }

        InferCommand::Text(cmd) => match cmd {
            TextCommand::Run {
                prompt,
                model,
                max_tokens,
                temperature,
                stream,
            } => {
                let req = TextRequest {
                    prompt,
                    model_id: model,
                    max_tokens,
                    temperature,
                };

                if stream {
                    let mut stream = client.stream_text(&req).await?;
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(text) => print!("{}", text),
                            Err(e) => eprintln!("\nError: {}", e),
                        }
                    }
                    println!();
                } else {
                    let progress = TaskProgress::new("Generating text");
                    let response = client.generate_text(&req).await?;
                    progress.finish_success(Some(&format!("{} tokens", response.tokens)));
                    println!("{}", response.text);
                }
            }
            TextCommand::Summarize {
                input,
                length,
                output,
            } => {
                let text = if std::path::Path::new(&input).exists() {
                    std::fs::read_to_string(&input)?
                } else {
                    input
                };

                let progress = TaskProgress::new("Summarizing");
                let summary = client
                    .summarize(&text, &format!("{:?}", length).to_lowercase())
                    .await?;
                progress.finish_success(None);

                if let Some(path) = output {
                    std::fs::write(&path, &summary)?;
                    println!("✅ Summary saved to {}", path.display());
                } else {
                    println!("📋 Summary:\n{}", summary);
                }
            }
            TextCommand::Sentiment { text } => {
                let progress = TaskProgress::new("Analyzing sentiment");
                let result = client.analyze_sentiment(&text).await?;
                progress.finish_success(None);

                println!("📊 Sentiment Analysis:");
                println!("  Overall: {} ({:.2})", result.label, result.score);
                println!("  Confidence: {:.1}%", result.confidence * 100.0);
                if let Some(breakdown) = result.breakdown {
                    println!("\n  Breakdown:");
                    for (emotion, score) in breakdown {
                        println!("    {}: {:.2}", emotion, score);
                    }
                }
            }
            TextCommand::Entities { text, types } => {
                let progress = TaskProgress::new("Extracting entities");
                let entities = client.extract_entities(&text, &types).await?;
                progress.finish_success(Some(&format!("{} entities", entities.len())));

                println!("🔍 Entities found:");
                for entity in entities {
                    println!(
                        "  {} [{}]: {}",
                        entity.text, entity.entity_type, entity.confidence
                    );
                }
            }
            TextCommand::Classify { text, categories } => {
                let progress = TaskProgress::new("Classifying");
                let result = client.classify_text(&text, &categories).await?;
                progress.finish_success(None);

                println!("📊 Classification Results:");
                for (category, score) in result {
                    let bar = "█".repeat((score * 30.0) as usize);
                    println!("  {:20} [{:<30}] {:.1}%", category, bar, score * 100.0);
                }
            }
            TextCommand::Translate { text, to, from } => {
                let progress = TaskProgress::new("Translating");
                let translated = client.translate(&text, &to, from.as_deref()).await?;
                progress.finish_success(None);
                println!("🌐 Translation:");
                println!(
                    "  {} -> {}",
                    translated.source_language, translated.target_language
                );
                println!("\n{}", translated.text);
            }
            TextCommand::Ask {
                question,
                context,
                web_search,
            } => {
                let ctx = if let Some(path) = context {
                    Some(std::fs::read_to_string(&path)?)
                } else {
                    None
                };

                let progress = TaskProgress::new("Finding answer");
                let answer = client
                    .answer_question(&question, ctx.as_deref(), web_search)
                    .await?;
                progress.finish_success(None);

                println!("❓ {}", question);
                println!("\n💡 {}", answer.text);

                if let Some(sources) = answer.sources {
                    println!("\n📚 Sources:");
                    for source in sources {
                        println!("  • {}", source);
                    }
                }
            }
        },

        InferCommand::Image(cmd) => match cmd {
            ImageCommand::Generate {
                prompt,
                output,
                size,
                n,
                model,
            } => {
                let progress = TaskProgress::new("Generating image");
                let req = ImageGenRequest {
                    prompt,
                    size: format!("{:?}", size).replace("_", ""),
                    n,
                    model_id: model,
                };
                let result = client.generate_image(&req).await?;
                progress.finish_success(None);

                // Save first image
                if let Some(image) = result.images.first() {
                    std::fs::write(&output, &image.data)?;
                    println!("✅ Image saved to {}", output.display());
                    if let Some(revised_prompt) = &image.revised_prompt {
                        println!("\n📝 Revised prompt: {}", revised_prompt);
                    }
                }
            }
            ImageCommand::Edit {
                image,
                prompt,
                output,
                mask,
            } => {
                let progress = TaskProgress::new("Editing image");
                client
                    .edit_image(&image, &prompt, mask.as_ref(), output.as_ref())
                    .await?;
                progress.finish_success(None);

                let out_path = output.unwrap_or_else(|| PathBuf::from("edited.png"));
                println!("✅ Edited image saved to {}", out_path.display());
            }
            ImageCommand::Vary { image, output, n } => {
                let progress = TaskProgress::new("Creating variations");
                let result = client
                    .create_image_variations(&image, n, output.as_ref())
                    .await?;
                progress.finish_success(None);
                println!("✅ Created {} variations", result.len());
            }
            ImageCommand::Describe {
                image,
                detail,
                question,
            } => {
                let progress = TaskProgress::new("Analyzing image");
                let analysis = client
                    .describe_image(
                        &image,
                        &format!("{:?}", detail).to_lowercase(),
                        question.as_deref(),
                    )
                    .await?;
                progress.finish_success(None);

                println!("🖼️  Image Analysis:\n{}", analysis.description);

                if !analysis.objects.is_empty() {
                    println!("\n📦 Objects detected:");
                    for obj in analysis.objects {
                        println!("  • {}", obj);
                    }
                }
            }
            ImageCommand::Ocr { image, output } => {
                let progress = TaskProgress::new("Extracting text");
                let text = client.ocr_image(&image).await?;
                progress.finish_success(None);

                if let Some(path) = output {
                    std::fs::write(&path, &text)?;
                    println!("✅ Text saved to {}", path.display());
                } else {
                    println!("📝 Extracted Text:\n{}", text);
                }
            }
        },

        InferCommand::Audio(cmd) => match cmd {
            AudioCommand::Transcribe {
                file,
                language,
                format,
                output,
                timestamps,
            } => {
                let progress = TaskProgress::new("Transcribing audio");
                let result = client
                    .transcribe_audio(&file, language.as_deref(), timestamps)
                    .await?;
                progress.finish_success(None);

                let output_text = match format {
                    TranscriptFormat::Text => result.text,
                    TranscriptFormat::Json => serde_json::to_string_pretty(&result)?,
                    TranscriptFormat::Srt => result.to_srt(),
                    TranscriptFormat::Vtt => result.to_vtt(),
                };

                if let Some(path) = output {
                    std::fs::write(&path, &output_text)?;
                    println!("✅ Transcription saved to {}", path.display());
                } else {
                    println!("📝 Transcription:\n{}", output_text);
                }
            }
            AudioCommand::Translate { file, output } => {
                let progress = TaskProgress::new("Translating audio");
                let result = client.translate_audio(&file).await?;
                progress.finish_success(None);

                if let Some(path) = output {
                    std::fs::write(&path, &result.text)?;
                    println!("✅ Translation saved to {}", path.display());
                } else {
                    println!("📝 Translation:\n{}", result.text);
                }
            }
            AudioCommand::Generate {
                prompt,
                output,
                duration,
            } => {
                let progress = TaskProgress::new("Generating audio");
                let audio = client.generate_audio(&prompt, duration).await?;
                progress.finish_success(None);

                std::fs::write(&output, &audio)?;
                println!("✅ Audio saved to {}", output.display());
            }
        },

        InferCommand::Tts(cmd) => match cmd {
            TtsCommand::Convert {
                input,
                output,
                voice,
                speed,
            } => {
                let text = if std::path::Path::new(&input).exists() {
                    std::fs::read_to_string(&input)?
                } else {
                    input
                };

                let progress = TaskProgress::new("Converting to speech");
                let audio = client
                    .text_to_speech(&text, voice.as_deref(), speed)
                    .await?;
                progress.finish_success(None);

                std::fs::write(&output, &audio)?;
                println!("✅ Audio saved to {}", output.display());
            }
            TtsCommand::Voices { language } => {
                let voices = client.list_voices(language.as_deref()).await?;
                println!("🎙️  Available Voices:");
                println!("{:<20} {:<15} {:<10} Language", "ID", "Name", "Gender");
                println!("{}", "-".repeat(70));
                for voice in voices {
                    println!(
                        "{:<20} {:<15} {:<10} {}",
                        voice.id, voice.name, voice.gender, voice.language
                    );
                }
            }
            TtsCommand::Preview { voice, text } => {
                let progress = TaskProgress::new("Generating preview");
                let audio = client.text_to_speech(&text, Some(&voice), None).await?;
                progress.finish_success(None);

                let temp_path = std::env::temp_dir().join(format!("tts_preview_{}.mp3", voice));
                std::fs::write(&temp_path, &audio)?;
                println!("✅ Preview saved to: {}", temp_path.display());

                // Try to play automatically
                #[cfg(target_os = "macos")]
                std::process::Command::new("afplay")
                    .arg(&temp_path)
                    .spawn()?;
                #[cfg(target_os = "linux")]
                std::process::Command::new("aplay")
                    .arg(&temp_path)
                    .spawn()?;
            }
        },

        InferCommand::Web(cmd) => match cmd {
            WebCommand::Search {
                query,
                limit,
                full_content,
                site,
            } => {
                let progress = TaskProgress::new("Searching web");
                let results = client.web_search(&query, limit, site.as_deref()).await?;
                progress.finish_success(Some(&format!("{} results", results.len())));

                for (i, result) in results.iter().enumerate() {
                    println!("\n{}. {}", i + 1, result.title);
                    println!("   URL: {}", result.url);
                    println!("   {}", result.snippet);

                    if full_content {
                        if let Some(content) = &result.full_content {
                            println!("\n   Full content:\n{}", content);
                        }
                    }
                }
            }
            WebCommand::Fetch {
                url,
                extract,
                format,
            } => {
                let progress = TaskProgress::new("Fetching webpage");
                let content = client
                    .fetch_webpage(&url, extract, &format!("{:?}", format).to_lowercase())
                    .await?;
                progress.finish_success(None);
                println!("{}", content);
            }
            WebCommand::Crawl {
                url,
                max_pages,
                same_domain,
                output,
            } => {
                let progress = TaskProgress::new("Crawling website");
                let pages = client.crawl_website(&url, max_pages, same_domain).await?;
                progress.finish_success(Some(&format!("{} pages", pages.len())));

                if let Some(dir) = output {
                    std::fs::create_dir_all(&dir)?;
                    for (i, page) in pages.iter().enumerate() {
                        let filename = format!("page_{:03}.md", i + 1);
                        std::fs::write(dir.join(&filename), &page.content)?;
                    }
                    println!("✅ Saved {} pages to {}", pages.len(), dir.display());
                } else {
                    for page in pages {
                        println!("\n--- {} ---\n{}", page.url, page.content);
                    }
                }
            }
        },

        InferCommand::Embedding(cmd) => match cmd {
            EmbeddingCommand::Create {
                input,
                model,
                batch_size,
            } => {
                let progress = TaskProgress::new("Creating embeddings");

                let texts = if std::path::Path::new(&input).exists() {
                    std::fs::read_to_string(&input)?
                        .lines()
                        .map(|s| s.to_string())
                        .collect()
                } else {
                    vec![input]
                };

                let embeddings = client
                    .create_embeddings(&texts, model.as_deref(), batch_size)
                    .await?;
                progress.finish_success(Some(&format!("{} embeddings", embeddings.len())));

                for (i, emb) in embeddings.iter().enumerate() {
                    println!("Embedding {}: {} dimensions", i + 1, emb.len());
                }
            }
            EmbeddingCommand::Similarity {
                text1,
                text2,
                model,
            } => {
                let progress = TaskProgress::new("Calculating similarity");
                let similarity = client
                    .calculate_similarity(&text1, &text2, model.as_deref())
                    .await?;
                progress.finish_success(None);

                println!("📊 Similarity: {:.2}%", similarity * 100.0);
                let description = if similarity > 0.9 {
                    "Very similar"
                } else if similarity > 0.7 {
                    "Similar"
                } else if similarity > 0.4 {
                    "Somewhat similar"
                } else {
                    "Dissimilar"
                };
                println!("Interpretation: {}", description);
            }
            EmbeddingCommand::Cluster {
                file,
                clusters,
                output,
            } => {
                let progress = TaskProgress::new("Clustering texts");
                let texts: Vec<String> = std::fs::read_to_string(&file)?
                    .lines()
                    .map(|s| s.to_string())
                    .collect();

                let cluster_results = client.cluster_texts(&texts, clusters).await?;
                progress.finish_success(None);

                println!("🎯 Clusters found:");
                for (i, cluster) in cluster_results.iter().enumerate() {
                    println!("\nCluster {} ({} items):", i + 1, cluster.items.len());
                    println!("  Keywords: {}", cluster.keywords.join(", "));
                    for item in &cluster.items {
                        println!("    - {}", item);
                    }
                }

                if let Some(path) = output {
                    std::fs::write(&path, serde_json::to_string_pretty(&cluster_results)?)?;
                    println!("\n✅ Results saved to {}", path.display());
                }
            }
        },

        InferCommand::Code(cmd) => match cmd {
            CodeCommand::Complete {
                input,
                language,
                context,
            } => {
                let code = if std::path::Path::new(&input).exists() {
                    std::fs::read_to_string(&input)?
                } else {
                    input
                };

                let ctx: Vec<String> = context
                    .iter()
                    .filter_map(|p| std::fs::read_to_string(p).ok())
                    .collect();

                let progress = TaskProgress::new("Completing code");
                let completion = client
                    .complete_code(&code, language.as_deref(), &ctx)
                    .await?;
                progress.finish_success(None);

                println!("💻 Code Completion:\n```\n{}\n```", completion);
            }
            CodeCommand::Explain { input, detail } => {
                let code = if std::path::Path::new(&input).exists() {
                    std::fs::read_to_string(&input)?
                } else {
                    input
                };

                let progress = TaskProgress::new("Explaining code");
                let explanation = client
                    .explain_code(&code, &format!("{:?}", detail).to_lowercase())
                    .await?;
                progress.finish_success(None);

                println!("📖 Code Explanation:\n{}", explanation);
            }
            CodeCommand::Review { file, focus } => {
                let code = std::fs::read_to_string(&file)?;
                let progress = TaskProgress::new("Reviewing code");
                let review = client.review_code(&code, &focus).await?;
                progress.finish_success(Some(&format!("{} issues", review.issues.len())));

                println!("🔍 Code Review for: {}", file.display());
                println!("Overall: {}\n", review.summary);

                for issue in review.issues {
                    let icon = match issue.severity.as_str() {
                        "error" => "❌",
                        "warning" => "⚠️",
                        _ => "ℹ️",
                    };
                    println!(
                        "{} [{}] Line {}: {}",
                        icon, issue.severity, issue.line, issue.message
                    );
                    if let Some(suggestion) = issue.suggestion {
                        println!("   Suggestion: {}", suggestion);
                    }
                }
            }
            CodeCommand::Fix {
                file,
                issue,
                output,
            } => {
                let code = std::fs::read_to_string(&file)?;
                let progress = TaskProgress::new("Fixing code");
                let fixed = client.fix_code(&code, issue.as_deref()).await?;
                progress.finish_success(None);

                if let Some(path) = output {
                    std::fs::write(&path, &fixed)?;
                    println!("✅ Fixed code saved to {}", path.display());
                } else {
                    println!("💻 Fixed Code:\n```\n{}\n```", fixed);
                }
            }
            CodeCommand::Test {
                file,
                framework,
                output,
            } => {
                let code = std::fs::read_to_string(&file)?;
                let progress = TaskProgress::new("Generating tests");
                let tests = client.generate_tests(&code, framework.as_deref()).await?;
                progress.finish_success(None);

                if let Some(path) = output {
                    std::fs::write(&path, &tests)?;
                    println!("✅ Tests saved to {}", path.display());
                } else {
                    println!("🧪 Generated Tests:\n```\n{}\n```", tests);
                }
            }
            CodeCommand::Doc { file, format } => {
                let code = std::fs::read_to_string(&file)?;
                let progress = TaskProgress::new("Generating documentation");
                let docs = client
                    .generate_docs(&code, &format!("{:?}", format).to_lowercase())
                    .await?;
                progress.finish_success(None);
                println!("{}", docs);
            }
        },

        InferCommand::Multimodal(cmd) => match cmd {
            MultimodalCommand::Chat {
                image,
                prompt,
                model,
            } => {
                let progress = TaskProgress::new("Processing");
                let response = client
                    .multimodal_chat(&image, &prompt, model.as_deref())
                    .await?;
                progress.finish_success(None);
                println!("🤖 {}\n", response);
            }
            MultimodalCommand::Document {
                file,
                question,
                structured,
            } => {
                let progress = TaskProgress::new("Analyzing document");
                let result = client
                    .analyze_document(&file, &question, structured)
                    .await?;
                progress.finish_success(None);

                if structured {
                    println!(
                        "📄 Structured Data:\n{}",
                        serde_json::to_string_pretty(&result.data)?
                    );
                } else {
                    println!("📄 Document Analysis:\n{}", result.text);
                }

                if !question.is_empty() {
                    println!("\n❓ Answers:");
                    for (q, a) in result.answers {
                        println!("  Q: {}\n  A: {}\n", q, a);
                    }
                }
            }
        },

        _ => {
            println!("Command not yet implemented");
        }
    }

    Ok(())
}

// Request/Response types
#[derive(serde::Deserialize)]
struct Capability {
    id: String,
    name: String,
    category: String,
    description: String,
    available: bool,
    models: Vec<String>,
    pricing: Option<f64>,
}

#[derive(serde::Serialize)]
struct TextRequest {
    prompt: String,
    model_id: Option<String>,
    max_tokens: u32,
    temperature: Option<f32>,
}

#[derive(serde::Deserialize)]
struct TextResponse {
    text: String,
    tokens: u32,
}

#[derive(serde::Deserialize)]
struct SentimentResult {
    label: String,
    score: f32,
    confidence: f32,
    breakdown: Option<Vec<(String, f32)>>,
}

#[derive(serde::Deserialize)]
struct Entity {
    text: String,
    entity_type: String,
    confidence: f32,
}

#[derive(serde::Deserialize)]
struct Translation {
    text: String,
    source_language: String,
    target_language: String,
}

#[derive(serde::Deserialize)]
struct Answer {
    text: String,
    sources: Option<Vec<String>>,
}

#[derive(serde::Serialize)]
struct ImageGenRequest {
    prompt: String,
    size: String,
    n: u32,
    model_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct ImageGenResult {
    images: Vec<GeneratedImage>,
}

#[derive(serde::Deserialize)]
struct GeneratedImage {
    data: Vec<u8>,
    revised_prompt: Option<String>,
}

#[derive(serde::Deserialize)]
struct ImageAnalysis {
    description: String,
    objects: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Transcription {
    text: String,
    segments: Vec<Segment>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Segment {
    start: f32,
    end: f32,
    text: String,
}

impl Transcription {
    fn to_srt(&self) -> String {
        let mut srt = String::new();
        for (i, seg) in self.segments.iter().enumerate() {
            srt.push_str(&format!(
                "{}\n{} --> {}\n{}\n\n",
                i + 1,
                format_time(seg.start),
                format_time(seg.end),
                seg.text
            ));
        }
        srt
    }
    fn to_vtt(&self) -> String {
        let mut vtt = "WEBVTT\n\n".to_string();
        for seg in &self.segments {
            vtt.push_str(&format!(
                "{} --> {}\n{}\n\n",
                format_time(seg.start),
                format_time(seg.end),
                seg.text
            ));
        }
        vtt
    }
}

fn format_time(seconds: f32) -> String {
    let hours = (seconds / 3600.0) as u32;
    let minutes = ((seconds % 3600.0) / 60.0) as u32;
    let secs = (seconds % 60.0) as u32;
    let millis = ((seconds % 1.0) * 1000.0) as u32;
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, secs, millis)
}

#[derive(serde::Deserialize)]
struct Voice {
    id: String,
    name: String,
    gender: String,
    language: String,
}

#[derive(serde::Deserialize)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
    full_content: Option<String>,
}

#[derive(serde::Deserialize)]
struct CrawledPage {
    url: String,
    content: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Cluster {
    keywords: Vec<String>,
    items: Vec<String>,
}

#[derive(serde::Deserialize)]
struct CodeReview {
    summary: String,
    issues: Vec<CodeIssue>,
}

#[derive(serde::Deserialize)]
struct CodeIssue {
    severity: String,
    line: u32,
    message: String,
    suggestion: Option<String>,
}

#[derive(serde::Deserialize)]
struct DocumentAnalysis {
    text: String,
    data: serde_json::Value,
    answers: Vec<(String, String)>,
}

use std::pin::Pin;

use futures::Stream;

// Client extension trait
trait InferClient {
    async fn list_capabilities(&self, category: Option<&str>) -> Result<Vec<Capability>>;
    async fn get_capability(&self, id: &str) -> Result<Capability>;
    async fn generate_text(&self, request: &TextRequest) -> Result<TextResponse>;
    async fn stream_text(
        &self,
        request: &TextRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
    async fn summarize(&self, text: &str, length: &str) -> Result<String>;
    async fn analyze_sentiment(&self, text: &str) -> Result<SentimentResult>;
    async fn extract_entities(&self, text: &str, types: &[String]) -> Result<Vec<Entity>>;
    async fn classify_text(&self, text: &str, categories: &[String]) -> Result<Vec<(String, f32)>>;
    async fn translate(&self, text: &str, to: &str, from: Option<&str>) -> Result<Translation>;
    async fn answer_question(
        &self,
        question: &str,
        context: Option<&str>,
        web_search: bool,
    ) -> Result<Answer>;
    async fn generate_image(&self, request: &ImageGenRequest) -> Result<ImageGenResult>;
    async fn edit_image(
        &self,
        image: &PathBuf,
        prompt: &str,
        mask: Option<&PathBuf>,
        output: Option<&PathBuf>,
    ) -> Result<()>;
    async fn create_image_variations(
        &self,
        image: &PathBuf,
        n: u32,
        output: Option<&PathBuf>,
    ) -> Result<Vec<PathBuf>>;
    async fn describe_image(
        &self,
        image: &str,
        detail: &str,
        question: Option<&str>,
    ) -> Result<ImageAnalysis>;
    async fn ocr_image(&self, image: &PathBuf) -> Result<String>;
    async fn transcribe_audio(
        &self,
        file: &PathBuf,
        language: Option<&str>,
        timestamps: bool,
    ) -> Result<Transcription>;
    async fn translate_audio(&self, file: &PathBuf) -> Result<Transcription>;
    async fn generate_audio(&self, prompt: &str, duration: Option<f32>) -> Result<Vec<u8>>;
    async fn text_to_speech(
        &self,
        text: &str,
        voice: Option<&str>,
        speed: Option<f32>,
    ) -> Result<Vec<u8>>;
    async fn list_voices(&self, language: Option<&str>) -> Result<Vec<Voice>>;
    async fn web_search(
        &self,
        query: &str,
        limit: usize,
        site: Option<&str>,
    ) -> Result<Vec<SearchResult>>;
    async fn fetch_webpage(&self, url: &str, extract: bool, format: &str) -> Result<String>;
    async fn crawl_website(
        &self,
        url: &str,
        max_pages: usize,
        same_domain: bool,
    ) -> Result<Vec<CrawledPage>>;
    async fn create_embeddings(
        &self,
        texts: &[String],
        model: Option<&str>,
        batch_size: usize,
    ) -> Result<Vec<Vec<f32>>>;
    async fn calculate_similarity(
        &self,
        text1: &str,
        text2: &str,
        model: Option<&str>,
    ) -> Result<f32>;
    async fn cluster_texts(&self, texts: &[String], n_clusters: usize) -> Result<Vec<Cluster>>;
    async fn complete_code(
        &self,
        code: &str,
        language: Option<&str>,
        context: &[String],
    ) -> Result<String>;
    async fn explain_code(&self, code: &str, detail: &str) -> Result<String>;
    async fn review_code(&self, code: &str, focus: &[String]) -> Result<CodeReview>;
    async fn fix_code(&self, code: &str, issue: Option<&str>) -> Result<String>;
    async fn generate_tests(&self, code: &str, framework: Option<&str>) -> Result<String>;
    async fn generate_docs(&self, code: &str, format: &str) -> Result<String>;
    async fn multimodal_chat(
        &self,
        images: &[PathBuf],
        prompt: &str,
        model: Option<&str>,
    ) -> Result<String>;
    async fn analyze_document(
        &self,
        file: &PathBuf,
        questions: &[String],
        structured: bool,
    ) -> Result<DocumentAnalysis>;
}

// Stub implementations
impl InferClient for crate::client::ApiClient {
    async fn list_capabilities(&self, _category: Option<&str>) -> Result<Vec<Capability>> {
        Ok(vec![])
    }
    async fn get_capability(&self, _id: &str) -> Result<Capability> {
        anyhow::bail!("Not implemented")
    }
    async fn generate_text(&self, _request: &TextRequest) -> Result<TextResponse> {
        anyhow::bail!("Not implemented")
    }
    async fn stream_text(
        &self,
        _request: &TextRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        anyhow::bail!("Not implemented")
    }
    async fn summarize(&self, _text: &str, _length: &str) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn analyze_sentiment(&self, _text: &str) -> Result<SentimentResult> {
        anyhow::bail!("Not implemented")
    }
    async fn extract_entities(&self, _text: &str, _types: &[String]) -> Result<Vec<Entity>> {
        Ok(vec![])
    }
    async fn classify_text(
        &self,
        _text: &str,
        _categories: &[String],
    ) -> Result<Vec<(String, f32)>> {
        Ok(vec![])
    }
    async fn translate(&self, _text: &str, _to: &str, _from: Option<&str>) -> Result<Translation> {
        anyhow::bail!("Not implemented")
    }
    async fn answer_question(
        &self,
        _question: &str,
        _context: Option<&str>,
        _web_search: bool,
    ) -> Result<Answer> {
        anyhow::bail!("Not implemented")
    }
    async fn generate_image(&self, _request: &ImageGenRequest) -> Result<ImageGenResult> {
        anyhow::bail!("Not implemented")
    }
    async fn edit_image(
        &self,
        _image: &PathBuf,
        _prompt: &str,
        _mask: Option<&PathBuf>,
        _output: Option<&PathBuf>,
    ) -> Result<()> {
        Ok(())
    }
    async fn create_image_variations(
        &self,
        _image: &PathBuf,
        _n: u32,
        _output: Option<&PathBuf>,
    ) -> Result<Vec<PathBuf>> {
        Ok(vec![])
    }
    async fn describe_image(
        &self,
        _image: &str,
        _detail: &str,
        _question: Option<&str>,
    ) -> Result<ImageAnalysis> {
        anyhow::bail!("Not implemented")
    }
    async fn ocr_image(&self, _image: &PathBuf) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn transcribe_audio(
        &self,
        _file: &PathBuf,
        _language: Option<&str>,
        _timestamps: bool,
    ) -> Result<Transcription> {
        anyhow::bail!("Not implemented")
    }
    async fn translate_audio(&self, _file: &PathBuf) -> Result<Transcription> {
        anyhow::bail!("Not implemented")
    }
    async fn generate_audio(&self, _prompt: &str, _duration: Option<f32>) -> Result<Vec<u8>> {
        anyhow::bail!("Not implemented")
    }
    async fn text_to_speech(
        &self,
        _text: &str,
        _voice: Option<&str>,
        _speed: Option<f32>,
    ) -> Result<Vec<u8>> {
        anyhow::bail!("Not implemented")
    }
    async fn list_voices(&self, _language: Option<&str>) -> Result<Vec<Voice>> {
        Ok(vec![])
    }
    async fn web_search(
        &self,
        _query: &str,
        _limit: usize,
        _site: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        Ok(vec![])
    }
    async fn fetch_webpage(&self, _url: &str, _extract: bool, _format: &str) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn crawl_website(
        &self,
        _url: &str,
        _max_pages: usize,
        _same_domain: bool,
    ) -> Result<Vec<CrawledPage>> {
        Ok(vec![])
    }
    async fn create_embeddings(
        &self,
        _texts: &[String],
        _model: Option<&str>,
        _batch_size: usize,
    ) -> Result<Vec<Vec<f32>>> {
        Ok(vec![])
    }
    async fn calculate_similarity(
        &self,
        _text1: &str,
        _text2: &str,
        _model: Option<&str>,
    ) -> Result<f32> {
        Ok(0.0)
    }
    async fn cluster_texts(&self, _texts: &[String], _n_clusters: usize) -> Result<Vec<Cluster>> {
        Ok(vec![])
    }
    async fn complete_code(
        &self,
        _code: &str,
        _language: Option<&str>,
        _context: &[String],
    ) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn explain_code(&self, _code: &str, _detail: &str) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn review_code(&self, _code: &str, _focus: &[String]) -> Result<CodeReview> {
        Ok(CodeReview {
            summary: String::new(),
            issues: vec![],
        })
    }
    async fn fix_code(&self, _code: &str, _issue: Option<&str>) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn generate_tests(&self, _code: &str, _framework: Option<&str>) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn generate_docs(&self, _code: &str, _format: &str) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn multimodal_chat(
        &self,
        _images: &[PathBuf],
        _prompt: &str,
        _model: Option<&str>,
    ) -> Result<String> {
        anyhow::bail!("Not implemented")
    }
    async fn analyze_document(
        &self,
        _file: &PathBuf,
        _questions: &[String],
        _structured: bool,
    ) -> Result<DocumentAnalysis> {
        anyhow::bail!("Not implemented")
    }
}
