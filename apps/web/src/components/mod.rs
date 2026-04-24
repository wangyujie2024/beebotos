pub mod browser;
pub mod charts;
pub mod chat_search;
pub mod command_palette;
pub mod error_boundary;
pub mod footer;
pub mod guard;
pub mod info_item;
pub mod loading;
pub mod modal;
pub mod nav;
pub mod pagination;
pub mod security;
pub mod sidebar;
pub mod star_rating;
pub mod topbar;
pub mod webchat;
pub mod wizard;

pub use charts::{BarChart, PieChart};
pub use chat_search::{
    AdvancedChatSearch, ChatSearch, ExportOptions, MessageExportDialog, PinnedMessage,
    PinnedMessagesPanel, SearchResult, SlashCommand, SlashCommandInput,
};
pub use command_palette::{CommandPalette, CommandPaletteButton};
pub use error_boundary::{
    use_error_context, AsyncHandler, ErrorBoundary, ErrorContext, ErrorMessage, GlobalErrorHandler,
};
pub use footer::Footer;
pub use guard::{AccessDenied, AuthGuard, GuestOnly};
pub use info_item::InfoItem;
pub use loading::{
    CardSkeleton, FadeIn, InlineLoading, ListItemSkeleton, PageLoading, ProgressiveLoading,
    ShimmerPlaceholder, SkeletonGrid, StatsCardSkeleton, TableSkeleton,
};
pub use modal::Modal;
pub use nav::Nav;
pub use pagination::{LoadMoreTrigger, PageSizeSelector, Pagination, PaginationState, VirtualList};
pub use security::{ContentSecurityPolicy, SanitizedText, SecureImage, SecureLink};
pub use sidebar::Sidebar;
pub use star_rating::StarRating;
pub use topbar::TopBar;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_exports() {
        let _ = Nav;
        let _ = Footer;
    }
}
