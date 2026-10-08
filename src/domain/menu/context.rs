use super::WindowId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuContext {
    pub focused: WindowId,
    pub has_project: bool,
    pub recents: Vec<String>,
    pub other_widget_focused: bool,

    pub has_open_tab: bool,
    pub active_tab_editable: bool,
    pub active_tab_dirty: bool,
    pub any_tab_dirty: bool,
    pub has_selection: bool,
    pub can_paste: bool,
    pub can_comment: bool,

    pub build_running: bool,
    pub updates_available: bool,

    pub simulator_open: bool,
    pub graph_open: bool,
    pub doom_open: bool,
    pub has_audio: bool,
    pub is_playing: bool,
    pub is_bypassed: bool,
}

impl Default for MenuContext {
    fn default() -> Self {
        Self {
            focused: WindowId::Splash,
            has_project: false,
            recents: Vec::new(),
            other_widget_focused: false,
            has_open_tab: false,
            active_tab_editable: false,
            active_tab_dirty: false,
            any_tab_dirty: false,
            has_selection: false,
            can_paste: false,
            can_comment: false,
            build_running: false,
            updates_available: false,
            simulator_open: false,
            graph_open: false,
            doom_open: false,
            has_audio: false,
            is_playing: false,
            is_bypassed: false,
        }
    }
}
