//! Social Brain API
//!
//! Public API for the social brain module providing unified access to
//! all cognitive capabilities: memory, emotion, learning, and reasoning.
//!
//! # Example
//!
//! ```
//! use beebotos_brain::api::{ApiConfig, SocialBrainApi};
//! use beebotos_brain::pad::Pad;
//!
//! // Create API instance
//! let mut api = SocialBrainApi::new();
//!
//! // Process a stimulus
//! let response = api.process_stimulus("Hello, world!").unwrap();
//! println!("Response: {}", response.response);
//!
//! // Apply emotional stimulus
//! api.apply_emotional_stimulus(Pad::new(0.5, 0.3, 0.2), 0.8);
//! ```
//!
//! # Thread Safety
//!
//! **Important**: `SocialBrainApi` is not `Send` or `Sync`. It is designed for
//! single-threaded use within a single agent context.
//!
//! ## Recommended Patterns
//!
//! ### Pattern 1: Single-threaded use (Recommended)
//!
//! ```ignore
//! // Create one API per thread
//! std::thread::spawn(|| {
//!     let mut api = SocialBrainApi::new();
//!     api.process_stimulus("Hello").unwrap();
//! });
//! ```
//!
//! ### Pattern 2: Shared with Arc<Mutex<_>>
//!
//! ```ignore
//! use std::sync::{Arc, Mutex};
//! use beebotos_brain::SocialBrainApi;
//!
//! let api = Arc::new(Mutex::new(SocialBrainApi::new()));
//! let api_clone = Arc::clone(&api);
//!
//! std::thread::spawn(move || {
//!     let mut api = api_clone.lock().unwrap();
//!     api.process_stimulus("Hello").unwrap();
//! });
//! ```
//!
//! ### Pattern 3: Message passing
//!
//! ```ignore
//! use std::sync::mpsc;
//!
//! let (tx, rx) = mpsc::channel();
//!
//! std::thread::spawn(move || {
//!     let mut api = SocialBrainApi::new();
//!     while let Ok(msg) = rx.recv() {
//!         api.process_stimulus(&msg).unwrap();
//!     }
//! });
//!
//! tx.send("Hello".to_string()).unwrap();
//! ```
//!
//! ## Safety Guarantees
//!
//! - All internal state is contained within the API instance
//! - No global mutable state (except metrics, which uses thread-safe counters)
//! - All public methods take `&mut self` where mutation occurs
//! - Input validation on all public APIs prevents undefined behavior

use crate::cognition::decision::{DecisionContext, DecisionEngine};
use crate::cognition::perception::{
    PerceptualInput, PerceptualSystem, SensoryModality, TextFeatureExtractor,
};
use crate::cognition::{Action, Belief, CognitiveState, Goal, MemoryItem};
// EmotionState 现在通过 From<Pad> 转换，无需直接导入
use crate::error::BrainResult;
use crate::memory::{MemoryQuery, MemoryResults, UnifiedMemory};
use crate::neat::{Genome, NeuralNetwork};
use crate::pad::{EmotionalEvent, EmotionalIntelligence, Pad};
use crate::personality::OceanProfile;
use crate::utils::compare_f32;

// =============================================================================
// Constants for sentiment analysis
// =============================================================================

/// Pleasure value when positive words dominate
const SENTIMENT_POSITIVE_PLEASURE: f32 = 0.3;
/// Pleasure value when negative words dominate  
const SENTIMENT_NEGATIVE_PLEASURE: f32 = -0.3;
/// Base arousal level for sentiment
const SENTIMENT_BASE_AROUSAL: f32 = 0.3;
/// Arousal increment per urgent word detected
const SENTIMENT_URGENCY_AROUSAL_INCREMENT: f32 = 0.2;
/// Default dominance level for sentiment
const SENTIMENT_DEFAULT_DOMINANCE: f32 = 0.5;

/// Personality influence factor on pleasure (neuroticism)
const PERSONALITY_NEUROTICISM_PLEASURE_FACTOR: f32 = 0.5;
/// Personality influence factor on arousal (openness)
const PERSONALITY_OPENNESS_AROUSAL_FACTOR: f32 = 0.2;

/// High arousal threshold for action suggestion
const HIGH_AROUSAL_THRESHOLD: f32 = 0.7;
/// Negative pleasure threshold for action suggestion
const NEGATIVE_PLEASURE_THRESHOLD: f32 = -0.5;

/// Social Brain API - unified interface to all cognitive capabilities
pub struct SocialBrainApi {
    /// Cognitive state management
    cognitive_state: CognitiveState,
    /// Memory system
    memory: UnifiedMemory,
    /// Emotional intelligence
    emotional_intelligence: EmotionalIntelligence,
    /// Personality profile
    personality: OceanProfile,
    /// Neural network brain
    network: Option<NeuralNetwork>,
    /// Perceptual system for processing inputs
    perceptual_system: PerceptualSystem,
    /// Decision engine for making choices
    decision_engine: DecisionEngine,
    /// API configuration
    config: ApiConfig,
}

/// API configuration for SocialBrainApi.
///
/// Controls which cognitive modules are enabled and their behavior.
/// Use `ApiConfig::default()` for standard settings.
///
/// # Example
/// ```rust
/// use beebotos_social_brain::api::ApiConfig;
///
/// let config = ApiConfig {
///     memory_enabled: true,
///     emotion_enabled: true,
///     learning_enabled: false, // Disable learning
///     personality_influence: 0.7,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// Enable memory system (short-term, episodic, semantic, procedural)
    pub memory_enabled: bool,
    /// Enable emotional processing (PAD model)
    pub emotion_enabled: bool,
    /// Enable learning module
    pub learning_enabled: bool,
    /// Personality influence factor (0.0-1.0) on emotional processing
    pub personality_influence: f32,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self::from_brain_config(&crate::BrainConfig::default())
    }
}

impl ApiConfig {
    /// Create ApiConfig from BrainConfig
    pub fn from_brain_config(config: &crate::BrainConfig) -> Self {
        Self {
            memory_enabled: config.memory.enabled,
            emotion_enabled: config.pad.enabled,
            learning_enabled: config.features.learning,
            personality_influence: config.personality.learning_rate * 50.0, // 转换因子
        }
    }

    /// Create ApiConfig with BrainConfig (更完整的配置)
    pub fn from_full_config(config: crate::BrainConfig) -> Self {
        Self::from_brain_config(&config)
    }
}

impl SocialBrainApi {
    /// Create new API instance with default configuration
    pub fn new() -> Self {
        Self::with_config(ApiConfig::default())
    }

    /// Create new API instance with BrainConfig (推荐)
    ///
    /// 使用完整的 BrainConfig 进行配置，避免配置重复
    pub fn with_brain_config(brain_config: crate::BrainConfig) -> Self {
        Self::with_config(ApiConfig::from_brain_config(&brain_config))
    }

    /// Create new API instance with custom configuration
    pub fn with_config(config: ApiConfig) -> Self {
        let personality = OceanProfile::balanced();

        // Initialize perceptual system with default extractors
        let mut perceptual_system = PerceptualSystem::new();
        perceptual_system.register_extractor(Box::new(TextFeatureExtractor));

        // Initialize decision engine with default strategy
        let mut decision_engine = DecisionEngine::new();
        decision_engine.set_strategy(0); // ExpectedValue strategy

        Self {
            cognitive_state: CognitiveState::new(),
            memory: UnifiedMemory::new(),
            emotional_intelligence: EmotionalIntelligence::new(),
            personality,
            network: None,
            perceptual_system,
            decision_engine,
            config,
        }
    }

    /// Initialize with a neural network genome
    pub fn with_genome(mut self, genome: &Genome) -> Self {
        self.network = Some(NeuralNetwork::from_genome(genome));
        self
    }

    /// Query the memory system for relevant information.
    ///
    /// Searches across all enabled memory types (short-term, episodic,
    /// semantic, procedural) based on the provided query parameters.
    ///
    /// # Arguments
    /// * `query` - Memory query specifying search terms, filters, and result
    ///   limits
    ///
    /// # Returns
    /// * `Ok(MemoryResults)` - Query results from all memory types
    /// * `Err(BrainError)` - If memory system encounters an error
    ///
    /// # Example
    /// ```rust
    /// use beebotos_social_brain::memory::{MemoryQuery, MemoryType};
    ///
    /// let query = MemoryQuery::new("project meeting")
    ///     .with_types(vec![MemoryType::Episodic, MemoryType::Semantic])
    ///     .with_limit(5);
    ///
    /// let results = api.query_memory(&query).unwrap();
    /// ```
    pub fn query_memory(&self, query: &MemoryQuery) -> BrainResult<MemoryResults> {
        if !self.config.memory_enabled {
            return Ok(MemoryResults::default());
        }

        tracing::debug!("Querying memory: {:?}", query);
        self.memory.query(query)
    }

    /// Store information in memory with specified importance.
    ///
    /// Content is stored in short-term memory first. High-importance items
    /// are prioritized and may be consolidated to long-term memory during
    /// sleep.
    ///
    /// # Arguments
    /// * `content` - The information to store
    /// * `importance` - Importance level (0.0-1.0), affects retention priority
    ///
    /// # Returns
    /// * `Ok(String)` - ID of the stored memory item
    /// * `Err(BrainError)` - If storage fails
    pub fn store_memory(&mut self, content: &str, importance: f32) -> BrainResult<String> {
        // Validate input
        crate::utils::validate_input_length(content, 100000, false)
            .map_err(|e| crate::error::BrainError::InvalidParameter(e))?;
        let importance = crate::utils::validate_importance(importance)
            .map_err(|e| crate::error::BrainError::InvalidParameter(e))?;

        if !self.config.memory_enabled {
            return Ok(String::new());
        }

        // Store in short-term memory
        let id = self
            .memory
            .short_term
            .push_with_priority(
                content,
                crate::memory::Priority::from_importance(importance),
            )
            .map(|evicted| evicted.id);

        Ok(id.unwrap_or_default())
    }

    /// Get current emotional state as EmotionState.
    ///
    /// Converts the internal PAD representation to the higher-level
    /// EmotionState type for external use.
    ///
    /// # Returns
    /// Current emotional state, or neutral if emotions are disabled
    pub fn current_emotion(&self) -> crate::emotion::state::EmotionState {
        if !self.config.emotion_enabled {
            return crate::emotion::state::EmotionState::neutral();
        }

        // Convert PAD to EmotionState (使用新的 From 实现)
        let pad = self.emotional_intelligence.current();
        (*pad).into()
    }

    /// Get current emotional state as PAD values.
    ///
    /// Returns the raw Pleasure-Arousal-Dominance values representing
    /// the current emotional state.
    ///
    /// # Returns
    /// PAD state with pleasure (-1.0 to 1.0), arousal (0.0 to 1.0), dominance
    /// (0.0 to 1.0)
    pub fn current_pad(&self) -> Pad {
        *self.emotional_intelligence.current()
    }

    /// Apply an emotional stimulus to the system.
    ///
    /// Updates the current emotional state based on the stimulus, modified by
    /// personality traits and scaled by intensity. The stimulus is recorded
    /// in emotional history.
    ///
    /// # Arguments
    /// * `stimulus` - The emotional stimulus as PAD values
    /// * `intensity` - Stimulus intensity multiplier (0.0-1.0)
    ///
    /// # Example
    /// ```rust
    /// use beebotos_social_brain::pad::Pad;
    ///
    /// // Positive news with high intensity
    /// api.apply_emotional_stimulus(Pad::new(0.6, 0.4, 0.3), 0.9);
    ///
    /// // Threat detected - negative pleasure, high arousal
    /// api.apply_emotional_stimulus(Pad::new(-0.5, 0.8, 0.2), 0.7);
    /// ```
    pub fn apply_emotional_stimulus(&mut self, stimulus: Pad, intensity: f32) {
        if !self.config.emotion_enabled {
            return;
        }

        // Validate intensity
        let intensity = crate::utils::clamp_f32(intensity, 0.0, 1.0);

        // Modify stimulus based on personality
        let modified = self.modify_by_personality(stimulus, intensity);

        self.emotional_intelligence.update(&EmotionalEvent {
            description: "Stimulus".to_string(),
            pleasure_impact: modified.pleasure * intensity,
            arousal_impact: modified.arousal * intensity,
            dominance_impact: modified.dominance * intensity,
        });
    }

    /// Process an external stimulus through the cognitive system.
    ///
    /// This is the main entry point for the social brain. The stimulus is:
    /// 1. Stored in memory
    /// 2. Analyzed for emotional content
    /// 3. Processed through the neural network (if available)
    /// 4. Used to update goals and intentions
    ///
    /// # Arguments
    /// * `stimulus` - Input text or sensory information
    ///
    /// # Returns
    /// * `Ok(StimulusResponse)` - Response including memory ID, emotional
    ///   change, and action
    /// * `Err(BrainError)` - If processing fails
    ///
    /// # Example
    /// ```rust
    /// let response = api
    ///     .process_stimulus("Urgent: system failure detected")
    ///     .unwrap();
    /// println!("Memory ID: {}", response.memory_id);
    /// println!("Emotional impact: {:?}", response.emotional_change);
    /// if let Some(action) = response.action_recommended {
    ///     println!("Recommended action: {:?}", action);
    /// }
    /// ```
    pub fn process_stimulus(&mut self, stimulus: &str) -> BrainResult<StimulusResponse> {
        // Validate input length to prevent abuse
        crate::utils::validate_input_length(stimulus, 10000, false)
            .map_err(|e| crate::error::BrainError::InvalidParameter(e))?;

        tracing::debug!("Processing stimulus: {}", stimulus);

        // Store in memory
        let memory_id = self.store_memory(stimulus, 0.5)?;

        // Determine emotional impact based on content analysis
        let emotional_impact = self.analyze_sentiment(stimulus);
        self.apply_emotional_stimulus(emotional_impact, 0.3);

        // Generate response using neural network if available
        let response = if let Some(network) = &self.network {
            let inputs = self.encode_stimulus(stimulus);
            let outputs = network.predict(&inputs);
            self.decode_response(&outputs)
        } else {
            "Neural network not initialized".to_string()
        };

        // Create or update goal based on stimulus
        let goal = self.infer_goal(stimulus);
        if let Some(g) = goal {
            self.cognitive_state.set_goal(g);
        }

        Ok(StimulusResponse {
            memory_id,
            emotional_change: emotional_impact,
            response,
            action_recommended: self.suggest_action(),
        })
    }

    /// Set a goal for the agent with given priority.
    ///
    /// Goals are automatically sorted by priority (highest first).
    /// The top goal may become the current intention.
    ///
    /// # Arguments
    /// * `description` - Goal description
    /// * `priority` - Priority level (0.0-1.0), higher = more important
    ///
    /// # Returns
    /// Unique ID for the created goal
    pub fn set_goal(&mut self, description: &str, priority: f32) -> BrainResult<String> {
        // Validate inputs
        crate::utils::validate_input_length(description, 1000, false)
            .map_err(|e| crate::error::BrainError::InvalidParameter(e))?;
        let priority = crate::utils::validate_priority(priority)
            .map_err(|e| crate::error::BrainError::InvalidParameter(e))?;

        let goal = Goal::new(description, priority);
        let id = goal.id.clone();
        self.cognitive_state.set_goal(goal);
        Ok(id)
    }

    /// Get list of current goals, sorted by priority (highest first).
    ///
    /// # Returns
    /// Vector of goal references
    pub fn current_goals(&self) -> Vec<&Goal> {
        self.cognitive_state.goals.iter().collect()
    }

    /// Form an intention from the highest priority goal.
    ///
    /// Creates a concrete plan to achieve the top goal and marks it
    /// as the current intention.
    ///
    /// # Returns
    /// The formed intention, or None if no goals exist
    pub fn form_intention(&mut self) -> Option<crate::cognition::Intention> {
        self.cognitive_state.form_intention()
    }

    /// Run memory consolidation process ("sleep" mode).
    ///
    /// Transfers frequently rehearsed items from short-term memory
    /// to long-term storage (episodic or semantic based on content).
    ///
    /// # Returns
    /// * `Ok(usize)` - Number of memories consolidated
    /// * `Err(BrainError)` - If consolidation fails
    pub fn consolidate_memories(&mut self) -> BrainResult<usize> {
        self.memory.consolidate()
    }

    /// Get reference to current personality profile.
    ///
    /// # Returns
    /// Reference to OCEAN personality profile
    pub fn personality(&self) -> &OceanProfile {
        &self.personality
    }

    /// Update the personality profile.
    ///
    /// Changes take effect immediately on subsequent emotional processing.
    ///
    /// # Arguments
    /// * `personality` - New OCEAN personality profile
    pub fn set_personality(&mut self, personality: OceanProfile) {
        self.personality = personality;
    }

    /// Process input through the neural network.
    ///
    /// Runs a forward pass through the evolved neural network.
    ///
    /// # Arguments
    /// * `inputs` - Input values for the network
    ///
    /// # Returns
    /// Network outputs, or None if no network is initialized
    pub fn think(&self, inputs: &[f32]) -> Option<Vec<f32>> {
        self.network.as_ref().map(|n| n.predict(inputs))
    }

    /// Get current API statistics.
    ///
    /// Returns information about memory usage, active goals,
    /// emotional state, and network status.
    ///
    /// # Returns
    /// Statistics snapshot
    pub fn stats(&self) -> ApiStats {
        ApiStats {
            memory_items: self.memory.short_term.len() + self.memory.episodic.len(),
            active_goals: self.cognitive_state.goals.len(),
            current_emotion: self.current_emotion(),
            has_network: self.network.is_some(),
            working_memory_items: self.cognitive_state.working_memory.items().len(),
            beliefs_count: self.get_beliefs().len(),
            decision_strategies: self.decision_engine.strategy_count(),
        }
    }

    // Private helper methods

    /// Modify emotional stimulus based on personality traits.
    ///
    /// # Personality Effects
    /// - **Neuroticism**: Amplifies negative emotions (higher = more negative
    ///   response)
    /// - **Openness**: Increases arousal (higher = more excited response)
    ///
    /// # Arguments
    /// * `stimulus` - Raw emotional stimulus
    /// * `_intensity` - Stimulus intensity (reserved for future use)
    ///
    /// # Returns
    /// Modified PAD state after personality influence
    fn modify_by_personality(&self, stimulus: Pad, _intensity: f32) -> Pad {
        // Personality modifies emotional response
        let o = self.personality.openness;
        let n = self.personality.neuroticism;

        // High neuroticism amplifies negative emotions
        let pleasure_mod = if stimulus.pleasure < 0.0 {
            stimulus.pleasure * (1.0 + n * PERSONALITY_NEUROTICISM_PLEASURE_FACTOR)
        } else {
            stimulus.pleasure
        };

        // High openness increases arousal
        let arousal_mod = stimulus.arousal * (1.0 + o * PERSONALITY_OPENNESS_AROUSAL_FACTOR);

        Pad::new(pleasure_mod, arousal_mod, stimulus.dominance)
    }

    /// Analyze sentiment of text and convert to PAD emotional state.
    ///
    /// Uses keyword-based sentiment analysis:
    /// - Positive words (good, great, etc.) increase pleasure
    /// - Negative words (bad, fail, etc.) decrease pleasure
    /// - Urgent words (urgent, critical, etc.) increase arousal
    ///
    /// # Arguments
    /// * `text` - Input text to analyze
    ///
    /// # Returns
    /// PAD state representing the emotional tone of the text
    ///
    /// # Example
    /// ```
    /// // "urgent success" -> positive pleasure + high arousal
    /// // "terrible failure" -> negative pleasure + base arousal
    /// ```
    fn analyze_sentiment(&self, text: &str) -> Pad {
        let text_lower = text.to_lowercase();

        let positive_words = ["good", "great", "excellent", "happy", "success", "win"];
        let negative_words = ["bad", "terrible", "sad", "fail", "error", "problem"];
        let urgent_words = ["urgent", "critical", "emergency", "now", "immediate"];

        let pos_count = positive_words
            .iter()
            .filter(|w| text_lower.contains(*w))
            .count();
        let neg_count = negative_words
            .iter()
            .filter(|w| text_lower.contains(*w))
            .count();
        let urg_count = urgent_words
            .iter()
            .filter(|w| text_lower.contains(*w))
            .count();

        let pleasure = if pos_count > neg_count {
            SENTIMENT_POSITIVE_PLEASURE
        } else if neg_count > pos_count {
            SENTIMENT_NEGATIVE_PLEASURE
        } else {
            0.0
        };

        let arousal =
            SENTIMENT_BASE_AROUSAL + (urg_count as f32 * SENTIMENT_URGENCY_AROUSAL_INCREMENT);
        let dominance = SENTIMENT_DEFAULT_DOMINANCE;

        Pad::new(pleasure, arousal, dominance)
    }

    fn encode_stimulus(&self, _stimulus: &str) -> Vec<f32> {
        // Simple encoding - would use more sophisticated NLP in production
        vec![0.5; 4] // Placeholder
    }

    fn decode_response(&self, outputs: &[f32]) -> String {
        if outputs.is_empty() {
            return "No response".to_string();
        }

        let max_idx = outputs
            .iter()
            .enumerate()
            .max_by(|a, b| compare_f32(a.1, b.1))
            .map(|(i, _)| i)
            .unwrap_or(0);

        match max_idx {
            0 => "Analyze situation".to_string(),
            1 => "Take action".to_string(),
            2 => "Request more information".to_string(),
            _ => "Process stimulus".to_string(),
        }
    }

    fn infer_goal(&self, stimulus: &str) -> Option<Goal> {
        let text_lower = stimulus.to_lowercase();

        if text_lower.contains("urgent") || text_lower.contains("critical") {
            Some(Goal::new("Handle urgent situation", 0.9))
        } else if text_lower.contains("learn") || text_lower.contains("study") {
            Some(Goal::new("Acquire knowledge", 0.7))
        } else if text_lower.contains("help") || text_lower.contains("assist") {
            Some(Goal::new("Provide assistance", 0.8))
        } else {
            None
        }
    }

    /// Suggest an action based on current emotional state.
    ///
    /// # Action Triggers
    /// - **High arousal** (> HIGH_AROUSAL_THRESHOLD): Suggest calming action
    /// - **Negative pleasure** (< NEGATIVE_PLEASURE_THRESHOLD): Suggest mood
    ///   improvement
    ///
    /// # Returns
    /// Optional action recommendation, or None if no action needed
    fn suggest_action(&self) -> Option<Action> {
        // Suggest action based on current state
        let emotion = self.current_pad();

        if emotion.arousal > HIGH_AROUSAL_THRESHOLD {
            Some(Action::new("high_arousal_response"))
        } else if emotion.pleasure < NEGATIVE_PLEASURE_THRESHOLD {
            Some(Action::new("address_negative_state"))
        } else {
            None
        }
    }

    // =============================================================================
    // Cognitive System Integration
    // =============================================================================

    /// Process perceptual input through the cognitive system.
    ///
    /// # Arguments
    /// * `input` - Perceptual input to process
    ///
    /// # Returns
    /// Processed percept if attention threshold is met
    pub fn process_perception(
        &self,
        input: PerceptualInput,
    ) -> Option<crate::cognition::perception::Percept> {
        self.perceptual_system.process_input(input)
    }

    /// Create a perceptual input from text.
    ///
    /// # Arguments
    /// * `text` - Text content to process
    /// * `source` - Source identifier
    ///
    /// # Returns
    /// PerceptualInput ready for processing
    pub fn create_text_input(&self, text: &str, source: &str) -> PerceptualInput {
        PerceptualInput {
            modality: SensoryModality::Text,
            raw_data: text.as_bytes().to_vec(),
            timestamp: crate::utils::current_timestamp_secs(),
            source: source.to_string(),
            confidence: 1.0,
        }
    }

    /// Make a decision based on available options.
    ///
    /// # Arguments
    /// * `context` - Decision context with options and constraints
    ///
    /// # Returns
    /// Decision result with chosen option
    pub fn decide(
        &self,
        context: &DecisionContext,
    ) -> Option<crate::cognition::decision::Decision> {
        if context.available_options.is_empty() {
            return None;
        }
        Some(self.decision_engine.decide(context))
    }

    /// Add an item to working memory.
    ///
    /// # Arguments
    /// * `key` - Unique key for the memory item
    /// * `value` - JSON value to store
    /// * `activation` - Initial activation level (0.0-1.0)
    pub fn add_to_working_memory(&mut self, key: &str, value: serde_json::Value, activation: f32) {
        let item = MemoryItem {
            key: key.to_string(),
            value,
            activation,
            timestamp: crate::utils::current_timestamp_secs(),
        };
        self.cognitive_state.memorize(item);
    }

    /// Get an item from working memory.
    ///
    /// # Arguments
    /// * `key` - Key of the item to retrieve
    ///
    /// # Returns
    /// Reference to the memory item if found
    pub fn get_from_working_memory(&self, key: &str) -> Option<&MemoryItem> {
        self.cognitive_state.working_memory.get(key)
    }

    /// Add a belief to the cognitive state.
    ///
    /// # Arguments
    /// * `proposition` - The belief proposition
    /// * `confidence` - Confidence level (0.0-1.0)
    pub fn add_belief(&mut self, proposition: &str, confidence: f32) {
        let belief = Belief::new(proposition, confidence);
        // Store in working memory for now
        let item = MemoryItem {
            key: format!("belief:{}", belief.id),
            value: serde_json::json!({
                "proposition": belief.proposition,
                "confidence": belief.confidence,
                "source": format!("{:?}", belief.source),
            }),
            activation: confidence,
            timestamp: belief.timestamp,
        };
        self.cognitive_state.memorize(item);
    }

    /// Get all beliefs from working memory.
    ///
    /// # Returns
    /// Vector of beliefs (as MemoryItems)
    pub fn get_beliefs(&self) -> Vec<&MemoryItem> {
        self.cognitive_state
            .working_memory
            .items()
            .iter()
            .filter(|item| item.key.starts_with("belief:"))
            .collect()
    }

    /// Get the perceptual system reference.
    pub fn perceptual_system(&self) -> &PerceptualSystem {
        &self.perceptual_system
    }

    /// Get mutable reference to perceptual system.
    pub fn perceptual_system_mut(&mut self) -> &mut PerceptualSystem {
        &mut self.perceptual_system
    }

    /// Get the decision engine reference.
    pub fn decision_engine(&self) -> &DecisionEngine {
        &self.decision_engine
    }

    /// Compare multiple decision strategies.
    ///
    /// # Arguments
    /// * `context` - Decision context to evaluate
    ///
    /// # Returns
    /// Vector of (strategy_name, decision) pairs
    pub fn compare_strategies(
        &self,
        context: &DecisionContext,
    ) -> Vec<(String, crate::cognition::decision::Decision)> {
        self.decision_engine.compare_strategies(context)
    }
}

impl Default for SocialBrainApi {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from processing a stimulus through the social brain.
///
/// Contains the results of cognitive processing including memory storage,
/// emotional impact, generated response, and any recommended actions.
#[derive(Debug, Clone)]
pub struct StimulusResponse {
    /// ID of the memory item created for this stimulus
    pub memory_id: String,
    /// Emotional impact of the stimulus (PAD values)
    pub emotional_change: Pad,
    /// Text response generated by the neural network
    pub response: String,
    /// Optional action recommended based on emotional state
    pub action_recommended: Option<Action>,
}

/// Statistics snapshot of the SocialBrainApi state.
///
/// Useful for monitoring, debugging, and health checks.
#[derive(Debug, Clone)]
pub struct ApiStats {
    /// Total number of items in memory (short-term + episodic)
    pub memory_items: usize,
    /// Number of currently active goals
    pub active_goals: usize,
    /// Current emotional state
    pub current_emotion: crate::emotion::state::EmotionState,
    /// Whether a neural network is initialized
    pub has_network: bool,
    /// Number of items in working memory
    pub working_memory_items: usize,
    /// Number of beliefs stored
    pub beliefs_count: usize,
    /// Number of available decision strategies
    pub decision_strategies: usize,
}

/// Helper trait for priority conversion
trait PriorityHelper {
    fn from_importance(importance: f32) -> crate::memory::Priority;
}

impl PriorityHelper for crate::memory::Priority {
    fn from_importance(importance: f32) -> Self {
        match importance {
            i if i > 0.9 => crate::memory::Priority::Critical,
            i if i > 0.7 => crate::memory::Priority::High,
            i if i > 0.4 => crate::memory::Priority::Medium,
            _ => crate::memory::Priority::Low,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_creation() {
        let api = SocialBrainApi::new();
        let stats = api.stats();
        assert_eq!(stats.active_goals, 0);
    }

    #[test]
    fn test_goal_setting() {
        let mut api = SocialBrainApi::new();
        let goal_id = api.set_goal("Test goal", 0.8).unwrap();
        assert!(!goal_id.is_empty());
        assert_eq!(api.current_goals().len(), 1);
    }

    #[test]
    fn test_goal_setting_invalid_priority() {
        let mut api = SocialBrainApi::new();
        // Test invalid priority (too high)
        assert!(api.set_goal("Test goal", 1.5).is_err());
        // Test invalid priority (negative)
        assert!(api.set_goal("Test goal", -0.1).is_err());
        // Test invalid priority (NaN)
        assert!(api.set_goal("Test goal", f32::NAN).is_err());
    }

    #[test]
    fn test_process_stimulus_validation() {
        let mut api = SocialBrainApi::new();
        // Empty input should fail
        assert!(api.process_stimulus("").is_err());
        // Normal input should succeed
        assert!(api.process_stimulus("Hello").is_ok());
    }

    #[test]
    fn test_emotion_processing() {
        let mut api = SocialBrainApi::new();
        let initial = api.current_emotion();

        api.apply_emotional_stimulus(Pad::new(0.5, 0.3, 0.2), 0.5);

        let after = api.current_emotion();
        assert!(after.pleasure >= initial.pleasure);
    }
}
