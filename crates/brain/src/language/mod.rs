#![allow(dead_code)]

pub mod nlp;
pub mod sentiment;
pub mod translation;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageProcessor {
    pub supported_languages: Vec<Language>,
    pub default_language: Language,
    pub translation_cache: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    English,
    Chinese,
    Spanish,
    French,
    German,
    Japanese,
    Korean,
    Russian,
    Arabic,
    Portuguese,
}

impl Language {
    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Chinese => "zh",
            Language::Spanish => "es",
            Language::French => "fr",
            Language::German => "de",
            Language::Japanese => "ja",
            Language::Korean => "ko",
            Language::Russian => "ru",
            Language::Arabic => "ar",
            Language::Portuguese => "pt",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Chinese => "Chinese",
            Language::Spanish => "Spanish",
            Language::French => "French",
            Language::German => "German",
            Language::Japanese => "Japanese",
            Language::Korean => "Korean",
            Language::Russian => "Russian",
            Language::Arabic => "Arabic",
            Language::Portuguese => "Portuguese",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextAnalysis {
    pub text: String,
    pub language: Language,
    pub sentiment: SentimentScore,
    pub entities: Vec<Entity>,
    pub keywords: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentScore {
    pub positive: f32,
    pub negative: f32,
    pub neutral: f32,
    pub compound: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub text: String,
    pub entity_type: EntityType,
    pub start_pos: usize,
    pub end_pos: usize,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType {
    Person,
    Organization,
    Location,
    Date,
    Time,
    Money,
    Percentage,
    Product,
    Event,
}

pub struct LanguageAnalyzer;

impl LanguageAnalyzer {
    pub fn new() -> Self {
        Self
    }

    pub fn analyze(&self, text: &str, language: Language) -> TextAnalysis {
        let words: Vec<&str> = text.split_whitespace().collect();

        let sentiment = SentimentScore {
            positive: 0.5,
            negative: 0.1,
            neutral: 0.4,
            compound: 0.4,
        };

        let entities = vec![];

        let keywords: Vec<String> = words
            .iter()
            .filter(|w| w.len() > 4)
            .take(5)
            .map(|w| w.to_string())
            .collect();

        let summary = if words.len() > 20 {
            words[..20].join(" ") + "..."
        } else {
            text.to_string()
        };

        TextAnalysis {
            text: text.to_string(),
            language,
            sentiment,
            entities,
            keywords,
            summary,
        }
    }

    pub fn detect_language(&self, text: &str) -> Language {
        if text.chars().any(|c| matches!(c as u32, 0x4e00..=0x9fff)) {
            Language::Chinese
        } else {
            Language::English
        }
    }

    pub fn translate(&self, text: &str, from: Language, to: Language) -> String {
        if from == to {
            return text.to_string();
        }

        format!("[{}->{}] {}", from.code(), to.code(), text)
    }
}

pub struct ConversationContext {
    pub history: Vec<Utterance>,
    pub detected_language: Language,
    pub topic: Option<String>,
    pub user_intent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utterance {
    pub speaker: String,
    pub text: String,
    pub timestamp: u64,
    pub sentiment: SentimentScore,
}

impl ConversationContext {
    pub fn new() -> Self {
        Self {
            history: vec![],
            detected_language: Language::English,
            topic: None,
            user_intent: None,
        }
    }

    pub fn add_utterance(&mut self, speaker: String, text: String) {
        let utterance = Utterance {
            speaker,
            text: text.clone(),
            timestamp: chrono::Utc::now().timestamp() as u64,
            sentiment: SentimentScore {
                positive: 0.5,
                negative: 0.1,
                neutral: 0.4,
                compound: 0.4,
            },
        };

        self.history.push(utterance);

        if self.history.len() > 100 {
            self.history.remove(0);
        }
    }

    pub fn get_context_summary(&self) -> String {
        let recent: Vec<String> = self
            .history
            .iter()
            .rev()
            .take(5)
            .map(|u| format!("{}: {}", u.speaker, u.text))
            .collect();

        recent.join("\n")
    }
}
