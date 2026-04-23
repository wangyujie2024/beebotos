pub mod browser;
pub mod charts;
pub mod chat_search;
pub mod command_palette;
pub mod error_boundary;
pub mod footer;
pub mod guard;
pub mod loading;
pub mod info_item;
pub mod modal;
pub mod nav;
pub mod star_rating;
pub mod pagination;
pub mod security;
pub mod sidebar;
pub mod topbar;
pub mod webchat;
pub mod wizard;

pub use chat_search::{
    AdvancedChatSearch, ChatSearch, ExportOptions, MessageExportDialog, PinnedMessage,
    PinnedMessagesPanel, SearchResult, SlashCommand, SlashCommandInput,
};
pub use command_palette::{CommandPalette, CommandPaletteButton};

pub use error_boundary::{
    use_error_context, AsyncHandler, ErrorBoundary, ErrorContext, ErrorMessage, GlobalErrorHandler,
};
pub use charts::{BarChart, PieChart};
pub use footer::Footer;
pub use info_item::InfoItem;
pub use guard::{
    use_any_permission_check, use_permission_check, use_protected_action, use_role_check,
    AccessDenied, AdminGuard, AllPermissionsGuard, AnyPermissionGuard, AnyRoleGuard, AuthGuard,
    CombinedGuard, GuestOnly, OperatorGuard, PermissionGuard, PermissionShow, RoleGuard, RoleShow,
};
pub use loading::{
    CardSkeleton, FadeIn, InlineLoading, ListItemSkeleton, PageLoading, ProgressiveLoading,
    ShimmerPlaceholder, SkeletonGrid, StatsCardSkeleton, TableSkeleton,
};
pub use nav::Nav;
pub use pagination::{LoadMoreTrigger, PageSizeSelector, Pagination, PaginationState, VirtualList};
pub use sidebar::Sidebar;
pub use star_rating::StarRating;
pub use topbar::TopBar;
pub use modal::Modal;
pub use security::{ContentSecurityPolicy, SanitizedText, SecureImage, SecureLink};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_exports() {
        let _ = Nav;
        let _ = Footer;
    }
}
