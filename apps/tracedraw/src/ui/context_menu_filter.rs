//! Filters only the right-click menu currently being drawn.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::marker::PhantomData;
use std::rc::Rc;

pub(crate) use crate::settings::ContextMenuPreferences as Preferences;

#[derive(Default)]
struct Separators {
    has_item: bool,
    pending: bool,
}

struct State {
    scene: String,
    hidden: BTreeSet<String>,
    orders: BTreeMap<String, Vec<String>>,
    path: String,
    bodies: HashMap<egui::Id, Separators>,
    content: Option<Rc<Cell<bool>>>,
}

thread_local! {
    static SCOPES: RefCell<Vec<State>> = const { RefCell::new(Vec::new()) };
    static POPUP_CONTENT: RefCell<Vec<Rc<Cell<bool>>>> = const { RefCell::new(Vec::new()) };
}

/// Counts actual rows while one customized popup is drawn, including disabled
/// rows and dynamic targets. Closed and default popups never create this guard.
pub(crate) struct PopupContent {
    has_item: Rc<Cell<bool>>,
}

impl PopupContent {
    pub(crate) fn new() -> Self {
        let has_item = Rc::new(Cell::new(false));
        POPUP_CONTENT.with(|content| content.borrow_mut().push(Rc::clone(&has_item)));
        Self { has_item }
    }

    pub(crate) fn has_item(&self) -> bool {
        self.has_item.get()
    }
}

impl Drop for PopupContent {
    fn drop(&mut self) {
        POPUP_CONTENT.with(|content| {
            content.borrow_mut().pop();
        });
    }
}

/// Owns a snapshot, so menu actions can freely change the app's settings.
/// The innermost scope applies, and leaving it restores the enclosing menu.
pub(crate) struct Scope {
    _thread: PhantomData<Rc<()>>,
}

impl Scope {
    pub(crate) fn new(preferences: &Preferences, menu: &str) -> Self {
        let content = POPUP_CONTENT.with(|content| content.borrow().last().cloned());
        SCOPES.with(|scopes| {
            scopes.borrow_mut().push(State {
                scene: menu.to_owned(),
                hidden: preferences.hidden_items(menu),
                orders: preferences.menu_orders(menu),
                path: String::new(),
                bodies: HashMap::new(),
                content,
            });
        });
        Self {
            _thread: PhantomData,
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        SCOPES.with(|scopes| {
            scopes.borrow_mut().pop();
        });
    }
}

/// The submenu occurrence being drawn. A full path distinguishes repeated
/// action keys under different parents without changing command identities.
pub(crate) struct Branch {
    scope: usize,
    previous_len: usize,
}

impl Branch {
    pub(crate) fn new(key: &str) -> Self {
        SCOPES.with(|scopes| {
            let mut scopes = scopes.borrow_mut();
            if scopes.last().is_none_or(|state| state.orders.is_empty()) {
                return Self {
                    scope: usize::MAX,
                    previous_len: 0,
                };
            }
            let scope = scopes.len().saturating_sub(1);
            let previous_len = scopes.last().map_or(0, |state| state.path.len());
            if let Some(state) = scopes.last_mut() {
                if !state.path.is_empty() {
                    state.path.push('/');
                }
                state.path.push_str(key);
            }
            Self {
                scope,
                previous_len,
            }
        })
    }
}

impl Drop for Branch {
    fn drop(&mut self) {
        SCOPES.with(|scopes| {
            if let Some(state) = scopes.borrow_mut().get_mut(self.scope) {
                state.path.truncate(self.previous_len);
            }
        });
    }
}

/// Draw each available sibling exactly once, in its configured order. Unknown
/// saved keys are ignored and new actions retain their relative default order.
/// The usual path borrows the caller's slice and allocates nothing.
pub(crate) fn ordered_keys<'a>(keys: &'a [&'a str]) -> Cow<'a, [&'a str]> {
    if super::menus::invoke::active() {
        return Cow::Borrowed(keys);
    }
    SCOPES.with(|scopes| {
        let scopes = scopes.borrow();
        let Some(order) = scopes
            .last()
            .and_then(|state| state.orders.get(&state.path))
        else {
            return Cow::Borrowed(keys);
        };
        let mut keys = keys.to_vec();
        keys.sort_by_key(|key| {
            order
                .iter()
                .position(|item| item == key)
                .unwrap_or(usize::MAX)
        });
        Cow::Owned(keys)
    })
}

/// Keeps rendering and command execution together while ordering sibling rows.
/// Blocks are never replayed to measure or enumerate a popup.
macro_rules! ordered_menu {
    ($ui:ident; fixed [$($fixed:expr),* $(,)?]; $($key:expr => $body:block $(,)?)*) => {{
        let menu_keys = [$($key),*];
        for menu_key in $crate::ui::context_menu_filter::ordered_keys_with_fixed(&menu_keys, &[$($fixed),*]).iter().copied() {
            $(if menu_key == $key { $body; continue; })*
        }
    }};
    ($ui:ident; $($key:expr => $body:block $(,)?)*) => {{
        let menu_keys = [$($key),*];
        for menu_key in $crate::ui::context_menu_filter::ordered_keys(&menu_keys).iter().copied() {
            $(if menu_key == $key { $body; continue; })*
        }
    }};
}
pub(crate) use ordered_menu;

/// Dynamic target groups retain their slots while the editable static siblings
/// move around them. Their individual targets belong to the document/library.
pub(crate) fn ordered_keys_with_fixed<'a>(
    keys: &'a [&'a str],
    fixed: &[&str],
) -> Cow<'a, [&'a str]> {
    let ordered = ordered_keys(keys);
    if matches!(ordered, Cow::Borrowed(_)) {
        return ordered;
    }
    let mut movable = ordered.iter().copied().filter(|key| !fixed.contains(key));
    Cow::Owned(
        keys.iter()
            .map(|key| {
                if fixed.contains(key) {
                    *key
                } else {
                    movable.next().unwrap_or(key)
                }
            })
            .collect(),
    )
}

/// Queries visibility without allocating a row or changing separator state.
/// Command enumeration and execution always retain every action.
pub(crate) fn is_visible(key: &str) -> bool {
    if super::menus::invoke::active() {
        return true;
    }
    SCOPES.with(|scopes| {
        scopes
            .borrow()
            .last()
            .is_none_or(|scope| !scope.hidden.contains(key))
    })
}

/// A static submenu disappears when every descendant action is hidden.
/// Dynamic target lists report their actual presence without running their body.
pub(crate) fn submenu_visible(key: &str, dynamic: bool) -> bool {
    if super::menus::invoke::active() {
        return true;
    }
    SCOPES.with(|scopes| {
        scopes.borrow().last().is_none_or(|scope| {
            !scope.hidden.contains(key)
                && super::context_menu_branches::has_visible_children(
                    &scope.scene,
                    key,
                    &scope.hidden,
                    dynamic,
                )
        })
    })
}

/// Called immediately before drawing a visible item, including native widgets.
pub(crate) fn show_item(ui: &mut egui::Ui, key: &str) -> bool {
    if super::menus::invoke::active() {
        return true;
    }
    if !is_visible(key) {
        return false;
    }
    let paint_separator = SCOPES.with(|scopes| {
        let mut scopes = scopes.borrow_mut();
        let Some(scope) = scopes.last_mut() else {
            return false;
        };
        if let Some(content) = &scope.content {
            content.set(true);
        }
        let body = scope.bodies.entry(ui.id()).or_default();
        let paint = body.has_item && body.pending;
        body.has_item = true;
        body.pending = false;
        paint
    });
    // Painting happens outside the thread-local borrow.
    if paint_separator {
        super::menus::paint_separator(ui);
    }
    true
}

/// Returns true when this scope handled the separator. A line is drawn only
/// before a later visible item in this same UI, so empty groups leave no lines.
pub(crate) fn defer_separator(ui: &egui::Ui) -> bool {
    if super::menus::invoke::active() {
        return true;
    }
    SCOPES.with(|scopes| {
        let mut scopes = scopes.borrow_mut();
        let Some(scope) = scopes.last_mut() else {
            return false;
        };
        let body = scope.bodies.entry(ui.id()).or_default();
        body.pending |= body.has_item;
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ui: &mut egui::Ui, key: &str) -> Option<egui::Rect> {
        show_item(ui, key).then(|| {
            ui.allocate_exact_size(egui::vec2(100.0, 10.0), egui::Sense::hover())
                .0
        })
    }

    #[test]
    fn default_and_out_of_scope_rows_stay_visible() {
        assert!(is_visible("action"));
        {
            let _scope = Scope::new(&Preferences::default(), "canvas");
            assert!(is_visible("action"));
            assert!(is_visible("new.action"));
        }
        assert!(is_visible("action"));
    }

    #[test]
    fn nested_scenes_restore_the_outer_filter_without_borrowing_preferences() {
        let mut preferences = Preferences::default();
        preferences.set_visible("outer", "action", false);
        let outer = Scope::new(&preferences, "outer");
        preferences.set_visible("outer", "action", true);
        assert!(!is_visible("action"));
        {
            let _inner = Scope::new(&preferences, "other");
            assert!(is_visible("action"));
        }
        assert!(!is_visible("action"));
        drop(outer);
        assert!(is_visible("action"));
        let _next_app = Scope::new(&Preferences::default(), "outer");
        assert!(is_visible("action"));
    }

    #[test]
    fn menu_scopes_do_not_cross_threads() {
        let mut preferences = Preferences::default();
        preferences.set_visible("canvas", "action", false);
        let _scope = Scope::new(&preferences, "canvas");
        std::thread::spawn(move || {
            assert!(is_visible("action"));
            let _scope = Scope::new(&preferences, "canvas");
            assert!(!is_visible("action"));
        })
        .join()
        .expect("menu thread");
        assert!(!is_visible("action"));
    }

    #[test]
    fn hidden_groups_leave_no_leading_trailing_or_repeated_separators() {
        let context = crate::theme::ui_context();
        let mut preferences = Preferences::default();
        preferences.set_visible("canvas", "hidden", false);
        let mut output = context.run_ui(Default::default(), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let _scope = Scope::new(&preferences, "canvas");
            let top = ui.cursor().top();
            super::super::menus::sep(ui);
            assert!(row(ui, "hidden").is_none());
            super::super::menus::sep(ui);
            let first = row(ui, "first").expect("first row");
            assert_eq!(first.top(), top);
            super::super::menus::sep(ui);
            assert!(row(ui, "hidden").is_none());
            super::super::menus::sep(ui);
            let second = row(ui, "second").expect("second row");
            assert_eq!(second.top() - first.bottom(), 7.0);
            let bottom = ui.cursor().top();
            super::super::menus::sep(ui);
            assert!(row(ui, "hidden").is_none());
            assert_eq!(ui.cursor().top(), bottom);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn submenu_separator_state_is_independent_of_the_parent_ui() {
        let context = crate::theme::ui_context();
        let mut output = context.run_ui(Default::default(), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let _scope = Scope::new(&Preferences::default(), "canvas");
            let _ = row(ui, "parent");
            super::super::menus::sep(ui);
            ui.push_id("submenu", |ui| {
                let top = ui.cursor().top();
                super::super::menus::sep(ui);
                let child = row(ui, "child").expect("child row");
                assert_eq!(child.top(), top);
            });
        });
        output.textures_delta.clear();
    }

    #[test]
    fn ordinary_menus_keep_their_immediate_separators() {
        let context = crate::theme::ui_context();
        let mut output = context.run_ui(Default::default(), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let top = ui.cursor().top();
            super::super::menus::sep(ui);
            assert_eq!(ui.cursor().top() - top, 7.0);
        });
        output.textures_delta.clear();
    }
}

#[cfg(test)]
mod heading_separator_tests {
    use super::*;

    #[test]
    fn a_visible_group_heading_consumes_its_separator_before_the_heading() {
        let context = crate::theme::ui_context();
        let mut output = context.run_ui(Default::default(), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let _scope = Scope::new(&Preferences::default(), "objects_object");
            assert!(show_item(ui, "docker.move_to_layer"));
            let previous = ui
                .allocate_exact_size(egui::vec2(100.0, 10.0), egui::Sense::hover())
                .0;
            super::super::menus::sep(ui);
            assert!(show_item(ui, "docker.move_to_layer"));
            let heading = ui.label("Next page").rect;
            assert_eq!(heading.top() - previous.bottom(), 7.0);
            assert!(show_item(ui, "docker.move_to_layer"));
            let target = ui
                .allocate_exact_size(egui::vec2(100.0, 10.0), egui::Sense::hover())
                .0;
            assert_eq!(target.top(), heading.bottom());
        });
        output.textures_delta.clear();
    }
}

#[cfg(test)]
mod ordering_tests {
    use super::*;

    #[test]
    fn context_menu_order_default_borrows_without_allocating() {
        let keys = ["first", "second"];
        assert!(matches!(ordered_keys(&keys), Cow::Borrowed(_)));
        let _scope = Scope::new(&Preferences::default(), "scene");
        assert!(matches!(ordered_keys(&keys), Cow::Borrowed(_)));
    }

    #[test]
    fn context_menu_order_uses_full_parent_paths_and_restores_nested_scopes() {
        let mut preferences = Preferences::default();
        preferences.set_order("scene", "", &["third", "first"]);
        preferences.set_order("scene", "parent", &["second", "first"]);
        preferences.set_order("scene", "parent/child", &["first", "third"]);
        let keys = ["first", "second", "third", "new"];
        let _scope = Scope::new(&preferences, "scene");
        preferences.reset_all();
        assert_eq!(&*ordered_keys(&keys), &["third", "first", "second", "new"]);
        {
            let _parent = Branch::new("parent");
            assert_eq!(&*ordered_keys(&keys), &["second", "first", "third", "new"]);
            {
                let _child = Branch::new("child");
                assert_eq!(&*ordered_keys(&keys), &["first", "third", "second", "new"]);
                let _nested = Scope::new(&Preferences::default(), "other");
                assert_eq!(&*ordered_keys(&keys), &keys);
            }
            assert_eq!(&*ordered_keys(&keys), &["second", "first", "third", "new"]);
        }
        assert_eq!(&*ordered_keys(&keys), &["third", "first", "second", "new"]);
    }

    #[test]
    fn context_menu_order_executes_each_keyed_block_once() {
        let mut preferences = Preferences::default();
        preferences.set_order("scene", "", &["third", "first", "missing"]);
        let _scope = Scope::new(&preferences, "scene");
        let mut visited = Vec::new();
        ordered_menu! { ui;
            "first" => { visited.push("first"); },
            "second" => { visited.push("second"); },
            "third" => { visited.push("third"); },
        }
        assert_eq!(visited, ["third", "first", "second"]);
    }

    #[test]
    fn context_menu_order_static_children_preserve_dynamic_group_slot() {
        let mut preferences = Preferences::default();
        preferences.set_order(
            "canvas_object",
            "docker.object_styles",
            &[
                "docker.break_style_link",
                "docker.new_style_from_selection",
                "docker.open_object_styles",
            ],
        );
        let _scope = Scope::new(&preferences, "canvas_object");
        let _branch = Branch::new("docker.object_styles");
        let keys = [
            "docker.new_style_from_selection",
            "docker.object_styles",
            "docker.break_style_link",
            "docker.open_object_styles",
        ];
        assert_eq!(
            &*ordered_keys_with_fixed(&keys, &["docker.object_styles"]),
            &[
                "docker.break_style_link",
                "docker.object_styles",
                "docker.new_style_from_selection",
                "docker.open_object_styles"
            ]
        );
    }
}
