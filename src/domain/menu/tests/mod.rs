mod enablement;
mod layout;

use super::*;

fn editing() -> MenuContext {
    MenuContext {
        focused: WindowId::Editor,
        has_project: true,
        has_open_tab: true,
        active_tab_editable: true,
        can_comment: true,
        ..MenuContext::default()
    }
}

fn global(ctx: &MenuContext) -> MenuModel {
    MenuModel::build(ctx, MenuSurface::Global)
}
