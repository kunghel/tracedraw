//! The update progress: what Help > Updates and the update card show
//! once Install (or Update now) is chosen. A dialog of a fixed size with
//! the TraceDraw icon, "Updating TraceDraw" and the two versions, then
//! the step the install is at: Downloading with a progress bar, the
//! percentage and a line with the megabytes, the speed and the time
//! left; Checking the download and Installing with a moving bar; the
//! finished state with a check mark, Restart Now and Later; a failure in
//! plain words with Try Again and Download from the website. Cancel stops
//! the download and removes what it wrote. The window is redrawn only
//! while something moves. See docs/behavior/updates.md.

use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::updates::{InstallState, Progress, Trouble};
use egui::{vec2, Color32, Context, Rect, RichText, Sense, Stroke, Ui, Vec2};

/// The dialog's size, title bar and frame included. Fixed, so nothing
/// jumps while the numbers change.
pub const SIZE: Vec2 = vec2(500.0, 280.0);
/// The height of the step, bar and detail lines.
const STATUS_H: f32 = 92.0;
/// Padding of the dialog's body.
const PAD: f32 = 20.0;
/// The height of the buttons.
const BUTTON_H: f32 = 30.0;
/// The green of the finished check mark.
const DONE_GREEN: Color32 = Color32::from_rgb(0x10, 0x8A, 0x3E);
/// The red of a failure.
const FAIL_RED: Color32 = Color32::from_rgb(0xC4, 0x2B, 0x1C);
/// How often the window is redrawn during a download: the numbers move
/// a few times a second (the download also asks for a frame when it has
/// news).
const DOWNLOAD_FRAME: std::time::Duration = std::time::Duration::from_millis(200);
/// How often the moving bar of Checking and Installing is redrawn.
const BUSY_FRAME: std::time::Duration = std::time::Duration::from_millis(40);

/// Where an install stands, as the dialog and the card show it.
#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Download(Progress),
    Checking,
    Installing,
    /// Done: `version` runs after Restart. `setup`: the system's installer
    /// still has to run (the Windows setup, the `.deb`), at Restart.
    Ready {
        version: String,
        setup: bool,
    },
    Failed(String),
}

/// The phase of `install` (with the download's `progress`); `None` when
/// nothing was started.
pub fn phase(install: &InstallState, progress: Option<Progress>) -> Option<Phase> {
    Some(match install {
        InstallState::Idle => return None,
        InstallState::Downloading { done, total } => Phase::Download(
            progress
                .filter(|p| p.done == *done && p.total == *total)
                .unwrap_or(Progress {
                    done: *done,
                    total: *total,
                    speed: None,
                    left: None,
                }),
        ),
        InstallState::Checking => Phase::Checking,
        InstallState::Installing => Phase::Installing,
        InstallState::Installed(v) => Phase::Ready {
            version: v.clone(),
            setup: false,
        },
        InstallState::Ready(v) => Phase::Ready {
            version: v.clone(),
            setup: true,
        },
        InstallState::Failed(e) => Phase::Failed(e.clone()),
    })
}

/// What the user chose in the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Stop the download.
    Cancel,
    /// Exit and start the new version.
    Restart,
    /// Keep working; the new version starts next time.
    Later,
    /// Start the install again after a failure.
    TryAgain,
    /// Open the release's download in the browser.
    Website,
    /// The close button or Escape: the dialog goes, a download goes on.
    Close,
}

/// Megabytes with one decimal.
pub fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_048_576.0)
}

/// A download speed: megabytes a second from about one, else kilobytes.
pub fn speed_text(bytes_per_second: f64) -> String {
    let mb = bytes_per_second / 1_048_576.0;
    if mb >= 0.95 {
        trf("updates.speed_mb", &[("n", &format!("{mb:.1}"))])
    } else {
        let kb = (bytes_per_second / 1024.0).round().max(0.0) as u64;
        trf("updates.speed_kb", &[("n", &kb.to_string())])
    }
}

/// The time left, rounded so that it does not flicker: whole seconds
/// under ten, steps of five seconds under a minute and a half, minutes
/// above.
pub fn left_text(seconds: f64) -> String {
    let s = seconds.max(0.0).ceil() as u64;
    if s < 10 {
        trf("updates.left_seconds", &[("n", &s.max(1).to_string())])
    } else if s < 90 {
        let five = s.div_ceil(5) * 5;
        trf("updates.left_seconds", &[("n", &five.to_string())])
    } else {
        let minutes = ((seconds / 60.0).round() as u64).max(2);
        trf("updates.left_minutes", &[("n", &minutes.to_string())])
    }
}

/// The detail line of a download: "12.3 MB of 45.0 MB, 3.2 MB/s, about
/// 10 s left", the speed once it is measured, the time left once it
/// means something.
pub fn detail(p: &Progress) -> String {
    let size = trf(
        "updates.size_progress",
        &[("done", &megabytes(p.done)), ("total", &megabytes(p.total))],
    );
    match (p.speed, p.left) {
        (Some(speed), Some(left)) => trf(
            "updates.detail_left",
            &[
                ("size", &size),
                ("speed", &speed_text(speed)),
                ("left", &left_text(left)),
            ],
        ),
        (Some(speed), None) => trf(
            "updates.detail_speed",
            &[("size", &size), ("speed", &speed_text(speed))],
        ),
        _ => size,
    }
}

/// An install error in plain words.
pub fn plain_error(error: &str) -> String {
    tr(match crate::updates::trouble(error) {
        Trouble::Network => "updates.error_network",
        Trouble::Damaged => "updates.error_damaged",
        Trouble::Permission => "updates.error_permission",
        Trouble::Space => "updates.error_space",
        Trouble::Other => "updates.error_other",
    })
}

/// Ask for the next frame only as soon as something on screen moves.
fn schedule_repaint(ctx: &Context, phase: &Phase) {
    match phase {
        Phase::Download(_) => ctx.request_repaint_after(DOWNLOAD_FRAME),
        Phase::Checking | Phase::Installing => ctx.request_repaint_after(BUSY_FRAME),
        Phase::Ready { .. } | Phase::Failed(_) => {}
    }
}

/// A bar for work of unknown length: a block sliding along the groove.
fn busy_bar(ui: &mut Ui, width: f32, height: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let r = height / 2.0;
    let p = ui.painter();
    p.rect_filled(rect, r, ui.visuals().extreme_bg_color);
    p.rect_stroke(
        rect,
        r,
        Stroke::new(1.0, Tokens::control_border()),
        egui::StrokeKind::Inside,
    );
    let t = ui.input(|i| i.time) as f32;
    let block = rect.width() * 0.3;
    // Once along and past the end every 1.4 s.
    let x = rect.left() - block + (t / 1.4).fract() * (rect.width() + block);
    let moving = Rect::from_min_max(
        egui::pos2(x.max(rect.left()), rect.top()),
        egui::pos2((x + block).min(rect.right()), rect.bottom()),
    );
    if moving.width() > 0.5 {
        p.rect_filled(moving, r, Tokens::ACCENT);
    }
    resp
}

/// A disc of `color` with a white check mark (`ok`) or exclamation mark.
fn badge(ui: &mut Ui, ok: bool, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    p.circle_filled(c, 11.0, color);
    let s = Stroke::new(2.2, Color32::WHITE);
    if ok {
        p.line_segment([c + vec2(-5.0, 0.5), c + vec2(-1.5, 4.0)], s);
        p.line_segment([c + vec2(-1.5, 4.0), c + vec2(5.0, -3.5)], s);
    } else {
        p.line_segment([c + vec2(0.0, -6.0), c + vec2(0.0, 2.0)], s);
        p.circle_filled(c + vec2(0.0, 5.5), 1.4, Color32::WHITE);
    }
}

/// The step, the bar and the detail of `phase`, in a box of a fixed
/// height across the width of `ui`.
pub fn status(ui: &mut Ui, phase: &Phase) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, STATUS_H), Sense::hover());
    let mut ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    ui.set_clip_rect(rect.intersect(ui.clip_rect()));
    ui.spacing_mut().item_spacing.y = 8.0;
    let step = |ui: &mut Ui, text: String, right: Option<String>| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(text)
                    .size(14.0)
                    .strong()
                    .color(Tokens::text()),
            );
            if let Some(right) = right {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(right)
                            .size(14.0)
                            .strong()
                            .color(Tokens::text()),
                    );
                });
            }
        });
    };
    let dim = |ui: &mut Ui, text: String| {
        ui.add(egui::Label::new(RichText::new(text).size(12.5).color(Tokens::text_dim())).wrap());
    };
    match phase {
        Phase::Download(p) => {
            let pct = crate::updates::percent(p.done, p.total);
            step(
                &mut ui,
                tr("updates.step_download"),
                Some(format!("{pct} %")),
            );
            ui.add(
                egui::ProgressBar::new(crate::updates::fraction(p.done, p.total))
                    .desired_width(width)
                    .desired_height(10.0)
                    .corner_radius(5.0)
                    .fill(Tokens::ACCENT),
            );
            dim(&mut ui, detail(p));
        }
        Phase::Checking | Phase::Installing => {
            let (title, text) = if *phase == Phase::Checking {
                ("updates.step_check", "updates.check_detail")
            } else {
                ("updates.step_install", "updates.install_detail")
            };
            step(&mut ui, tr(title), None);
            busy_bar(&mut ui, width, 10.0);
            dim(&mut ui, tr(text));
        }
        Phase::Ready { version, setup } => {
            ui.horizontal(|ui| {
                badge(ui, true, DONE_GREEN);
                ui.add_space(4.0);
                ui.label(
                    RichText::new(trf("updates.ready_title", &[("new", version)]))
                        .size(15.0)
                        .strong()
                        .color(Tokens::text()),
                );
            });
            dim(
                &mut ui,
                tr(if *setup {
                    "updates.ready_setup"
                } else {
                    "updates.ready_restart"
                }),
            );
        }
        Phase::Failed(e) => {
            ui.horizontal(|ui| {
                badge(ui, false, FAIL_RED);
                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr("updates.failed_title"))
                        .size(15.0)
                        .strong()
                        .color(Tokens::text()),
                );
            });
            ui.add(
                egui::Label::new(
                    RichText::new(plain_error(e))
                        .size(13.0)
                        .color(Tokens::text()),
                )
                .wrap(),
            );
            let details = trf("updates.error_details", &[("error", e)]);
            ui.add(
                egui::Label::new(RichText::new(&details).size(11.5).color(Tokens::text_dim()))
                    .truncate(),
            )
            .on_hover_text(details);
        }
    }
}

/// A push button `BUTTON_H` tall and at least 100 wide; the primary one
/// filled with the accent colour.
pub fn push_button(ui: &mut Ui, text: &str, primary: bool, enabled: bool) -> bool {
    let label = if primary {
        RichText::new(text).color(Color32::WHITE).strong()
    } else {
        RichText::new(text)
    };
    let mut button = egui::Button::new(label).min_size(vec2(100.0, BUTTON_H));
    if primary {
        button = button.fill(Tokens::ACCENT);
    }
    let resp = ui.add_enabled(enabled, button);
    crate::ui::tips::named(resp, text).clicked()
}

/// The buttons of `phase`, laid out right to left in `ui`.
fn buttons(ui: &mut Ui, phase: &Phase) -> Option<Action> {
    let mut action = None;
    ui.spacing_mut().item_spacing.x = 8.0;
    match phase {
        Phase::Download(_) | Phase::Checking | Phase::Installing => {
            // Checked and installing files are not stopped half way: the
            // button stays, greyed, so that nothing moves.
            let can = matches!(phase, Phase::Download(_));
            if push_button(ui, &tr("dialog.cancel"), false, can) {
                action = Some(Action::Cancel);
            }
        }
        Phase::Ready { .. } => {
            if push_button(ui, &tr("updates.later"), false, true) {
                action = Some(Action::Later);
            }
            if push_button(ui, &tr("updates.restart"), true, true) {
                action = Some(Action::Restart);
            }
        }
        Phase::Failed(_) => {
            if push_button(ui, &tr("updates.website"), false, true) {
                action = Some(Action::Website);
            }
            if push_button(ui, &tr("updates.try_again"), true, true) {
                action = Some(Action::TryAgain);
            }
        }
    }
    action
}

/// The icon, "Updating TraceDraw" and "Version 1.0 to 1.1".
fn header(ui: &mut Ui, version: &str, new: &str) {
    let icon = crate::ui::brand::icon(ui.ctx());
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::hover());
        crate::ui::brand::paint_icon(ui.painter(), icon.as_ref(), r);
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add_space(3.0);
            ui.label(
                RichText::new(tr("updates.progress_title"))
                    .size(19.0)
                    .strong()
                    .color(Tokens::text()),
            );
            ui.label(
                RichText::new(trf(
                    "updates.progress_versions",
                    &[("version", version), ("new", new)],
                ))
                .size(13.0)
                .color(Tokens::text_dim()),
            );
        });
    });
}

/// The update progress dialog for the update from `version` to `new` at
/// `phase`. Returns what was chosen this frame. Enter picks Restart Now
/// or Try Again; Escape and the close button answer [`Action::Close`].
pub fn dialog(ctx: &Context, version: &str, new: &str, phase: &Phase) -> Option<Action> {
    let mut action = None;
    let closed = crate::ui::chrome::dialog(ctx, "update_progress", "TraceDraw", SIZE, |ui| {
        let body = ui.max_rect();
        let edge = crate::ui::chrome::EDGE;
        let inner = Rect::from_min_max(
            body.min + vec2(edge + PAD, PAD - 2.0),
            body.max - vec2(edge + PAD, edge + PAD - 4.0),
        );
        let mut content = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        header(&mut content, version, new);
        content.add_space(18.0);
        status(&mut content, phase);
        let row = Rect::from_min_max(
            egui::pos2(inner.left(), inner.bottom() - BUTTON_H),
            inner.max,
        );
        let mut row_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        action = buttons(&mut row_ui, phase);
    });
    schedule_repaint(ctx, phase);
    if action.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        action = match phase {
            Phase::Ready { .. } => Some(Action::Restart),
            Phase::Failed(_) => Some(Action::TryAgain),
            _ => None,
        };
    }
    if closed && action.is_none() {
        action = Some(Action::Close);
    }
    action
}

/// The progress in the update card: [`status`], and the frames the
/// phase needs.
pub fn card_status(ui: &mut Ui, phase: &Phase) {
    status(ui, phase);
    schedule_repaint(ui.ctx(), phase);
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1 << 20;

    #[test]
    fn the_detail_line_grows_as_the_numbers_become_meaningful() {
        let mut p = Progress {
            done: 12 * MB + MB / 3,
            total: 45 * MB,
            speed: None,
            left: None,
        };
        assert_eq!(detail(&p), "12.3 MB of 45.0 MB");
        p.speed = Some(3.2 * MB as f64);
        assert_eq!(detail(&p), "12.3 MB of 45.0 MB, 3.2 MB/s");
        p.left = Some(9.2);
        assert_eq!(detail(&p), "12.3 MB of 45.0 MB, 3.2 MB/s, about 10 s left");
        assert_eq!(speed_text(300.0 * 1024.0), "300 KB/s");
        assert_eq!(left_text(0.2), "about 1 s left");
        assert_eq!(left_text(9.0), "about 9 s left");
        assert_eq!(left_text(41.0), "about 45 s left");
        assert_eq!(left_text(89.0), "about 90 s left");
        assert_eq!(left_text(95.0), "about 2 min left");
        assert_eq!(left_text(600.0), "about 10 min left");
    }

    #[test]
    fn every_install_state_has_its_phase() {
        assert_eq!(phase(&InstallState::Idle, None), None);
        let p = Progress {
            done: 1,
            total: 2,
            speed: Some(5.0),
            left: None,
        };
        assert_eq!(
            phase(&InstallState::Downloading { done: 1, total: 2 }, Some(p)),
            Some(Phase::Download(p))
        );
        // A progress of another moment is not mixed in.
        assert_eq!(
            phase(&InstallState::Downloading { done: 2, total: 2 }, Some(p)),
            Some(Phase::Download(Progress {
                done: 2,
                total: 2,
                speed: None,
                left: None
            }))
        );
        assert_eq!(phase(&InstallState::Checking, None), Some(Phase::Checking));
        assert_eq!(
            phase(&InstallState::Installing, None),
            Some(Phase::Installing)
        );
        assert_eq!(
            phase(&InstallState::Installed("2.0".into()), None),
            Some(Phase::Ready {
                version: "2.0".into(),
                setup: false
            })
        );
        assert_eq!(
            phase(&InstallState::Ready("2.0".into()), None),
            Some(Phase::Ready {
                version: "2.0".into(),
                setup: true
            })
        );
        assert_eq!(
            phase(&InstallState::Failed("x".into()), None),
            Some(Phase::Failed("x".into()))
        );
        assert!(plain_error("the download answered 503").contains("internet"));
    }
}
