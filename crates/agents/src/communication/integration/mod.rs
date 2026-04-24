//! Third-party Service Integrations
//!
//! Provides integrations with external services like Google Calendar,
//! Google Drive, and other enterprise platforms.

pub mod google_calendar;

pub use google_calendar::{
    Attendee, CalendarEvent, CalendarEventBuilder, CalendarFreeBusy, CalendarItem,
    CalendarListEntry, ConferenceData, ConferenceSolution, ConferenceSolutionKey, Creator,
    EntryPoint, EventQuery, EventStatus, EventTime, FreeBusyRequest, FreeBusyResponse,
    GoogleCalendarClient, ReminderOverride, Reminders, ResponseStatus, TimeSlot, Visibility,
};
