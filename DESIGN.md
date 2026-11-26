---
version: alpha
name: TraceDraw
description: A native vector drawing workspace with compact tools and quiet document chrome.
colors:
  primary: "#0078D7"
  panelLight: "#F4F4F4"
  fieldLight: "#FFFFFF"
  textLight: "#1E1E1E"
  selectedRowLight: "#CCE8FF"
typography:
  sans:
    fontFamily: "Selawik, script-specific system fallbacks"
rounded:
  control: "2px"
omitted:
  - section: spacing
    reason: "Existing egui styles and the documented Options layout own spacing; this feature introduces no global scale."
  - section: components
    reason: "Canonical Rust component ownership is recorded below instead of duplicating component tokens."
---

# TraceDraw interface design

TraceDraw is a Rust and egui drawing application for desktop and browser.
The drawing is the main surface. Surrounding tools use compact rows, clear
labels, thin borders and restrained selection colour, like a working drawing
desk with its instruments kept within reach. Settings should make a precise
choice easy without adding decorative containers or competing accents.

This document records the existing interface; it does not introduce a new
theme. The canonical token source remains
[`apps/tracedraw/src/theme.rs`](apps/tracedraw/src/theme.rs). Its `Tokens`,
theme palettes and `visuals` adapter feed egui controls and the application
chrome. The frontmatter mirrors the light palette, `Tokens::ACCENT`, bundled
UI font and control radius. Other themes use the same semantic tokens.
Changes to those values must update this record and the canonical source
together. Never hard-code the light palette in a feature.

The interface supports Light, Medium Light, Medium Dark, Dark and High
Contrast. Selection, keyboard focus and disabled state must remain readable
in each. Use `Tokens::text`, `text_dim`, `selected_row` and related semantic
roles; do not substitute an arbitrary colour for a control state.

Selawik is the bundled UI typeface, with its bold face for emphasis. The
script fallbacks and supported locales are maintained in
[`docs/i18n.md`](docs/i18n.md) and `theme::UI_SCRIPT_FONTS`. All user-visible
control labels and tooltips go through the existing translation tables.
Locale support does not imply a market-specific product policy.

## Canonical controls and layout

- egui widgets with the application theme own checkboxes, text fields and
  buttons. Keep keyboard focus and native widget behavior.
- `ui::keynav::ComboBox` owns keyboard-accessible choice lists. Reuse its
  popup behavior for menu-location selection.
- `ui::tips::RichTip` owns tooltips. Icon-only actions need translated names.
- `ui::chrome` and `ui::options` own dialog framing, navigation, confirmation
  actions and the save/cancel snapshot. Preserve that shared behavior.
- egui `ScrollArea` owns scrolling. Long settings lists scroll within the
  page; they must not push OK or Cancel outside the dialog.
- `ui::icons` owns the application's icon vocabulary. Use existing symbols
  and simple drawn disclosure/drag markers rather than decorative imagery.

[`docs/behavior/options.md`](docs/behavior/options.md) remains the detailed
Options layout contract. The context-menu editor follows the neighbouring
customization pages and adapts within that dialog at narrow widths. Control
labels wrap or shorten without hiding their meaning; full names remain
available in tooltips.

## Context-menu editing

[`docs/behavior/context-menus.md`](docs/behavior/context-menus.md) owns the
feature's behavior. A tree row expresses a real menu relationship:
disclosure control, visibility checkbox, translated label and reorder
controls. Indentation represents menu depth. Searching retains parent paths
so a matching child remains understandable.

Dragging changes the order of siblings, with a visible insertion cue.
Moving a branch preserves its children. Up and down controls provide the
same operation for keyboard and single-pointer use. A hidden parent is
identified explicitly rather than through colour alone. Keep the underlying
child choices available for editing.

Visibility and order share the existing Options snapshot: OK keeps both,
Cancel restores both. Restore actions return both to their defaults. Menu
actions retain their normal command paths. No animation or repaint loop is
needed while this list is idle.

Verify hierarchy, drag and button ordering, search, keyboard use, restore and
cancel with the project tests. Inspect rendered light/dark, translated and
narrow-window states before claiming the interface is complete.
