//! The Tools > Sizes and Areas window (#2098): the selected objects'
//! sizes counted together and their areas, or every page's size and the
//! pages' total area (`crate::sizes`), with Copy for a quote or an order
//! and Add Dimensions to Each Object to label the objects' sizes.

use crate::ui::btn_icons::IconUi;
use crate::app::App;
use crate::i18n::tr;
use crate::sizes;
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Context, RichText};

/// Which list the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizesTab {
    #[default]
    Objects,
    Pages,
}

pub fn dialog(app: &mut App, ctx: &Context, tab: &mut SizesTab, close: &mut bool) {
    crate::ui::dialogs::window(ctx, tr("sizes.title")).show(ctx, |ui| {
        ui.set_min_width(380.0);
        ui.horizontal(|ui| {
            for (t, key, how) in [
                (SizesTab::Objects, "sizes.objects", "sizes.objects_how"),
                (SizesTab::Pages, "sizes.pages", "sizes.pages_how"),
            ] {
                if ui
                    .selectable_label(*tab == t, tr(key))
                    .rich_tip(crate::ui::tips::pair(key, how))
                    .clicked()
                {
                    *tab = t;
                }
            }
        });
        ui.separator();
        let units = app.units;
        let lines = match tab {
            SizesTab::Objects => {
                sizes::objects_report(units, &sizes::object_sizes(&app.selected_rc()))
            }
            SizesTab::Pages => sizes::pages_report(units, &sizes::page_sizes(app)),
        };
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for l in &lines {
                    ui.label(RichText::new(l).monospace());
                }
            });
        if *tab == SizesTab::Objects {
            ui.label(RichText::new(tr("sizes.objects_how")).color(Tokens::text_dim()));
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .ibutton("sizes.copy")
                .rich_tip_always(crate::ui::tips::pair("sizes.copy", "sizes.copy_tip"))
                .clicked()
            {
                ui.ctx().copy_text(lines.join("\n").trim_end().to_string());
                app.status = tr("sizes.copied");
            }
            if *tab == SizesTab::Objects
                && ui
                    .add_enabled(
                        !app.selection.is_empty(),
                        crate::ui::btn_icons::Ib::new("menu.object.add_dimensions_each"),
                    )
                    .rich_tip(crate::ui::tips::pair(
                        "menu.object.add_dimensions_each",
                        "sizes.add_dimensions_tip",
                    ))
                    .clicked()
            {
                app.add_dimensions(true);
                // The dimensions are the selection now: back to the drawing.
                *close = true;
            }
            if ui
                .button(tr("dialog.close"))
                .rich_tip(crate::ui::tips::keyed("dialog.close"))
                .clicked()
            {
                *close = true;
            }
        });
    });
}
