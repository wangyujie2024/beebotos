//! Google Calendar Integration
//!
//! Provides scheduling and event management capabilities by integrating with
//! Google Calendar API. Supports event creation, querying, and modification.
//!
//! # Features
//! - Create events with attendees
//! - List upcoming events
//! - Check availability
//! - Manage recurring events
//! - OAuth2 authentication

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::error::{AgentError, Result};

/// Google Calendar API client
pub struct GoogleCalendarClient {
    /// OAuth2 access token
    access_token: String,
    /// HTTP client
    http_client: reqwest::Client,
    /// Base API URL
    base_url: String,
}

/// Calendar event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Event ID (assigned by Google)
    pub id: Option<String>,
    /// Event title
    pub title: String,
    /// Event description
    pub description: Option<String>,
    /// Event location
    pub location: Option<String>,
    /// Start time
    pub start: EventTime,
    /// End time
    pub end: EventTime,
    /// Attendees
    pub attendees: Vec<Attendee>,
    /// Event status
    pub status: EventStatus,
    /// Visibility
    pub visibility: Visibility,
    /// Recurrence rule (RFC 5545)
    pub recurrence: Option<Vec<String>>,
    /// Conference data (for video meetings)
    pub conference_data: Option<ConferenceData>,
    /// Reminders
    pub reminders: Option<Reminders>,
    /// Created time
    pub created: Option<DateTime<Utc>>,
    /// Updated time
    pub updated: Option<DateTime<Utc>>,
    /// Creator info
    pub creator: Option<Creator>,
    /// Color ID
    pub color_id: Option<String>,
    /// HTML link
    pub html_link: Option<String>,
}

/// Event time specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTime {
    /// DateTime (for specific time events)
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub date_time: Option<DateTime<Utc>>,
    /// Date (for all-day events, format: "yyyy-mm-dd")
    pub date: Option<String>,
    /// Time zone
    pub time_zone: Option<String>,
}

impl EventTime {
    /// Create from DateTime
    pub fn from_datetime(dt: DateTime<Utc>) -> Self {
        Self {
            date_time: Some(dt),
            date: None,
            time_zone: Some("UTC".to_string()),
        }
    }

    /// Create from date string (all-day event)
    pub fn from_date(date: impl Into<String>) -> Self {
        Self {
            date_time: None,
            date: Some(date.into()),
            time_zone: None,
        }
    }

    /// Get as DateTime if available
    pub fn to_datetime(&self) -> Option<DateTime<Utc>> {
        self.date_time
    }
}

/// Event attendee
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attendee {
    /// Email address
    pub email: String,
    /// Display name
    pub display_name: Option<String>,
    /// Response status
    pub response_status: ResponseStatus,
    /// Optional (attendance not required)
    pub optional: bool,
}

/// Response status for attendees
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    NeedsAction,
    Declined,
    Tentative,
    Accepted,
}

impl Default for ResponseStatus {
    fn default() -> Self {
        ResponseStatus::NeedsAction
    }
}

/// Event status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventStatus {
    Confirmed,
    Tentative,
    Cancelled,
}

impl Default for EventStatus {
    fn default() -> Self {
        EventStatus::Confirmed
    }
}

/// Event visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Default,
    Public,
    Private,
    Confidential,
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility::Default
    }
}

/// Conference data (Google Meet, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceData {
    /// Conference ID
    pub conference_id: Option<String>,
    /// Conference solution (e.g., "hangoutsMeet")
    pub conference_solution: Option<ConferenceSolution>,
    /// Entry points (links/phones)
    pub entry_points: Vec<EntryPoint>,
}

/// Conference solution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceSolution {
    /// Solution type key
    pub key: ConferenceSolutionKey,
    /// Solution name
    pub name: String,
}

/// Conference solution key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConferenceSolutionKey {
    /// Key type
    #[serde(rename = "type")]
    pub type_: String,
}

/// Entry point for conference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPoint {
    /// Entry point type
    #[serde(rename = "entryPointType")]
    pub entry_point_type: String,
    /// URI to join
    pub uri: String,
    /// Label
    pub label: Option<String>,
    /// PIN code
    pub pin: Option<String>,
}

/// Event reminders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminders {
    /// Whether to use default reminders
    pub use_default: bool,
    /// Custom overrides
    pub overrides: Option<Vec<ReminderOverride>>,
}

/// Reminder override
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderOverride {
    /// Method (popup, email)
    pub method: String,
    /// Minutes before event
    pub minutes: i32,
}

/// Event creator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Creator {
    /// Email
    pub email: String,
    /// Display name
    pub display_name: Option<String>,
    /// Whether this is the creator's self calendar
    pub self_: Option<bool>,
}

/// Calendar list entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarListEntry {
    /// Calendar ID
    pub id: String,
    /// Calendar summary/title
    pub summary: String,
    /// Calendar description
    pub description: Option<String>,
    /// Primary calendar flag
    pub primary: Option<bool>,
    /// Access role
    pub access_role: String,
    /// Background color
    pub background_color: Option<String>,
}

/// Free/busy query request
#[derive(Debug, Clone, Serialize)]
pub struct FreeBusyRequest {
    /// Time range start
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub time_min: DateTime<Utc>,
    /// Time range end
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub time_max: DateTime<Utc>,
    /// Time zone
    pub time_zone: String,
    /// Items (calendars) to check
    pub items: Vec<CalendarItem>,
}

/// Calendar item for free/busy
#[derive(Debug, Clone, Serialize)]
pub struct CalendarItem {
    /// Calendar ID
    pub id: String,
}

/// Free/busy response
#[derive(Debug, Clone, Deserialize)]
pub struct FreeBusyResponse {
    /// Kind
    pub kind: String,
    /// Time range start
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub time_min: DateTime<Utc>,
    /// Time range end
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub time_max: DateTime<Utc>,
    /// Calendars free/busy info
    pub calendars: HashMap<String, CalendarFreeBusy>,
}

/// Free/busy info for a calendar
#[derive(Debug, Clone, Deserialize)]
pub struct CalendarFreeBusy {
    /// Busy time slots
    pub busy: Vec<TimeSlot>,
}

/// Time slot
#[derive(Debug, Clone, Deserialize)]
pub struct TimeSlot {
    /// Start time
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub start: DateTime<Utc>,
    /// End time
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub end: DateTime<Utc>,
}

/// Query parameters for listing events
#[derive(Debug, Clone, Default)]
pub struct EventQuery {
    /// Max results to return
    pub max_results: Option<i32>,
    /// Start time filter
    pub time_min: Option<DateTime<Utc>>,
    /// End time filter
    pub time_max: Option<DateTime<Utc>>,
    /// Order by (startTime, updated)
    pub order_by: Option<String>,
    /// Search query
    pub q: Option<String>,
    /// Show deleted events
    pub show_deleted: bool,
    /// Show hidden invitations
    pub show_hidden_invitations: bool,
    /// Single events (expand recurring)
    pub single_events: bool,
}

impl GoogleCalendarClient {
    /// Create new client with access token
    pub fn new(access_token: impl Into<String>) -> Self {
        Self {
            access_token: access_token.into(),
            http_client: reqwest::Client::new(),
            base_url: "https://www.googleapis.com/calendar/v3".to_string(),
        }
    }

    /// Create new client with custom HTTP configuration
    pub fn with_client(access_token: impl Into<String>, client: reqwest::Client) -> Self {
        Self {
            access_token: access_token.into(),
            http_client: client,
            base_url: "https://www.googleapis.com/calendar/v3".to_string(),
        }
    }

    /// Update access token
    pub fn set_access_token(&mut self, token: impl Into<String>) {
        self.access_token = token.into();
    }

    /// Get authorization header
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    /// List user's calendars
    pub async fn list_calendars(&self) -> Result<Vec<CalendarListEntry>> {
        let url = format!("{}/users/me/calendarList", self.base_url);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to list calendars: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Google Calendar API error: {}",
                error_text
            )));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        let calendars: Vec<CalendarListEntry> = data
            .get("items")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        info!("Listed {} calendars", calendars.len());
        Ok(calendars)
    }

    /// Get primary calendar ID
    pub async fn get_primary_calendar_id(&self) -> Result<String> {
        let calendars = self.list_calendars().await?;
        calendars
            .into_iter()
            .find(|c| c.primary == Some(true))
            .map(|c| c.id)
            .ok_or_else(|| AgentError::not_found("No primary calendar found"))
    }

    /// List events from a calendar
    pub async fn list_events(
        &self,
        calendar_id: &str,
        query: EventQuery,
    ) -> Result<Vec<CalendarEvent>> {
        let mut url = format!("{}/calendars/{}/events", self.base_url, calendar_id);

        // Build query string
        let mut params = Vec::new();
        if let Some(max) = query.max_results {
            params.push(format!("maxResults={}", max));
        }
        if let Some(min) = query.time_min {
            params.push(format!("timeMin={}", min.to_rfc3339()));
        }
        if let Some(max) = query.time_max {
            params.push(format!("timeMax={}", max.to_rfc3339()));
        }
        if let Some(order) = query.order_by {
            params.push(format!("orderBy={}", order));
        }
        if let Some(q) = query.q {
            params.push(format!("q={}", urlencoding::encode(&q)));
        }
        if query.show_deleted {
            params.push("showDeleted=true".to_string());
        }
        if query.show_hidden_invitations {
            params.push("showHiddenInvitations=true".to_string());
        }
        if query.single_events {
            params.push("singleEvents=true".to_string());
        }

        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to list events: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Google Calendar API error: {}",
                error_text
            )));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        let events: Vec<CalendarEvent> = data
            .get("items")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        debug!(
            "Listed {} events from calendar {}",
            events.len(),
            calendar_id
        );
        Ok(events)
    }

    /// Get a single event
    pub async fn get_event(&self, calendar_id: &str, event_id: &str) -> Result<CalendarEvent> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            self.base_url, calendar_id, event_id
        );

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get event: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Google Calendar API error: {}",
                error_text
            )));
        }

        let event: CalendarEvent = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        Ok(event)
    }

    /// Create a new event
    pub async fn create_event(
        &self,
        calendar_id: &str,
        event: CalendarEvent,
    ) -> Result<CalendarEvent> {
        let url = format!("{}/calendars/{}/events", self.base_url, calendar_id);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", self.auth_header())
            .json(&event)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to create event: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Google Calendar API error: {}",
                error_text
            )));
        }

        let created: CalendarEvent = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        info!("Created event: {:?}", created.id);
        Ok(created)
    }

    /// Update an existing event
    pub async fn update_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        event: CalendarEvent,
    ) -> Result<CalendarEvent> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            self.base_url, calendar_id, event_id
        );

        let response = self
            .http_client
            .put(&url)
            .header("Authorization", self.auth_header())
            .json(&event)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to update event: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Google Calendar API error: {}",
                error_text
            )));
        }

        let updated: CalendarEvent = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        info!("Updated event: {}", event_id);
        Ok(updated)
    }

    /// Delete an event
    pub async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<()> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            self.base_url, calendar_id, event_id
        );

        let response = self
            .http_client
            .delete(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to delete event: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Google Calendar API error: {}",
                error_text
            )));
        }

        info!("Deleted event: {}", event_id);
        Ok(())
    }

    /// Query free/busy information
    pub async fn query_free_busy(&self, request: FreeBusyRequest) -> Result<FreeBusyResponse> {
        let url = format!("{}/freeBusy", self.base_url);

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", self.auth_header())
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to query free/busy: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Google Calendar API error: {}",
                error_text
            )));
        }

        let result: FreeBusyResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        Ok(result)
    }

    /// Check if a time slot is free
    pub async fn is_time_slot_free(
        &self,
        calendar_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<bool> {
        let request = FreeBusyRequest {
            time_min: start,
            time_max: end,
            time_zone: "UTC".to_string(),
            items: vec![CalendarItem {
                id: calendar_id.to_string(),
            }],
        };

        let response = self.query_free_busy(request).await?;

        if let Some(calendar) = response.calendars.get(calendar_id) {
            Ok(calendar.busy.is_empty())
        } else {
            Ok(true) // Assume free if calendar not found
        }
    }

    /// Find next available slot
    pub async fn find_next_available_slot(
        &self,
        calendar_id: &str,
        after: DateTime<Utc>,
        duration_minutes: i64,
        max_days: i64,
    ) -> Result<Option<DateTime<Utc>>> {
        let end = after + Duration::days(max_days);
        let duration = Duration::minutes(duration_minutes);

        // Get all events in the range
        let events = self
            .list_events(
                calendar_id,
                EventQuery {
                    time_min: Some(after),
                    time_max: Some(end),
                    single_events: true,
                    ..Default::default()
                },
            )
            .await?;

        // Sort events by start time
        let mut busy_slots: Vec<(DateTime<Utc>, DateTime<Utc>)> = events
            .into_iter()
            .filter_map(|e| {
                let start = e.start.to_datetime()?;
                let end = e.end.to_datetime()?;
                Some((start, end))
            })
            .collect();
        busy_slots.sort_by_key(|(s, _)| *s);

        // Find first available slot
        let mut current = after;

        for (busy_start, busy_end) in busy_slots {
            if current + duration <= busy_start {
                // Found a gap
                return Ok(Some(current));
            }
            // Move current to after this busy slot
            if busy_end > current {
                current = busy_end;
            }
        }

        // Check if there's space at the end
        if current + duration <= end {
            Ok(Some(current))
        } else {
            Ok(None)
        }
    }

    /// Quick create event with essential fields
    pub async fn quick_create_event(
        &self,
        calendar_id: &str,
        title: impl Into<String>,
        start: DateTime<Utc>,
        duration_minutes: i64,
    ) -> Result<CalendarEvent> {
        let end = start + Duration::minutes(duration_minutes);

        let event = CalendarEvent {
            id: None,
            title: title.into(),
            description: None,
            location: None,
            start: EventTime::from_datetime(start),
            end: EventTime::from_datetime(end),
            attendees: vec![],
            status: EventStatus::Confirmed,
            visibility: Visibility::Default,
            recurrence: None,
            conference_data: None,
            reminders: None,
            created: None,
            updated: None,
            creator: None,
            color_id: None,
            html_link: None,
        };

        self.create_event(calendar_id, event).await
    }
}

/// Builder for creating calendar events
pub struct CalendarEventBuilder {
    event: CalendarEvent,
}

impl CalendarEventBuilder {
    /// Create new builder with title
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            event: CalendarEvent {
                id: None,
                title: title.into(),
                description: None,
                location: None,
                start: EventTime::from_datetime(Utc::now()),
                end: EventTime::from_datetime(Utc::now() + Duration::hours(1)),
                attendees: vec![],
                status: EventStatus::Confirmed,
                visibility: Visibility::Default,
                recurrence: None,
                conference_data: None,
                reminders: None,
                created: None,
                updated: None,
                creator: None,
                color_id: None,
                html_link: None,
            },
        }
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.event.description = Some(desc.into());
        self
    }

    /// Set location
    pub fn location(mut self, loc: impl Into<String>) -> Self {
        self.event.location = Some(loc.into());
        self
    }

    /// Set start time
    pub fn start(mut self, start: DateTime<Utc>) -> Self {
        self.event.start = EventTime::from_datetime(start);
        self
    }

    /// Set end time
    pub fn end(mut self, end: DateTime<Utc>) -> Self {
        self.event.end = EventTime::from_datetime(end);
        self
    }

    /// Set duration from start
    pub fn duration_minutes(mut self, minutes: i64) -> Self {
        let start = self.event.start.to_datetime().unwrap_or_else(Utc::now);
        self.event.end = EventTime::from_datetime(start + Duration::minutes(minutes));
        self
    }

    /// Add attendee
    pub fn add_attendee(mut self, email: impl Into<String>, display_name: Option<String>) -> Self {
        self.event.attendees.push(Attendee {
            email: email.into(),
            display_name,
            response_status: ResponseStatus::NeedsAction,
            optional: false,
        });
        self
    }

    /// Add optional attendee
    pub fn add_optional_attendee(
        mut self,
        email: impl Into<String>,
        display_name: Option<String>,
    ) -> Self {
        self.event.attendees.push(Attendee {
            email: email.into(),
            display_name,
            response_status: ResponseStatus::NeedsAction,
            optional: true,
        });
        self
    }

    /// Set visibility
    pub fn visibility(mut self, visibility: Visibility) -> Self {
        self.event.visibility = visibility;
        self
    }

    /// Add Google Meet conference
    pub fn add_google_meet(mut self) -> Self {
        self.event.conference_data = Some(ConferenceData {
            conference_id: None,
            conference_solution: Some(ConferenceSolution {
                key: ConferenceSolutionKey {
                    type_: "hangoutsMeet".to_string(),
                },
                name: "Google Meet".to_string(),
            }),
            entry_points: vec![],
        });
        self
    }

    /// Set reminders
    pub fn reminders(mut self, use_default: bool, overrides: Vec<ReminderOverride>) -> Self {
        self.event.reminders = Some(Reminders {
            use_default,
            overrides: if overrides.is_empty() {
                None
            } else {
                Some(overrides)
            },
        });
        self
    }

    /// Add popup reminder
    pub fn add_popup_reminder(mut self, minutes_before: i32) -> Self {
        let mut reminders = self.event.reminders.unwrap_or_else(|| Reminders {
            use_default: false,
            overrides: Some(vec![]),
        });

        if let Some(ref mut overrides) = reminders.overrides {
            overrides.push(ReminderOverride {
                method: "popup".to_string(),
                minutes: minutes_before,
            });
        }

        self.event.reminders = Some(reminders);
        self
    }

    /// Build the event
    pub fn build(self) -> CalendarEvent {
        self.event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_time_from_datetime() {
        let now = Utc::now();
        let et = EventTime::from_datetime(now);
        assert_eq!(et.date_time, Some(now));
        assert_eq!(et.time_zone, Some("UTC".to_string()));
    }

    #[test]
    fn test_event_builder() {
        let event = CalendarEventBuilder::new("Test Event")
            .description("Test description")
            .location("Test location")
            .duration_minutes(30)
            .add_attendee("test@example.com", Some("Test User".to_string()))
            .add_popup_reminder(15)
            .build();

        assert_eq!(event.title, "Test Event");
        assert_eq!(event.description, Some("Test description".to_string()));
        assert_eq!(event.location, Some("Test location".to_string()));
        assert_eq!(event.attendees.len(), 1);
    }
}
