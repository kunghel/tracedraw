//! Bars longer than the room they get. The bar shows what fits; a "»"
//! button at its end brings the next items into view (the bar slides,
//! cut between items) and a "«" button at its start brings the first
//! ones back; the mouse wheel over the bar slides it too, and on a touch
//! screen a finger dragged along the bar slides it with the finger
//! ([`finger_slide`], #1997). Every item stays where it is in the bar and
//! works as there: the bar is drawn once. The toolbox on a side does the
//! same up and down. See docs/behavior/toolbars.md.

use crate::ui::tips::RichTip;
use egui::{Align, Id, LayerId, Layout, Pos2, Rect, Sense, Ui, UiBuilder, Vec2};

/// The size of the « and » buttons across the bar.
pub const BUTTON: f32 = 18.0;

/// What a bar measured in the frame before.
#[derive(Debug, Clone, Default, PartialEq)]
struct State {
    /// The bar's whole length.
    content: f32,
    /// How far the bar is slid.
    offset: f32,
    /// The items' extents along the bar, from its start.
    items: Vec<(f32, f32)>,
    /// The finger sliding the bar, while one does.
    grab: Option<Grab>,
}

/// A finger pressed on a bar: where, and what it turned out to be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grab {
    /// Where the finger came down.
    origin: Pos2,
    /// Once the finger has moved past a tap's distance: whether it
    /// slides the bar (it moved along the bar rather than across it).
    slides: Option<bool>,
    /// Where along the bar the slide began, and the bar's position then.
    anchor: Option<(f32, f32)>,
}

/// The id under which the bar a finger slides is kept while it does.
fn sliding_id() -> Id {
    Id::new("finger_slides_bar")
}

/// True while a finger slides a bar with [`finger_slide`], and in the
/// frame it lifts: the bar's items start no drag of their own then (a
/// tool button opens no flyout, a number field's label steps no value, a
/// docker tab does not move, a swatch drags no colour), and the item
/// under the finger when it lifts takes no click (egui clicks nothing
/// after a move that far; an item that takes a slipped release as a
/// click checks this). Costs an input read with a mouse.
pub fn finger_sliding(ctx: &egui::Context) -> bool {
    let finger = ctx.input(|i| {
        i.has_touch_screen()
            && (i.any_touches()
                || i.raw
                    .events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Touch { .. })))
    });
    finger && ctx.data(|d| d.get_temp::<Id>(sliding_id())).is_some()
}

/// A finger dragged along a bar slides the bar with it (#1997). Called
/// each frame before the bar's items are drawn, with the bar's `room` on
/// the screen, whether it runs `horizontal`ly, and `at`, its position
/// (how far it is slid, or scrolled) this frame; `grab` is kept by the
/// caller between frames. While a finger that came down in the room and
/// on the bar's own layer (not on a menu or a flyout over it) has moved
/// past a tap's distance, more along the bar than across it, the bar's
/// new position comes back: where it was when the slide began, less the
/// finger's movement since, so the bar sticks to the finger. A tap, a
/// finger that moved across the bar (a colour dragged off a palette, a
/// tool button dragged sideways for its flyout) and the mouse give
/// `None`, and the bar's items keep their own clicks and drags. Nothing
/// is done, and nothing read but a flag, until the screen has been
/// touched.
pub fn finger_slide(
    ui: &Ui,
    id: Id,
    grab: &mut Option<Grab>,
    room: Rect,
    horizontal: bool,
    at: f32,
) -> Option<f32> {
    let finger = ui.input(|i| {
        if i.has_touch_screen() && i.any_touches() && i.pointer.primary_down() {
            i.pointer.press_origin().zip(i.pointer.latest_pos())
        } else {
            None
        }
    });
    let along = |p: Pos2| if horizontal { p.x } else { p.y };
    let across = |p: Pos2| if horizontal { p.y } else { p.x };
    let Some((origin, latest)) = finger else {
        // No finger down: a slide that was going ends. In the frame the
        // finger lifts the slide still shows to the items, so the one
        // under the finger takes no click; the flag goes the frame after.
        if let Some(g) = grab.take().filter(|g| g.slides == Some(true)) {
            if ui.input(|i| i.pointer.primary_released()) {
                *grab = Some(Grab { anchor: None, ..g });
            } else {
                ui.data_mut(|d| {
                    if d.get_temp::<Id>(sliding_id()) == Some(id) {
                        d.remove::<Id>(sliding_id());
                    }
                });
            }
        }
        return None;
    };
    let g = match grab {
        Some(g) if g.origin == origin => g,
        _ => {
            // A new press: in the room, and on the bar itself.
            if !room.contains(origin) {
                *grab = None;
                return None;
            }
            let layer = ui
                .ctx()
                .layer_id_at(origin)
                .unwrap_or(LayerId::background());
            if layer != ui.layer_id() {
                *grab = None;
                return None;
            }
            // A slide left over from a bar gone since does not count.
            ui.data_mut(|d| d.remove::<Id>(sliding_id()));
            grab.insert(Grab {
                origin,
                slides: None,
                anchor: None,
            })
        }
    };
    if g.slides.is_none() {
        let d = latest - origin;
        if d.length() > ui.ctx().options(|o| o.input_options.max_click_dist) {
            let slides =
                (along(latest) - along(origin)).abs() >= (across(latest) - across(origin)).abs();
            g.slides = Some(slides);
            if slides {
                g.anchor = Some((along(latest), at));
                ui.data_mut(|d| d.insert_temp(sliding_id(), id));
            }
        }
    }
    match (g.slides, g.anchor) {
        (Some(true), Some((from, was))) => Some(was - (along(latest) - from)),
        _ => None,
    }
}

/// Areas that hold other items (a bar's own hover area, a group) are not
/// items: their children are.
fn items_only(items: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = items
        .iter()
        .copied()
        .filter(|&(a, b)| {
            !items
                .iter()
                .any(|&(c, d)| (c, d) != (a, b) && c >= a - 0.5 && d <= b + 0.5 && d - c < b - a)
        })
        .collect();
    out.sort_by(|x, y| x.0.total_cmp(&y.0));
    out
}

/// The room the items get when the bar is longer than `room`: less the
/// buttons at both ends.
fn view(room: f32) -> f32 {
    (room - 2.0 * BUTTON).max(1.0)
}

/// The largest slide: the bar's end just inside the room.
pub fn max_offset(content: f32, room: f32) -> f32 {
    if content <= room + 0.5 {
        0.0
    } else {
        (content - view(room)).max(0.0)
    }
}

/// The slide after "»": the first item not wholly in view comes first.
pub fn next_offset(items: &[(f32, f32)], content: f32, room: f32, offset: f32) -> f32 {
    let end = offset + view(room);
    let items = items_only(items);
    let next = items
        .iter()
        .find(|&&(a, b)| b > end + 0.5 && a > offset + 0.5)
        .map(|&(a, _)| a)
        .unwrap_or(end);
    next.min(max_offset(content, room))
}

/// The slide after "«": as far back as keeps the first item now in view
/// at the end of the room, cut between items.
pub fn previous_offset(items: &[(f32, f32)], room: f32, offset: f32) -> f32 {
    let target = (offset - view(room)).max(0.0);
    if target <= 0.5 {
        return 0.0;
    }
    let items = items_only(items);
    items
        .iter()
        .map(|&(a, _)| a)
        .find(|&a| a >= target - 0.5)
        .unwrap_or(target)
        .min(offset)
}

/// Draw a horizontal bar with `add`, sliding when it is longer than the
/// room it gets; the bar's whole length.
pub fn row(ui: &mut Ui, id_salt: &str, add: impl FnOnce(&mut Ui)) -> f32 {
    strip(ui, id_salt, true, |ui| {
        add(ui);
        None
    })
}

/// [`row`] whose `add` may return an item's rectangle to bring into view
/// (the bar slides just enough to show it whole).
pub fn row_showing(ui: &mut Ui, id_salt: &str, add: impl FnOnce(&mut Ui) -> Option<Rect>) -> f32 {
    strip(ui, id_salt, true, add)
}

/// The vertical counterpart, for a column of tool buttons.
pub fn column(ui: &mut Ui, id_salt: &str, add: impl FnOnce(&mut Ui)) -> f32 {
    strip(ui, id_salt, false, |ui| {
        add(ui);
        None
    })
}

/// The triangle of a column's scroll button, pointing up (`up`) or down,
/// in the button's text colour: a shape, as no bundled font has the small
/// triangles U+25B4 and U+25BE (#1780).
fn arrow(ui: &Ui, resp: &egui::Response, up: bool) {
    let color = ui.style().interact(resp).fg_stroke.color;
    let c = resp.rect.center();
    let (w, h) = (4.0, 3.0);
    let (tip, base) = if up { (-h, h) } else { (h, -h) };
    let points = vec![
        Pos2::new(c.x, c.y + tip),
        Pos2::new(c.x + w, c.y + base),
        Pos2::new(c.x - w, c.y + base),
    ];
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
}

/// How far the items cut at an end of a long bar fade out.
pub const FADE: f32 = 14.0;

/// The items cut at the start (`back`) or the end (`forward`) of a long
/// bar's `view` fade into the panel's colour, a hint that the bar goes on
/// that way (#3591). Painted over the items, nothing to click.
fn fade_cut_ends(ui: &Ui, view: Rect, horizontal: bool, back: bool, forward: bool) {
    if !view.is_positive() {
        return;
    }
    let solid = ui.visuals().panel_fill;
    let clear = solid.gamma_multiply(0.0);
    let along = if horizontal {
        view.width()
    } else {
        view.height()
    };
    let len = FADE.min(along / 3.0);
    let mut mesh = egui::Mesh::default();
    let mut band = |r: Rect, start: egui::Color32, end: egui::Color32| {
        let i = mesh.vertices.len() as u32;
        let (a, b, c, d) = if horizontal {
            (
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
            )
        } else {
            (
                r.left_top(),
                r.left_bottom(),
                r.right_bottom(),
                r.right_top(),
            )
        };
        // a and d at the start of the band, b and c at its end.
        mesh.colored_vertex(a, start);
        mesh.colored_vertex(b, end);
        mesh.colored_vertex(c, end);
        mesh.colored_vertex(d, start);
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    };
    if back {
        let r = if horizontal {
            Rect::from_min_size(view.min, Vec2::new(len, view.height()))
        } else {
            Rect::from_min_size(view.min, Vec2::new(view.width(), len))
        };
        band(r, solid, clear);
    }
    if forward {
        let r = if horizontal {
            Rect::from_min_max(Pos2::new(view.max.x - len, view.min.y), view.max)
        } else {
            Rect::from_min_max(Pos2::new(view.min.x, view.max.y - len), view.max)
        };
        band(r, clear, solid);
    }
    if !mesh.is_empty() {
        ui.painter().add(egui::Shape::mesh(mesh));
    }
}

/// The wheel turned over a bar, kept from its items while they are drawn
/// ([`hold_wheel_from_items`]) and given back after.
struct HeldWheel {
    events: Vec<egui::Event>,
    smooth: Vec2,
}

impl HeldWheel {
    /// The wheel back where it was, for the bar itself to slide with it.
    fn give_back(self, ui: &Ui) {
        ui.input_mut(|i| {
            i.raw.events.extend(self.events.iter().cloned());
            i.events.extend(self.events);
            i.smooth_scroll_delta += self.smooth;
        });
    }
}

/// Whether the wheel over a bar may reach the item under the pointer: the
/// item has the keyboard focus (the user clicked into it to type) or
/// took the wheel last (a number box gives up the focus when it steps;
/// it keeps the wheel while the pointer stays on it). Shift+wheel never
/// does: it always slides the bar.
fn item_keeps_wheel(ui: &Ui, bar: Id) -> bool {
    let ctx = ui.ctx();
    let owner_id = bar.with("wheel_owner");
    let Some(pointer) = ctx.pointer_hover_pos() else {
        ctx.data_mut(|d| d.remove::<Rect>(owner_id));
        return false;
    };
    if ui.input(|i| i.modifiers.shift) {
        return false;
    }
    // The focused box, its spin arrows beside it counted as part of it.
    let focused = ctx
        .memory(|m| m.focused())
        .and_then(|f| ctx.read_response(f))
        .map(|r| r.rect.expand2(Vec2::new(16.0, 0.0)))
        .filter(|r| r.contains(pointer));
    if let Some(r) = focused {
        ctx.data_mut(|d| d.insert_temp(owner_id, r));
        return true;
    }
    match ctx.data(|d| d.get_temp::<Rect>(owner_id)) {
        Some(r) if r.contains(pointer) && !ui.input(|i| i.pointer.any_pressed()) => true,
        Some(_) => {
            ctx.data_mut(|d| d.remove::<Rect>(owner_id));
            false
        }
        None => false,
    }
}

/// With the pointer over the bar `bar` (its `room` on the screen), the
/// wheel is taken away from the bar's items before they are drawn
/// (#3589), unless the item under the pointer keeps it
/// ([`item_keeps_wheel`]); `None` when nothing was taken. Ctrl+wheel stays
/// (it sizes the bar's buttons).
fn hold_wheel_from_items(ui: &Ui, bar: Id, room: Rect) -> Option<HeldWheel> {
    if !ui.rect_contains_pointer(room) {
        return None;
    }
    let turned = ui.input(|i| {
        i.smooth_scroll_delta != Vec2::ZERO
            || i.raw
                .events
                .iter()
                .any(|e| matches!(e, egui::Event::MouseWheel { .. }))
    });
    if !turned || item_keeps_wheel(ui, bar) {
        return None;
    }
    let plain = |e: &egui::Event| {
        matches!(e, egui::Event::MouseWheel { modifiers, .. }
            if !modifiers.ctrl && !modifiers.command)
    };
    Some(ui.input_mut(|i| {
        let events: Vec<egui::Event> = i.raw.events.iter().filter(|e| plain(e)).cloned().collect();
        i.raw.events.retain(|e| !plain(e));
        i.events.retain(|e| !plain(e));
        let smooth = std::mem::take(&mut i.smooth_scroll_delta);
        HeldWheel { events, smooth }
    }))
}

fn strip(
    ui: &mut Ui,
    id_salt: &str,
    horizontal: bool,
    add: impl FnOnce(&mut Ui) -> Option<Rect>,
) -> f32 {
    let id = ui.make_persistent_id(("bar_overflow", id_salt));
    let mut st: State = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let full = ui.available_rect_before_wrap();
    let along = |v: Vec2| if horizontal { v.x } else { v.y };
    let room = along(full.size()).max(0.0);
    let long = st.content > room + 0.5;
    st.offset = st.offset.clamp(0.0, max_offset(st.content, room));
    let lead = if long { BUTTON } else { 0.0 };
    let shown = if long { view(room) } else { room };
    // Laid out as the bar was, slid by the offset, clipped to the room
    // left between the buttons across the whole panel.
    let panel = ui.clip_rect();
    let (origin, size, clip, layout) = if horizontal {
        (
            Pos2::new(full.min.x + lead - st.offset, full.min.y),
            Vec2::new(100_000.0, full.height().max(1.0)),
            Rect::from_min_max(
                Pos2::new(full.min.x + lead, panel.min.y),
                Pos2::new(full.min.x + lead + shown, panel.max.y),
            ),
            Layout::left_to_right(Align::Min),
        )
    } else {
        (
            Pos2::new(full.min.x, full.min.y + lead - st.offset),
            Vec2::new(full.width().max(1.0), 100_000.0),
            Rect::from_min_max(
                Pos2::new(panel.min.x, full.min.y + lead),
                Pos2::new(panel.max.x, full.min.y + lead + shown),
            ),
            Layout::top_down(Align::Center),
        )
    };
    // A finger dragged along a long bar slides it (#1997): known before
    // the items are drawn, so they start no drags of their own.
    let slid = if long {
        finger_slide(
            ui,
            id,
            &mut st.grab,
            full.intersect(panel),
            horizontal,
            st.offset,
        )
    } else {
        st.grab = None;
        None
    };
    // The wheel over a bar slides the bar wherever the pointer is on it
    // (#3589): its boxes and lists do not see it, so a turn meant to
    // slide the bar changes no font or size. The box being typed in (the
    // one the user clicked into) keeps it while the pointer is over it;
    // Shift+wheel always slides the bar.
    let held = horizontal
        .then(|| hold_wheel_from_items(ui, id, full.intersect(panel)))
        .flatten();
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(Rect::from_min_size(origin, size))
            .layout(layout)
            .id_salt(id.with("bar")),
    );
    child.set_clip_rect(clip.intersect(panel));
    let keep = add(&mut child);
    if let Some(held) = held {
        held.give_back(ui);
    }
    let used = child.min_rect();
    let content = if horizontal {
        used.right() - origin.x
    } else {
        used.bottom() - origin.y
    };
    let layer = child.layer_id();
    let items: Vec<(f32, f32)> = ui.ctx().viewport(|v| {
        v.this_pass
            .widgets
            .get_layer(layer)
            .filter(|w| {
                let c = w.rect.center();
                if horizontal {
                    w.rect.min.x >= origin.x - 0.5
                        && c.y >= used.min.y - 0.5
                        && c.y <= used.max.y + 0.5
                } else {
                    w.rect.min.y >= origin.y - 0.5
                        && c.x >= used.min.x - 0.5
                        && c.x <= used.max.x + 0.5
                }
            })
            .map(|w| {
                if horizontal {
                    (w.rect.min.x - origin.x, w.rect.max.x - origin.x)
                } else {
                    (w.rect.min.y - origin.y, w.rect.max.y - origin.y)
                }
            })
            .collect()
    });
    // The space the bar takes in its panel.
    let taken = if horizontal {
        Rect::from_min_size(
            Pos2::new(full.min.x, used.min.y),
            Vec2::new(content.min(room), used.height().max(1.0)),
        )
    } else {
        Rect::from_min_size(
            Pos2::new(used.min.x, full.min.y),
            Vec2::new(used.width().max(1.0), content.min(room)),
        )
    };
    let bar_resp = ui.allocate_rect(taken, Sense::hover());
    let mut offset = st.offset;
    if content > room + 0.5 {
        if long != (content > room + 0.5) {
            ui.ctx().request_repaint();
        }
        let tr = crate::i18n::tr;
        let (back, forward, back_tip, forward_tip) = if horizontal {
            (
                "\u{ab}",
                "\u{bb}",
                tr("bars.earlier_items"),
                tr("bars.more_items"),
            )
        } else {
            // Drawn as triangles below: no bundled font has the small
            // triangles, and Windows showed empty boxes (#1780).
            ("", "", tr("bars.earlier_items"), tr("bars.more_items"))
        };
        let cross = if horizontal {
            used.height()
        } else {
            used.width()
        }
        .clamp(16.0, 28.0);
        let (back_rect, forward_rect) = if horizontal {
            (
                Rect::from_center_size(
                    Pos2::new(full.min.x + BUTTON / 2.0, used.center().y),
                    Vec2::new(BUTTON, cross),
                ),
                Rect::from_center_size(
                    Pos2::new(full.min.x + room - BUTTON / 2.0, used.center().y),
                    Vec2::new(BUTTON, cross),
                ),
            )
        } else {
            (
                Rect::from_center_size(
                    Pos2::new(used.center().x, full.min.y + BUTTON / 2.0),
                    Vec2::new(cross, BUTTON),
                ),
                Rect::from_center_size(
                    Pos2::new(used.center().x, full.min.y + room - BUTTON / 2.0),
                    Vec2::new(cross, BUTTON),
                ),
            )
        };
        let can_back = offset > 0.5;
        let can_forward = offset < max_offset(content, room) - 0.5;
        // Each button shows only while there is more of the bar its way
        // (#3588), and the items cut at that end fade out, so a bar with
        // more to see never looks complete (#3591).
        let view_rect = if horizontal {
            Rect::from_min_max(
                Pos2::new(full.min.x + BUTTON, used.min.y),
                Pos2::new(full.min.x + BUTTON + view(room), used.max.y),
            )
        } else {
            Rect::from_min_max(
                Pos2::new(used.min.x, full.min.y + BUTTON),
                Pos2::new(used.max.x, full.min.y + BUTTON + view(room)),
            )
        }
        .intersect(panel);
        fade_cut_ends(ui, view_rect, horizontal, can_back, can_forward);
        if can_back {
            let back_resp = ui.put(
                back_rect,
                egui::Button::new(back).frame(false).sense(Sense::click()),
            );
            if !horizontal {
                arrow(ui, &back_resp, true);
            }
            if back_resp.rich_tip(back_tip).clicked() {
                offset = previous_offset(&items, room, offset);
            }
        }
        if can_forward {
            let forward_resp = ui.put(
                forward_rect,
                egui::Button::new(forward)
                    .frame(false)
                    .sense(Sense::click()),
            );
            if !horizontal {
                arrow(ui, &forward_resp, false);
            }
            if forward_resp.rich_tip(forward_tip).clicked() {
                offset = next_offset(&items, content, room, offset);
            }
        }
        // The mouse wheel over the bar slides it.
        let over = ui.rect_contains_pointer(full.intersect(panel)) || bar_resp.hovered();
        if over {
            let wheel = ui.input(|i| i.smooth_scroll_delta);
            let delta = if horizontal {
                if wheel.x != 0.0 {
                    wheel.x
                } else {
                    wheel.y
                }
            } else {
                wheel.y
            };
            if delta != 0.0 {
                offset -= delta;
            }
        }
        // The finger sliding the bar: the bar follows it.
        if let Some(pos) = slid {
            offset = pos;
        }
        // An item asked to be seen: slid just enough to show it whole.
        if let Some(r) = keep {
            let (start, end) = if horizontal {
                (r.min.x - origin.x, r.max.x - origin.x)
            } else {
                (r.min.y - origin.y, r.max.y - origin.y)
            };
            if start < offset {
                offset = start;
            } else if end > offset + view(room) {
                offset = end - view(room);
            }
        }
        offset = offset.clamp(0.0, max_offset(content, room));
        if offset != st.offset {
            ui.ctx().request_repaint();
        }
    } else {
        offset = 0.0;
    }
    ui.data_mut(|d| {
        d.insert_temp(
            id,
            State {
                content,
                offset,
                items,
                grab: st.grab,
            },
        )
    });
    content
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<(f32, f32)> {
        (0..10)
            .map(|i| (i as f32 * 40.0, i as f32 * 40.0 + 40.0))
            .collect()
    }

    #[test]
    fn a_bar_that_fits_does_not_slide() {
        assert_eq!(max_offset(300.0, 300.0), 0.0);
        assert_eq!(max_offset(300.0, 400.0), 0.0);
    }

    #[test]
    fn forward_brings_the_first_hidden_item_to_the_start() {
        // 400 long in 200 of room: 164 between the buttons, items 40
        // apart. In view at first: 0..164, the item at 160 is cut, so
        // it comes first next.
        let it = items();
        assert_eq!(next_offset(&it, 400.0, 200.0, 0.0), 160.0);
        // Never past the end.
        assert_eq!(max_offset(400.0, 200.0), 400.0 - 164.0);
        assert_eq!(next_offset(&it, 400.0, 200.0, 160.0), 236.0);
    }

    #[test]
    fn back_returns_between_items_and_to_the_start() {
        let it = items();
        assert_eq!(previous_offset(&it, 200.0, 236.0), 80.0);
        assert_eq!(previous_offset(&it, 200.0, 80.0), 0.0);
    }

    #[test]
    fn areas_holding_items_are_not_cut_points() {
        let mut it = items();
        it.push((0.0, 400.0));
        it.push((120.0, 240.0));
        assert_eq!(next_offset(&it, 400.0, 200.0, 0.0), 160.0);
    }

    /// A bar of twelve 40 px buttons 300 wide, slid by `slide` with the
    /// » button clicks, then drawn once more: whether a « and a » are
    /// drawn, and the fades over the cut ends (their meshes' extents
    /// across the bar).
    fn arrows_and_fades(slides: usize) -> (bool, bool, Vec<(f32, f32)>) {
        let ctx = crate::theme::ui_context();
        let mut clock = 0.0;
        let mut run = |events: Vec<egui::Event>| {
            clock += 0.02;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 40.0))),
                events,
                time: Some(clock),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                row(ui, "arrows_test", |ui| {
                    for i in 0..12 {
                        ui.add_sized([40.0, 40.0], egui::Button::new(format!("{i}")));
                    }
                });
            });
            out.textures_delta.clear();
            out
        };
        for _ in 0..3 {
            run(Vec::new());
        }
        let forward = Pos2::new(300.0 - BUTTON / 2.0, 20.0);
        for _ in 0..slides {
            let button = |pressed| egui::Event::PointerButton {
                pos: forward,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            run(vec![egui::Event::PointerMoved(forward), button(true)]);
            run(vec![button(false)]);
            run(vec![egui::Event::PointerGone]);
        }
        let out = run(Vec::new());
        let mut back = false;
        let mut fwd = false;
        let mut fades = Vec::new();
        for c in &out.shapes {
            match &c.shape {
                egui::Shape::Text(t) => {
                    back |= t.galley.text() == "\u{ab}";
                    fwd |= t.galley.text() == "\u{bb}";
                }
                egui::Shape::Mesh(m) => {
                    let xs = m.vertices.iter().map(|v| v.pos.x);
                    let lo = xs.clone().fold(f32::MAX, f32::min);
                    let hi = xs.fold(f32::MIN, f32::max);
                    fades.push((lo, hi));
                }
                _ => {}
            }
        }
        (back, fwd, fades)
    }

    /// The « shows only once the bar has slid from its start and the »
    /// only while there is more of it to the right (#3588); the items cut
    /// at an end with more beyond it fade out there (#3591).
    #[test]
    fn arrows_and_fades_show_only_toward_more_of_the_bar() {
        let (back, forward, fades) = arrows_and_fades(0);
        assert!(!back, "no « at the start");
        assert!(forward, "a » with more to the right");
        assert!(
            fades
                .iter()
                .any(|&(lo, hi)| hi > 300.0 - BUTTON - 1.0 && lo > 200.0),
            "the right end fades: {fades:?}"
        );
        assert!(
            !fades.iter().any(|&(lo, _)| lo < BUTTON + 1.0),
            "the start does not: {fades:?}"
        );
        // Slid to the end: a «, no », the left end fades.
        let (back, forward, fades) = arrows_and_fades(6);
        assert!(back, "a « once slid");
        assert!(!forward, "no » at the end");
        assert!(
            fades
                .iter()
                .any(|&(lo, hi)| lo < BUTTON + 1.0 && hi < 100.0),
            "the left end fades: {fades:?}"
        );
        assert!(
            !fades.iter().any(|&(_, hi)| hi > 300.0 - BUTTON - 1.0),
            "the end does not: {fades:?}"
        );
    }

    /// Wheel turns over a button of a long bar slide the bar: the button
    /// under the pointer does not take them (#3589).
    #[test]
    fn the_wheel_over_an_item_slides_the_bar() {
        let bar = Bar::new(Vec2::new(300.0, 40.0), true);
        for _ in 0..3 {
            bar.frame(Vec::new());
        }
        let before = bar.rect(3);
        bar.frame(vec![egui::Event::PointerMoved(before.center())]);
        bar.frame(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -60.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        }]);
        for _ in 0..30 {
            bar.frame(Vec::new());
        }
        assert!(bar.rect(3).min.x < before.min.x - 10.0, "{:?}", bar.rect(3));
    }

    /// A bar of twelve 40 px buttons drawn headlessly in a window of
    /// `size`: where each button was drawn in the last frame and how
    /// many times each was clicked, with frames fed `events`.
    struct Bar {
        ctx: egui::Context,
        size: Vec2,
        horizontal: bool,
        rects: std::cell::RefCell<Vec<Rect>>,
        clicks: std::cell::RefCell<Vec<usize>>,
        clock: std::cell::Cell<f64>,
    }

    impl Bar {
        fn new(size: Vec2, horizontal: bool) -> Self {
            Self {
                ctx: crate::theme::ui_context(),
                size,
                horizontal,
                rects: std::cell::RefCell::new(vec![Rect::NOTHING; 12]),
                clicks: std::cell::RefCell::new(vec![0; 12]),
                clock: std::cell::Cell::new(0.0),
            }
        }

        fn frame(&self, events: Vec<egui::Event>) {
            self.clock.set(self.clock.get() + 0.02);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                events,
                time: Some(self.clock.get()),
                ..Default::default()
            };
            let mut out = self.ctx.run_ui(input, |ui| {
                let add = |ui: &mut Ui| {
                    for i in 0..12 {
                        let resp = ui.add_sized([40.0, 40.0], egui::Button::new(format!("{i}")));
                        self.rects.borrow_mut()[i] = resp.rect;
                        if resp.clicked() {
                            self.clicks.borrow_mut()[i] += 1;
                        }
                    }
                };
                if self.horizontal {
                    row(ui, "finger_test", add);
                } else {
                    column(ui, "finger_test", add);
                }
            });
            out.textures_delta.clear();
        }

        fn rect(&self, i: usize) -> Rect {
            self.rects.borrow()[i]
        }

        fn clicks(&self, i: usize) -> usize {
            self.clicks.borrow()[i]
        }

        /// A finger (or, without `finger`, the mouse) pressed at `from`,
        /// moved to `to` in `steps` and lifted there.
        fn drag(&self, from: Pos2, to: Pos2, steps: usize, finger: bool) {
            let touch = |phase, pos| egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(2),
                phase,
                pos,
                force: None,
            };
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let mut events = vec![egui::Event::PointerMoved(from), button(from, true)];
            if finger {
                events.insert(0, touch(egui::TouchPhase::Start, from));
            }
            self.frame(events);
            for k in 1..=steps {
                let p = from + (to - from) * (k as f32 / steps as f32);
                let mut events = vec![egui::Event::PointerMoved(p)];
                if finger {
                    events.insert(0, touch(egui::TouchPhase::Move, p));
                }
                self.frame(events);
            }
            let mut events = vec![button(to, false), egui::Event::PointerGone];
            if finger {
                events.insert(0, touch(egui::TouchPhase::End, to));
            }
            self.frame(events);
            self.frame(Vec::new());
            self.frame(Vec::new());
        }
    }

    /// A finger dragged along a bar longer than its room slides the bar
    /// with the finger: once it has moved past a tap's distance (6 px),
    /// the items follow its movement from there, and they stay where it
    /// left them (#1997).
    #[test]
    fn a_finger_dragged_along_a_long_bar_slides_it_with_the_finger() {
        let bar = Bar::new(Vec2::new(300.0, 40.0), true);
        for _ in 0..3 {
            bar.frame(Vec::new());
        }
        let before = bar.rect(2);
        assert!(before.right() < 300.0, "{before:?} shows whole");
        let from = before.center();
        // 80 px to the left in steps of 10: the first step decides the
        // slide, the 70 after it move the bar.
        bar.drag(from, from - Vec2::new(80.0, 0.0), 8, true);
        let after = bar.rect(2);
        assert!(
            (after.min.x - (before.min.x - 70.0)).abs() < 0.5,
            "{after:?} from {before:?}"
        );
        assert_eq!(after.min.y, before.min.y);
        // Back the other way, past the start: the bar stops at its start.
        let from = bar.rect(3).center();
        bar.drag(from, from + Vec2::new(200.0, 0.0), 10, true);
        assert!((bar.rect(2).min.x - before.min.x).abs() < 0.5);
    }

    /// The toolbox on a side: a finger dragged up its column slides it
    /// up (#1997).
    #[test]
    fn a_finger_dragged_along_a_column_slides_it() {
        let bar = Bar::new(Vec2::new(40.0, 300.0), false);
        for _ in 0..3 {
            bar.frame(Vec::new());
        }
        let before = bar.rect(2);
        let from = before.center();
        bar.drag(from, from - Vec2::new(0.0, 80.0), 8, true);
        let after = bar.rect(2);
        assert!(
            (after.min.y - (before.min.y - 70.0)).abs() < 0.5,
            "{after:?} from {before:?}"
        );
        assert_eq!(after.min.x, before.min.x);
    }

    /// A tap on a button of a long bar is a click, as before; a finger
    /// that slid the bar clicks nothing when it lifts, not the button it
    /// came down on and not the one under it then (#1997).
    #[test]
    fn a_tap_clicks_a_button_and_a_slide_clicks_nothing() {
        let bar = Bar::new(Vec2::new(300.0, 40.0), true);
        for _ in 0..3 {
            bar.frame(Vec::new());
        }
        let at = bar.rect(2).center();
        // A tap: down and up at one spot (a finger wobbles a pixel).
        bar.drag(at, at + Vec2::new(1.0, 1.0), 1, true);
        assert_eq!(bar.clicks(2), 1, "a tap clicks");
        let before = bar.rect(2);
        bar.drag(at, at - Vec2::new(80.0, 0.0), 8, true);
        assert!((bar.rect(2).min.x - (before.min.x - 70.0)).abs() < 0.5);
        let clicks: Vec<usize> = (0..12).map(|i| bar.clicks(i)).collect();
        assert_eq!(
            clicks.iter().sum::<usize>(),
            1,
            "the slide clicked nothing: {clicks:?}"
        );
    }

    /// A finger that moves across a bar (off it) is no slide: the bar
    /// stays, the items keep that drag (#1997).
    #[test]
    fn a_finger_moving_across_the_bar_does_not_slide_it() {
        let bar = Bar::new(Vec2::new(300.0, 40.0), true);
        for _ in 0..3 {
            bar.frame(Vec::new());
        }
        let before = bar.rect(2);
        let from = before.center();
        bar.drag(from, from + Vec2::new(10.0, 60.0), 6, true);
        assert_eq!(bar.rect(2), before);
    }

    /// The mouse dragged along a bar slides nothing, before the screen
    /// was ever touched and after (#1997).
    #[test]
    fn a_mouse_drag_does_not_slide_the_bar() {
        let bar = Bar::new(Vec2::new(300.0, 40.0), true);
        for _ in 0..3 {
            bar.frame(Vec::new());
        }
        let before = bar.rect(2);
        let from = before.center();
        bar.drag(from, from - Vec2::new(80.0, 0.0), 8, false);
        assert_eq!(bar.rect(2), before, "no touch screen yet");
        assert_eq!(bar.clicks(2), 0);
        // The screen touched once: the mouse still slides nothing.
        let at = bar.rect(3).center();
        bar.drag(at, at, 1, true);
        assert_eq!(bar.clicks(3), 1);
        bar.drag(from, from - Vec2::new(80.0, 0.0), 8, false);
        assert_eq!(bar.rect(2), before, "a touch screen, the mouse");
    }
}
