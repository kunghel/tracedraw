//! The commands of the image editing mode (Bitmaps > Edit Inside): each
//! with its name, shortcut and whether it can run now, run the same way
//! from its menu, the standard bar and the keys. See
//! docs/behavior/edit-inside-picture.md.

use crate::app::App;
use crate::pixel_edit::{self, Tones};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelCmd {
    Finish,
    Cancel,
    Undo,
    Redo,
    CopyToLayer,
    CutToLayer,
    PasteAsLayer,
    Delete,
    Fill,
    ContentFill,
    SelectAll,
    SelectNone,
    InvertSelection,
    Feather,
    Grow,
    Border,
    Smooth,
    Shadows,
    Midtones,
    Highlights,
    QuickMask,
    SaveSelection,
    Crop,
    FlipH,
    FlipV,
    RedEye,
    LayerNew,
    LayerDuplicate,
    LayerDelete,
    LayerUp,
    LayerDown,
    MergeDown,
    Flatten,
    ZoomIn,
    ZoomOut,
    ZoomFit,
    Zoom100,
    TurnRight,
    TurnLeft,
    Turn180,
    AutoCrop,
    Grayscale,
    BlackWhite,
    CanvasSize,
    SmartResize,
    Histogram,
    Hints,
}

impl PixelCmd {
    /// Every command (the image bar's customize menu lists them).
    pub const ALL: [PixelCmd; 47] = [
        PixelCmd::Finish,
        PixelCmd::Cancel,
        PixelCmd::Undo,
        PixelCmd::Redo,
        PixelCmd::CopyToLayer,
        PixelCmd::CutToLayer,
        PixelCmd::PasteAsLayer,
        PixelCmd::Delete,
        PixelCmd::Fill,
        PixelCmd::ContentFill,
        PixelCmd::SelectAll,
        PixelCmd::SelectNone,
        PixelCmd::InvertSelection,
        PixelCmd::Feather,
        PixelCmd::Grow,
        PixelCmd::Border,
        PixelCmd::Smooth,
        PixelCmd::Shadows,
        PixelCmd::Midtones,
        PixelCmd::Highlights,
        PixelCmd::QuickMask,
        PixelCmd::SaveSelection,
        PixelCmd::Crop,
        PixelCmd::FlipH,
        PixelCmd::FlipV,
        PixelCmd::RedEye,
        PixelCmd::LayerNew,
        PixelCmd::LayerDuplicate,
        PixelCmd::LayerDelete,
        PixelCmd::LayerUp,
        PixelCmd::LayerDown,
        PixelCmd::MergeDown,
        PixelCmd::Flatten,
        PixelCmd::ZoomIn,
        PixelCmd::ZoomOut,
        PixelCmd::ZoomFit,
        PixelCmd::Zoom100,
        PixelCmd::TurnRight,
        PixelCmd::TurnLeft,
        PixelCmd::Turn180,
        PixelCmd::AutoCrop,
        PixelCmd::Grayscale,
        PixelCmd::BlackWhite,
        PixelCmd::CanvasSize,
        PixelCmd::SmartResize,
        PixelCmd::Histogram,
        PixelCmd::Hints,
    ];

    /// Its button's id on the image bar ("pixel:" and its name).
    pub fn id(self) -> String {
        format!("pixel:{self:?}")
    }

    /// The command of a button id.
    pub fn from_id(id: &str) -> Option<PixelCmd> {
        let name = id.strip_prefix("pixel:")?;
        PixelCmd::ALL.into_iter().find(|c| format!("{c:?}") == name)
    }

    /// The picture of its button, when it has one.
    pub fn action(self) -> Option<crate::ui::icons::Action> {
        use crate::ui::icons::Action as A;
        Some(match self {
            PixelCmd::Undo => A::Undo,
            PixelCmd::Redo => A::Redo,
            PixelCmd::CopyToLayer => A::Copy,
            PixelCmd::CutToLayer => A::Cut,
            PixelCmd::PasteAsLayer => A::Paste,
            PixelCmd::Delete => A::Delete,
            _ => return None,
        })
    }

    /// The i18n key of its name.
    pub fn key(self) -> &'static str {
        match self {
            PixelCmd::Finish => "pixel.finish",
            PixelCmd::Cancel => "pixel.cancel",
            PixelCmd::Undo => "pixel.undo",
            PixelCmd::Redo => "pixel.redo",
            PixelCmd::CopyToLayer => "pixel.layer.copy_to",
            PixelCmd::CutToLayer => "pixel.layer.cut_to",
            PixelCmd::PasteAsLayer => "pixel.layer.paste",
            PixelCmd::Delete => "pixel.sel.delete",
            PixelCmd::Fill => "pixel.sel.fill",
            PixelCmd::ContentFill => "pixel.content_fill",
            PixelCmd::SelectAll => "pixel.sel.all",
            PixelCmd::SelectNone => "pixel.sel.none",
            PixelCmd::InvertSelection => "pixel.sel.invert",
            PixelCmd::Feather => "pixel.sel.feather",
            PixelCmd::Grow => "pixel.sel.grow",
            PixelCmd::Border => "pixel.sel.border",
            PixelCmd::Smooth => "pixel.sel.smooth",
            PixelCmd::Shadows => "pixel.sel.shadows",
            PixelCmd::Midtones => "pixel.sel.midtones",
            PixelCmd::Highlights => "pixel.sel.highlights",
            PixelCmd::QuickMask => "pixel.sel.quick_mask",
            PixelCmd::SaveSelection => "pixel.sel.save",
            PixelCmd::Crop => "pixel.sel.crop",
            PixelCmd::FlipH => "pixel.flip_h",
            PixelCmd::FlipV => "pixel.flip_v",
            PixelCmd::RedEye => "pixel.red_eye",
            PixelCmd::LayerNew => "pixel.layer.new",
            PixelCmd::LayerDuplicate => "pixel.layer.duplicate",
            PixelCmd::LayerDelete => "pixel.layer.delete",
            PixelCmd::LayerUp => "pixel.layer.up",
            PixelCmd::LayerDown => "pixel.layer.down",
            PixelCmd::MergeDown => "pixel.layer.merge_down",
            PixelCmd::Flatten => "pixel.layer.flatten",
            PixelCmd::ZoomIn => "menu.view.zoom_in",
            PixelCmd::ZoomOut => "menu.view.zoom_out",
            PixelCmd::ZoomFit => "pixel.zoom_fit",
            PixelCmd::Zoom100 => "pixel.zoom_100",
            PixelCmd::TurnRight => "pixel.turn_right",
            PixelCmd::TurnLeft => "pixel.turn_left",
            PixelCmd::Turn180 => "pixel.turn_180",
            PixelCmd::AutoCrop => "pixel.auto_crop",
            PixelCmd::Grayscale => "pixel.grayscale",
            PixelCmd::BlackWhite => "pixel.black_white",
            PixelCmd::CanvasSize => "pixel.canvas_size",
            PixelCmd::SmartResize => "pixel.smart_resize_cmd",
            PixelCmd::Histogram => "pixel.section.histogram",
            PixelCmd::Hints => "pixel.section.hints",
        }
    }

    /// The picture of its menu row (see `pixel_menu_icons.rs`), when it
    /// has one.
    pub fn picture(self) -> Option<&'static str> {
        Some(match self {
            PixelCmd::TurnRight => "turn_right",
            PixelCmd::TurnLeft => "turn_left",
            PixelCmd::Turn180 => "turn_180",
            PixelCmd::AutoCrop => "auto_crop",
            PixelCmd::Grayscale => "grayscale",
            PixelCmd::BlackWhite => "black_white",
            PixelCmd::CanvasSize => "canvas_size",
            PixelCmd::SmartResize => "smart_resize",
            PixelCmd::Histogram => "histogram",
            PixelCmd::FlipH => "flip_h",
            PixelCmd::FlipV => "flip_v",
            _ => return None,
        })
    }

    /// Its keys, as the menus show them.
    pub fn shortcut(self) -> &'static str {
        match self {
            PixelCmd::Finish => "Enter",
            PixelCmd::Undo => "Ctrl+Z",
            PixelCmd::Redo => "Ctrl+Y",
            PixelCmd::Delete => "Del",
            PixelCmd::SelectAll => "Ctrl+A",
            PixelCmd::SelectNone => "Ctrl+D",
            PixelCmd::InvertSelection => "Ctrl+Shift+I",
            PixelCmd::QuickMask => "Shift+Q",
            PixelCmd::ZoomIn => "Ctrl++",
            PixelCmd::ZoomOut => "Ctrl+-",
            PixelCmd::ZoomFit => "Ctrl+0",
            PixelCmd::Zoom100 => "Ctrl+1",
            PixelCmd::TurnRight => "Ctrl+R",
            PixelCmd::TurnLeft => "Ctrl+Shift+R",
            _ => "",
        }
    }

    /// Whether it can run now.
    pub fn enabled(self, app: &App) -> bool {
        let Some(pe) = app.pixel_edit.as_ref() else {
            return false;
        };
        let has = pe.pi.sel.is_some();
        let n = pe.pi.layers.len().max(1);
        let i = pe.pi.active;
        match self {
            PixelCmd::Undo => pe.history.can_undo(),
            PixelCmd::Redo => pe.history.can_redo(),
            PixelCmd::SelectNone
            | PixelCmd::Feather
            | PixelCmd::Grow
            | PixelCmd::Border
            | PixelCmd::Smooth
            | PixelCmd::SaveSelection
            | PixelCmd::Crop
            | PixelCmd::ContentFill => has,
            PixelCmd::LayerDelete => n > 1,
            PixelCmd::LayerUp => i + 1 < n,
            PixelCmd::LayerDown | PixelCmd::MergeDown => i > 0,
            PixelCmd::Flatten => pe.pi.is_layered(),
            _ => true,
        }
    }

    /// Whether it is on (the quick mask).
    pub fn checked(self, app: &App) -> Option<bool> {
        let pe = app.pixel_edit.as_ref()?;
        match self {
            PixelCmd::QuickMask => {
                Some(pe.target == crate::pixel_edit_app::MaskTarget::QuickMask)
            }
            _ => None,
        }
    }

    /// Run it, when it can run.
    pub fn run(self, app: &mut App) {
        if !self.enabled(app) {
            return;
        }
        match self {
            PixelCmd::Finish => {
                match app.pixel_edit.as_mut() {
                    Some(pe) if pe.warp.is_some() => pe.apply_warp(),
                    _ => app.finish_pixel_edit(),
                }
                return;
            }
            PixelCmd::Cancel => {
                app.cancel_pixel_edit();
                return;
            }
            PixelCmd::CanvasSize | PixelCmd::SmartResize => {
                app.open_docker(crate::app::DockerTab::PixelImage);
                return;
            }
            PixelCmd::Histogram => {
                app.open_docker(crate::app::DockerTab::PixelHistogram);
                return;
            }
            PixelCmd::Hints => {
                app.open_docker(crate::app::DockerTab::PixelHints);
                return;
            }
            _ => {}
        }
        let Some(pe) = app.pixel_edit.as_mut() else {
            return;
        };
        let (w, h) = pe.pi.img.dimensions();
        let whole = |pi: &pixel_edit::PixelImage| Some([0, 0, pi.img.width(), pi.img.height()]);
        let name = crate::ui::pixel_bar::new_layer_name(pe);
        let sel_by = |pe: &mut crate::pixel_edit_app::PixelEdit,
                      f: &dyn Fn(&image::GrayImage) -> image::GrayImage| {
            if let Some(m) = pe.pi.sel.as_ref().map(f) {
                pe.set_selection(Some(m));
            }
        };
        match self {
            PixelCmd::Finish | PixelCmd::Cancel => {}
            PixelCmd::Undo => pe.undo(),
            PixelCmd::Redo => pe.redo(),
            PixelCmd::CopyToLayer | PixelCmd::CutToLayer => {
                let cut = self == PixelCmd::CutToLayer;
                pe.apply_all(true, |pi| {
                    pi.selection_to_layer(name.clone(), cut)
                        .then(|| [0, 0, pi.img.width(), pi.img.height()])
                });
            }
            PixelCmd::PasteAsLayer => {
                if let Some(img) = crate::clipboard::read_system().image {
                    pe.apply_all(true, |pi| pixel_edit::paste_as_layer(pi, name.clone(), &img));
                }
            }
            PixelCmd::Delete => {
                pe.apply(false, pixel_edit::clear_selection);
            }
            PixelCmd::Fill => {
                let (c, o) = (pe.opts.color, pe.opts.opacity);
                pe.apply(false, |pi| pixel_edit::fill_selection(pi, c, o));
            }
            PixelCmd::ContentFill => {
                pe.apply(false, pixel_edit::content_fill);
            }
            PixelCmd::SelectAll => {
                pe.set_selection(Some(image::GrayImage::from_pixel(w, h, image::Luma([255]))));
            }
            PixelCmd::SelectNone => pe.set_selection(None),
            PixelCmd::InvertSelection => {
                let inv = pixel_edit::invert(pe.pi.sel.as_ref(), w, h);
                pe.set_selection(Some(inv));
            }
            PixelCmd::Feather => {
                let r = pe.feather_px;
                sel_by(pe, &|m| pixel_edit::feather(m, r));
            }
            PixelCmd::Grow => {
                let r = pe.grow_px;
                sel_by(pe, &|m| pixel_edit::grow(m, r));
            }
            PixelCmd::Border => {
                let r = pe.feather_px;
                sel_by(pe, &|m| pixel_edit::border(m, r));
            }
            PixelCmd::Smooth => {
                let r = pe.feather_px;
                sel_by(pe, &|m| pixel_edit::smooth_selection(m, r));
            }
            PixelCmd::Shadows | PixelCmd::Midtones | PixelCmd::Highlights => {
                let t = match self {
                    PixelCmd::Shadows => Tones::Shadows,
                    PixelCmd::Midtones => Tones::Midtones,
                    _ => Tones::Highlights,
                };
                let m = pixel_edit::tone_mask(&pe.pi.img, t);
                pe.set_selection(Some(m));
            }
            PixelCmd::QuickMask => {
                let on = pe.target != crate::pixel_edit_app::MaskTarget::QuickMask;
                pe.set_quick_mask(on);
            }
            PixelCmd::SaveSelection => {
                if let Some(m) = pe.pi.sel.clone() {
                    pe.saved_sels.push(m);
                }
            }
            PixelCmd::Crop => {
                pe.apply_all(true, pixel_edit::crop_to_selection);
            }
            PixelCmd::FlipH | PixelCmd::FlipV => {
                let across = self == PixelCmd::FlipH;
                pe.apply_all(true, |pi| {
                    pixel_edit::flip(pi, across);
                    whole(pi)
                });
            }
            PixelCmd::RedEye => {
                pe.apply(false, pixel_edit::red_eye_selection);
            }
            PixelCmd::LayerNew => {
                pe.apply_all(true, |pi| {
                    pi.add_layer(name.clone());
                    whole(pi)
                });
            }
            PixelCmd::LayerDuplicate => {
                pe.apply_all(true, |pi| {
                    pi.duplicate_layer(name.clone());
                    whole(pi)
                });
            }
            PixelCmd::LayerDelete => {
                pe.apply_all(true, |pi| pi.delete_layer().then(|| whole(pi)).flatten());
            }
            PixelCmd::LayerUp | PixelCmd::LayerDown => {
                let up = self == PixelCmd::LayerUp;
                pe.apply_all(true, |pi| pi.move_layer(up).then(|| whole(pi)).flatten());
            }
            PixelCmd::MergeDown => {
                pe.apply_all(true, |pi| pi.merge_down().then(|| whole(pi)).flatten());
            }
            PixelCmd::Flatten => {
                pe.apply_all(true, |pi| pi.flatten().then(|| whole(pi)).flatten());
            }
            PixelCmd::ZoomIn | PixelCmd::ZoomOut => {
                let area = pe.ws_area;
                if area.is_positive() {
                    let z = pe.ws.step(if self == PixelCmd::ZoomIn { 1 } else { -1 });
                    pe.ws.zoom_to(area, area.center(), z);
                }
            }
            PixelCmd::ZoomFit => {
                let area = pe.ws_area;
                pe.ws.fit(area, w, h, true);
            }
            PixelCmd::Zoom100 => {
                let area = pe.ws_area;
                if area.is_positive() {
                    pe.ws.zoom_to(area, area.center(), 1.0);
                }
            }
            PixelCmd::TurnRight | PixelCmd::TurnLeft | PixelCmd::Turn180 => {
                let q = match self {
                    PixelCmd::TurnRight => 1,
                    PixelCmd::Turn180 => 2,
                    _ => 3,
                };
                pe.apply_all(true, |pi| crate::pixel_ops::turn(pi, q));
                let area = pe.ws_area;
                let (w, h) = pe.pi.img.dimensions();
                pe.ws.fit(area, w, h, false);
            }
            PixelCmd::AutoCrop => {
                pe.apply_all(true, crate::pixel_ops::auto_crop);
            }
            PixelCmd::Grayscale => {
                pe.apply_all(true, crate::pixel_ops::grayscale);
            }
            PixelCmd::BlackWhite => {
                pe.apply_all(true, |pi| crate::pixel_ops::black_and_white(pi, 128));
            }
            PixelCmd::CanvasSize
            | PixelCmd::SmartResize
            | PixelCmd::Histogram
            | PixelCmd::Hints => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_image_command_has_a_name_in_english() {
        for c in PixelCmd::ALL {
            assert_ne!(crate::i18n::tr(c.key()), c.key(), "{c:?}");
        }
    }
}
