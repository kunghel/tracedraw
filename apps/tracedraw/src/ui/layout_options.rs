//! Rulers, grids and guidelines: the Rulers, Grid and Guidelines pages of
//! Document Setup and the Guidelines docker. Every document change goes
//! through a command (SetMetadata for the settings, the guideline commands
//! for guidelines).

use crate::ui::btn_icons::IconUi;
use crate::app::{App, Units};
use crate::guides::GuidePreset;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::Ui;
use tracedraw_core::{
    document::{GridDisplay, Guide, GuideLine, GuideStyle, Metadata},
    geometry::Point,
    Color, Command,
};

/// What the Guidelines docker's form edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuideKind {
    #[default]
    Horizontal,
    Vertical,
    Angled,
}

impl GuideKind {
    pub const ALL: [GuideKind; 3] = [
        GuideKind::Horizontal,
        GuideKind::Vertical,
        GuideKind::Angled,
    ];

    pub fn key(self) -> &'static str {
        match self {
            GuideKind::Horizontal => "options.horizontal",
            GuideKind::Vertical => "options.vertical",
            GuideKind::Angled => "options.angled",
        }
    }

    pub fn of(line: &GuideLine) -> GuideKind {
        match line {
            GuideLine::Horizontal { .. } => GuideKind::Horizontal,
            GuideLine::Vertical { .. } => GuideKind::Vertical,
            GuideLine::Angled { .. } => GuideKind::Angled,
        }
    }
}

/// The Guidelines docker's entry fields, counted from the ruler origin
/// (mm) and in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GuideForm {
    pub kind: GuideKind,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
}

impl GuideForm {
    /// The guideline the form describes, in page coordinates.
    pub fn line(&self, app: &App) -> GuideLine {
        let p = app.from_ruler(Point::new(self.x, self.y));
        match self.kind {
            GuideKind::Horizontal => GuideLine::Horizontal { y: p.y },
            GuideKind::Vertical => GuideLine::Vertical { x: p.x },
            GuideKind::Angled => GuideLine::Angled {
                x: p.x,
                y: p.y,
                angle: self.angle,
            },
        }
    }

    /// Fill the form from a guideline.
    pub fn load(&mut self, app: &App, line: &GuideLine) {
        self.kind = GuideKind::of(line);
        let (o, _) = line.point_and_direction();
        let r = app.to_ruler(o);
        match line {
            GuideLine::Horizontal { .. } => self.y = r.y,
            GuideLine::Vertical { .. } => self.x = r.x,
            GuideLine::Angled { angle, .. } => {
                self.x = r.x;
                self.y = r.y;
                self.angle = *angle;
            }
        }
    }
}

/// The presets of Document Setup > Guidelines > Presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedPreset {
    OneInchMargins,
    BleedArea,
    PageBorders,
    PrintableArea,
    ThreeColumnNewsletter,
    BasicGrid,
    UpperLeftGrid,
}

impl NamedPreset {
    pub const ALL: [NamedPreset; 7] = [
        NamedPreset::OneInchMargins,
        NamedPreset::BleedArea,
        NamedPreset::PageBorders,
        NamedPreset::PrintableArea,
        NamedPreset::ThreeColumnNewsletter,
        NamedPreset::BasicGrid,
        NamedPreset::UpperLeftGrid,
    ];

    fn key(self) -> &'static str {
        match self {
            NamedPreset::OneInchMargins => "guides.preset_one_inch_margins",
            NamedPreset::BleedArea => "guides.preset_bleed_area",
            NamedPreset::PageBorders => "guides.preset_page_borders",
            NamedPreset::PrintableArea => "guides.preset_printable_area",
            NamedPreset::ThreeColumnNewsletter => "guides.preset_three_columns",
            NamedPreset::BasicGrid => "guides.preset_basic_grid",
            NamedPreset::UpperLeftGrid => "guides.preset_upper_left_grid",
        }
    }

    /// The guideline sets this preset adds, for a page of `size` with
    /// `bleed` around it (mm).
    pub fn presets(self, size: tracedraw_core::geometry::Size, bleed: f64) -> Vec<GuidePreset> {
        let inch = 25.4;
        match self {
            NamedPreset::OneInchMargins => vec![GuidePreset::Margins {
                top: inch,
                bottom: inch,
                left: inch,
                right: inch,
            }],
            NamedPreset::BleedArea => vec![GuidePreset::Margins {
                top: -bleed,
                bottom: -bleed,
                left: -bleed,
                right: -bleed,
            }],
            NamedPreset::PageBorders => vec![GuidePreset::Margins {
                top: 0.0,
                bottom: 0.0,
                left: 0.0,
                right: 0.0,
            }],
            NamedPreset::PrintableArea => vec![GuidePreset::Margins {
                top: 5.0,
                bottom: 5.0,
                left: 5.0,
                right: 5.0,
            }],
            NamedPreset::ThreeColumnNewsletter => vec![
                GuidePreset::Margins {
                    top: inch,
                    bottom: inch,
                    left: inch,
                    right: inch,
                },
                GuidePreset::Columns {
                    count: 3,
                    gutter: inch / 4.0,
                },
            ],
            NamedPreset::BasicGrid => vec![GuidePreset::Grid {
                rows: (size.height / inch).floor().max(1.0) as u32,
                columns: (size.width / inch).floor().max(1.0) as u32,
            }],
            NamedPreset::UpperLeftGrid => vec![GuidePreset::Grid {
                rows: 4,
                columns: 4,
            }],
        }
    }
}

/// Lines of a named preset for a page; columns are inset by the margins
/// of the newsletter preset, the upper-left grid covers the page's
/// upper-left quarter.
pub fn preset_lines(
    preset: NamedPreset,
    size: tracedraw_core::geometry::Size,
    bleed: f64,
) -> Vec<GuideLine> {
    let inch = 25.4;
    match preset {
        NamedPreset::ThreeColumnNewsletter => {
            let mut out = GuidePreset::Margins {
                top: inch,
                bottom: inch,
                left: inch,
                right: inch,
            }
            .lines(size);
            let inner = tracedraw_core::geometry::Size::new(
                (size.width - 2.0 * inch).max(0.0),
                size.height,
            );
            for l in (GuidePreset::Columns {
                count: 3,
                gutter: inch / 4.0,
            })
            .lines(inner)
            {
                if let GuideLine::Vertical { x } = l {
                    out.push(GuideLine::Vertical { x: x + inch });
                }
            }
            out
        }
        NamedPreset::UpperLeftGrid => {
            let quarter = tracedraw_core::geometry::Size::new(size.width / 2.0, size.height / 2.0);
            GuidePreset::Grid {
                rows: 4,
                columns: 4,
            }
            .lines(quarter)
            .into_iter()
            .map(|l| match l {
                GuideLine::Horizontal { y } => GuideLine::Horizontal {
                    y: y + size.height / 2.0,
                },
                other => other,
            })
            .collect()
        }
        other => other
            .presets(size, bleed)
            .into_iter()
            .flat_map(|p| p.lines(size))
            .collect(),
    }
}

/// State of the Guidelines page's preset section.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetForm {
    pub user_defined: bool,
    pub named: Vec<NamedPreset>,
    pub margins: bool,
    pub margin_top: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
    pub margin_right: f64,
    /// Which margins follow another (#3916).
    pub margin_mirror: EdgeMirror,
    /// Guidelines outside the page edges, at these distances (#3916).
    pub bleed: bool,
    pub bleed_top: f64,
    pub bleed_bottom: f64,
    pub bleed_left: f64,
    pub bleed_right: f64,
    pub bleed_mirror: EdgeMirror,
    pub columns: bool,
    pub column_count: u32,
    pub column_gutter: f64,
    pub grid: bool,
    pub grid_rows: u32,
    pub grid_columns: u32,
    /// A modular grid: the space between one row and the next, and
    /// between one column and the next (a guideline on each side of it).
    pub grid_row_gutter: f64,
    pub grid_column_gutter: f64,
    /// The grid laid inside the margins (when Margins is on) instead of
    /// over the whole page.
    pub grid_in_margins: bool,
    /// The pages Apply Presets adds the guidelines to (#4118).
    pub scope: PresetScope,
}

/// The pages Apply Presets adds its guidelines to (#4118).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresetScope {
    /// The guidelines being edited: the active page's, or every page's
    /// while those are chosen.
    #[default]
    Current,
    /// Every page's own, the left-hand pages mirrored (inside and outside
    /// margins swapped), as for a book.
    AllMirrored,
    OddPages,
    EvenPages,
}

impl PresetScope {
    pub const ALL: [PresetScope; 4] = [
        PresetScope::Current,
        PresetScope::AllMirrored,
        PresetScope::OddPages,
        PresetScope::EvenPages,
    ];

    pub fn key(self) -> &'static str {
        match self {
            PresetScope::Current => "guides.scope_current",
            PresetScope::AllMirrored => "guides.scope_mirrored",
            PresetScope::OddPages => "guides.scope_odd",
            PresetScope::EvenPages => "guides.scope_even",
        }
    }
}

impl Default for PresetForm {
    fn default() -> Self {
        PresetForm {
            user_defined: false,
            named: Vec::new(),
            margins: true,
            margin_top: 12.7,
            margin_bottom: 12.7,
            margin_left: 12.7,
            margin_right: 12.7,
            margin_mirror: EdgeMirror::None,
            bleed: false,
            bleed_top: 3.0,
            bleed_bottom: 3.0,
            bleed_left: 3.0,
            bleed_right: 3.0,
            bleed_mirror: EdgeMirror::None,
            columns: false,
            column_count: 2,
            column_gutter: 6.35,
            grid: false,
            grid_rows: 4,
            grid_columns: 4,
            grid_row_gutter: 0.0,
            grid_column_gutter: 0.0,
            grid_in_margins: false,
            scope: PresetScope::Current,
        }
    }
}

/// Which of four edge distances follow another one (#3916): none, the
/// bottom the top, the right the left, or all four the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeMirror {
    None,
    TopBottom,
    LeftRight,
    All,
}

impl EdgeMirror {
    pub const ALL: [EdgeMirror; 4] = [
        EdgeMirror::None,
        EdgeMirror::TopBottom,
        EdgeMirror::LeftRight,
        EdgeMirror::All,
    ];

    pub fn key(self) -> &'static str {
        match self {
            EdgeMirror::None => "guides.mirror_none",
            EdgeMirror::TopBottom => "guides.mirror_top_bottom",
            EdgeMirror::LeftRight => "guides.mirror_left_right",
            EdgeMirror::All => "guides.mirror_all",
        }
    }

    /// The distances (top, bottom, left, right) with the followers set
    /// from the ones they follow.
    pub fn apply(self, [top, bottom, left, right]: [f64; 4]) -> [f64; 4] {
        match self {
            EdgeMirror::None => [top, bottom, left, right],
            EdgeMirror::TopBottom => [top, top, left, right],
            EdgeMirror::LeftRight => [top, bottom, left, left],
            EdgeMirror::All => [top, top, top, top],
        }
    }
}

/// The four edge distance fields of a preset (top, bottom, left, right),
/// with a Mirror choice; a field that follows another is greyed (#3916).
fn edge_fields(ui: &mut Ui, id: &str, units: Units, v: &mut [f64; 4], mirror: &mut EdgeMirror) {
    let mode = *mirror;
    let follows = |i: usize| match mode {
        EdgeMirror::None => false,
        EdgeMirror::TopBottom => i == 1,
        EdgeMirror::LeftRight => i == 3,
        EdgeMirror::All => i != 0,
    };
    egui::Grid::new(id)
        .num_columns(4)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for row in [[0, 2], [1, 3]] {
                for i in row {
                    let key = ["guides.top", "guides.bottom", "guides.left", "guides.right"][i];
                    row_label(ui, key);
                    ui.add_enabled_ui(!follows(i), |ui| {
                        if let Some(n) = paper_field(ui, key, units, v[i], 0.1) {
                            v[i] = n;
                        }
                    });
                }
                ui.end_row();
            }
            row_label(ui, "guides.mirror");
            egui::ComboBox::from_id_salt((id, "mirror"))
                .selected_text(tr(mirror.key()))
                .show_ui(ui, |ui| {
                    for m in EdgeMirror::ALL {
                        ui.selectable_value(mirror, m, tr(m.key()));
                    }
                })
                .response
                .rich_tip(crate::ui::tips::pair("guides.mirror", "guides.mirror_tip"));
            ui.end_row();
        });
    *v = mirror.apply(*v);
}

fn set_metadata(app: &mut App, metadata: Metadata) {
    if metadata != app.doc().metadata {
        app.run(Command::SetMetadata { metadata });
    }
}

/// A row label right-aligned in the first grid column.
fn row_label(ui: &mut Ui, key: &str) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(tr(key));
    });
}

/// A distance field in the ruler unit through the drawing scale (a
/// position, an offset, a spacing); returns the new value in mm when it
/// changed.
fn distance_field(ui: &mut Ui, tip: &str, units: Units, mm: f64, speed: f64) -> Option<f64> {
    unit_field(ui, tip, units, units.world_mm(), mm, speed, false)
}

/// A distance field that takes the focus when `focus` is set, its value
/// selected (the Guidelines panel, #2883).
fn distance_field_focused(
    ui: &mut Ui,
    tip: &str,
    units: Units,
    mm: f64,
    speed: f64,
    focus: bool,
) -> Option<f64> {
    unit_field(ui, tip, units, units.world_mm(), mm, speed, focus)
}

/// A distance field in the ruler unit as printed (page margins, columns,
/// the baseline grid), not through the drawing scale.
fn paper_field(ui: &mut Ui, tip: &str, units: Units, mm: f64, speed: f64) -> Option<f64> {
    unit_field(ui, tip, units, units.mm(), mm, speed, false)
}

fn unit_field(
    ui: &mut Ui,
    tip: &str,
    units: Units,
    unit_mm: f64,
    mm: f64,
    speed: f64,
    focus: bool,
) -> Option<f64> {
    let unit_mm = if unit_mm.is_finite() && unit_mm > 0.0 {
        unit_mm
    } else {
        1.0
    };
    let mut v = mm / unit_mm;
    let r = ui
        .add_sized(
            [110.0, 20.0],
            crate::ui::field::NumField::new(&mut v)
                .speed(speed)
                .max_decimals(4)
                .suffix(format!(" {}", units.short()))
                .focus(focus),
        )
        .rich_tip(crate::ui::tips::pair(tip, &format!("{tip}_tip")));
    r.changed().then_some(v * unit_mm)
}

fn units_combo(ui: &mut Ui, id: &str, units: &mut Units) -> bool {
    let mut changed = false;
    crate::ui::keynav::ComboBox::from_id_salt(id)
        .selected_text(units.label())
        .width(160.0)
        .show_ui(ui, |ui| {
            for u in Units::ALL {
                if ui.selectable_label(*units == u, u.label()).clicked() {
                    *units = u;
                    changed = true;
                }
            }
        });
    changed
}

/// A colour button; returns the new colour when it changed.
fn color_button(ui: &mut Ui, tip: &str, c: Color) -> Option<Color> {
    let [r, g, b] = c.to_rgb8();
    let mut rgb = [r, g, b];
    ui.color_edit_button_srgb(&mut rgb)
        .rich_tip(crate::ui::tips::pair(tip, &format!("{tip}_tip")))
        .changed()
        .then(|| Color::rgb8(rgb[0], rgb[1], rgb[2]))
}

fn section(ui: &mut Ui, key: &str) {
    ui.add_space(6.0);
    ui.label(tr(key));
    ui.separator();
}

/// Document Setup > Rulers.
pub fn rulers_page(app: &mut App, ui: &mut Ui) {
    let mut meta = app.doc().metadata.clone();
    let units = app.units;
    section(ui, "options.units_section");
    egui::Grid::new("rulers_units")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            row_label(ui, "options.units");
            crate::ui::tips::tip_over(
                ui,
                crate::ui::tips::pair("options.units", "options.units_tip"),
                |ui| {
                    let mut u = app.units;
                    if units_combo(ui, "ruler_units", &mut u) {
                        app.units = u;
                    };
                },
            );

            ui.end_row();
        });
    section(ui, "options.drawing_scale");
    drawing_scale_fields(app, ui, &mut meta);
    section(ui, "options.origin");
    egui::Grid::new("rulers_origin")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            row_label(ui, "options.horizontal_origin");
            if let Some(v) = distance_field(
                ui,
                "options.horizontal_origin",
                units,
                meta.rulers.origin_x,
                0.1,
            ) {
                meta.rulers.origin_x = v;
            }
            ui.end_row();
            row_label(ui, "options.vertical_origin");
            if let Some(v) = distance_field(
                ui,
                "options.vertical_origin",
                units,
                meta.rulers.origin_y,
                0.1,
            ) {
                meta.rulers.origin_y = v;
            }
            ui.end_row();
            // Which way the vertical ruler counts (#3872).
            row_label(ui, "options.vertical_direction");
            crate::ui::keynav::ComboBox::from_id_salt("vertical_ruler_direction")
                .selected_text(tr(if meta.rulers.vertical_from_top {
                    "options.top_to_bottom"
                } else {
                    "options.bottom_to_top"
                }))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut meta.rulers.vertical_from_top,
                        false,
                        tr("options.bottom_to_top"),
                    );
                    ui.selectable_value(
                        &mut meta.rulers.vertical_from_top,
                        true,
                        tr("options.top_to_bottom"),
                    );
                })
                .response
                .rich_tip(crate::ui::tips::keyed("options.vertical_direction"));
            ui.end_row();
        });
    section(ui, "options.tick_divisions");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let mut n = meta.rulers.tick_divisions;
        if ui
            .add_sized(
                [70.0, 20.0],
                crate::ui::field::NumField::new(&mut n).range(1..=100),
            )
            .rich_tip(crate::ui::tips::pair(
                "options.tick_divisions",
                "options.tick_divisions_tip",
            ))
            .changed()
        {
            meta.rulers.tick_divisions = n;
        }
        ui.label(tr("options.per_tick"));
    });
    section(ui, "options.nudge_section");
    egui::Grid::new("rulers_nudge")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            row_label(ui, "options.nudge_label");
            if let Some(v) = distance_field(ui, "options.nudge_label", units, app.nudge_mm, 0.01) {
                if v > 0.0 {
                    app.nudge_mm = v;
                }
            }
            ui.end_row();
            row_label(ui, "options.super_nudge");
            if let Some(v) = distance_field(
                ui,
                "options.super_nudge",
                units,
                app.settings.super_nudge_mm,
                0.01,
            ) {
                if v > 0.0 {
                    app.settings.super_nudge_mm = v;
                }
            }
            ui.end_row();
            row_label(ui, "options.micro_nudge");
            if let Some(v) = distance_field(
                ui,
                "options.micro_nudge",
                units,
                app.settings.micro_nudge_mm,
                0.001,
            ) {
                if v > 0.0 {
                    app.settings.micro_nudge_mm = v;
                }
            }
            ui.end_row();
        });
    ui.add_space(8.0);
    ui.checkbox(&mut app.show_rulers, tr("options.show_rulers"))
        .rich_tip(crate::ui::tips::pair(
            "options.show_rulers",
            "options.show_rulers_tip",
        ));
    if ui
        .ibutton("options.reset_origin")
        .rich_tip(crate::ui::tips::pair(
            "options.reset_origin",
            "options.reset_origin_tip",
        ))
        .clicked()
    {
        meta.rulers.origin_x = 0.0;
        meta.rulers.origin_y = 0.0;
    }
    set_metadata(app, meta);
}

/// Document Setup > Rulers > Drawing scale: the typical scales, then the
/// page distance and the world distance it stands for, each with its unit.
/// The world unit becomes the ruler unit.
fn drawing_scale_fields(app: &mut App, ui: &mut Ui, meta: &mut Metadata) {
    use crate::drawing_scale as ds;
    let r = &mut meta.rulers;
    let current = ds::matching_preset(r);
    let (page_unit, world_unit) = ds::units_of(r);
    let mut new_units = None;
    egui::Grid::new("drawing_scale")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            row_label(ui, "options.typical_scales");
            let shown = current
                .map(|p| p.label.to_string())
                .unwrap_or_else(|| tr("options.scale_custom"));
            let mut pick = None;
            crate::ui::keynav::ComboBox::from_id_salt("typical_scales")
                .selected_text(shown)
                .width(160.0)
                .height(320.0)
                .show_ui(ui, |ui| {
                    for p in ds::PRESETS.iter() {
                        let on = current.is_some_and(|c| c.label == p.label);
                        if ui.selectable_label(on, p.label).clicked() {
                            pick = Some(*p);
                        }
                    }
                })
                .response
                .rich_tip(crate::ui::tips::pair(
                    "options.typical_scales",
                    "options.typical_scales_tip",
                ));
            if let Some(p) = pick {
                new_units = ds::apply_preset(r, &p);
            }
            ui.end_row();
            row_label(ui, "options.page_distance");
            crate::ui::tips::tip_over(
                ui,
                crate::ui::tips::pair("options.page_distance", "options.page_distance_tip"),
                |ui| {
                    ui.horizontal(|ui| {
                        let world = world_unit.from_mm(r.scale_world);
                        let mut v = page_unit.from_mm(r.scale_page);
                        if ui
                            .add_sized(
                                [90.0, 20.0],
                                crate::ui::field::NumField::new(&mut v)
                                    .speed(0.1)
                                    .max_decimals(4)
                                    .range(0.0001..=1_000_000.0),
                            )
                            .changed()
                        {
                            ds::set(r, v, page_unit, world, world_unit);
                        }
                        let mut u = page_unit;
                        if units_combo(ui, "scale_page_unit", &mut u) {
                            ds::set(r, u.from_mm(r.scale_page), u, world, world_unit);
                        }
                    });
                },
            );

            ui.end_row();
            row_label(ui, "options.world_distance");
            crate::ui::tips::tip_over(
                ui,
                crate::ui::tips::pair("options.world_distance", "options.world_distance_tip"),
                |ui| {
                    ui.horizontal(|ui| {
                        let page = page_unit.from_mm(r.scale_page);
                        let mut v = world_unit.from_mm(r.scale_world);
                        if ui
                            .add_sized(
                                [90.0, 20.0],
                                crate::ui::field::NumField::new(&mut v)
                                    .speed(0.1)
                                    .max_decimals(4)
                                    .range(0.0001..=1_000_000_000.0),
                            )
                            .changed()
                        {
                            ds::set(r, page, page_unit, v, world_unit);
                        }
                        let mut u = world_unit;
                        if units_combo(ui, "scale_world_unit", &mut u) {
                            ds::set(r, page, page_unit, u.from_mm(r.scale_world), u);
                            new_units = Some(u);
                        }
                    });
                },
            );

            ui.end_row();
        });
    ui.label(
        egui::RichText::new(tr("options.scale_note"))
            .color(crate::theme::Tokens::text_dim())
            .size(11.0),
    );
    if let Some(u) = new_units {
        app.units = u;
    }
}

/// Document Setup > Grid.
pub fn grid_page(app: &mut App, ui: &mut Ui) {
    let mut meta = app.doc().metadata.clone();
    let units = app.units;
    section(ui, "options.document_grid");
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.show_grid, tr("options.show_grid"))
            .rich_tip(crate::ui::tips::pair(
                "options.show_grid",
                "options.show_grid_tip",
            ));
        ui.add_space(24.0);
        ui.checkbox(&mut app.snap.grid, tr("options.snap_to_grid"))
            .rich_tip(crate::ui::tips::pair(
                "options.snap_to_grid",
                "options.snap_to_grid_tip",
            ));
    });
    // Modes, presets, subdivisions, angled grids and colours (#4395).
    if ui
        .add(crate::ui::btn_icons::Ib::new("options.open_grid_panel"))
        .rich_tip(crate::ui::tips::pair(
            "options.open_grid_panel",
            "options.open_grid_panel_tip",
        ))
        .clicked()
    {
        app.open_docker(crate::app::DockerTab::Grid);
    }
    egui::Grid::new("grid_spacing")
        .num_columns(3)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            for (key, value, salt, tip) in [
                (
                    "options.horizontal",
                    &mut meta.grid.spacing_x,
                    "gx",
                    "options.grid_horizontal",
                ),
                (
                    "options.vertical",
                    &mut meta.grid.spacing_y,
                    "gy",
                    "options.grid_vertical",
                ),
            ] {
                row_label(ui, key);
                if app.grid_frequency {
                    // Lines per unit.
                    let mut f = units.world_mm() / value.max(1e-9);
                    if ui
                        .add_sized(
                            [110.0, 20.0],
                            crate::ui::field::NumField::new(&mut f)
                                .speed(0.05)
                                .max_decimals(4)
                                .range(0.0001..=10_000.0),
                        )
                        .rich_tip(crate::ui::tips::pair(tip, &format!("{tip}_tip")))
                        .changed()
                        && f > 0.0
                    {
                        *value = units.world_mm() / f;
                    }
                } else if let Some(v) = distance_field(ui, tip, units, *value, 0.1) {
                    if v > 0.0 {
                        *value = v;
                    }
                }
                let spacing = crate::i18n::trf("options.grid_spacing_of", &[("u", &units.label())]);
                let frequency =
                    crate::i18n::trf("options.grid_frequency_of", &[("u", &units.label())]);
                crate::ui::keynav::ComboBox::from_id_salt(salt)
                    .selected_text(if app.grid_frequency {
                        frequency.clone()
                    } else {
                        spacing.clone()
                    })
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut app.grid_frequency, false, spacing);
                        ui.selectable_value(&mut app.grid_frequency, true, frequency);
                    })
                    .response
                    .rich_tip(crate::ui::tips::pair(
                        "options.grid_measure",
                        "options.grid_measure_tip",
                    ));
                ui.end_row();
            }
            row_label(ui, "options.show_grid_as");
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut meta.grid.display,
                    GridDisplay::Lines,
                    tr("options.grid_lines"),
                )
                .rich_tip(crate::ui::tips::pair(
                    "options.grid_lines",
                    "options.grid_lines_tip",
                ));
                ui.radio_value(
                    &mut meta.grid.display,
                    GridDisplay::Dots,
                    tr("options.grid_dots"),
                )
                .rich_tip(crate::ui::tips::pair(
                    "options.grid_dots",
                    "options.grid_dots_tip",
                ));
            });
            ui.end_row();
        });

    section(ui, "options.baseline_grid");
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.show_baseline_grid, tr("options.show_grid"))
            .rich_tip(crate::ui::tips::pair(
                "options.show_grid",
                "options.show_grid_tip",
            ));
        ui.add_space(24.0);
        ui.checkbox(&mut app.snap.baseline_grid, tr("options.snap_to_grid"))
            .rich_tip(crate::ui::tips::pair(
                "options.snap_to_grid",
                "options.snap_to_grid_tip",
            ));
    });
    egui::Grid::new("baseline_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            row_label(ui, "options.baseline_spacing");
            crate::ui::tips::tip_over(
                ui,
                crate::ui::tips::pair("options.baseline_spacing", "options.baseline_spacing_tip"),
                |ui| {
                    // Spacing in points, as text leading is.
                    let mut pt = meta.grid.baseline_spacing * 72.0 / 25.4;
                    if ui
                        .add_sized(
                            [110.0, 20.0],
                            crate::ui::field::NumField::new(&mut pt)
                                .speed(0.1)
                                .max_decimals(2)
                                .range(0.1..=1000.0)
                                .suffix(" pt"),
                        )
                        .changed()
                    {
                        meta.grid.baseline_spacing = pt * 25.4 / 72.0;
                    };
                },
            );

            ui.end_row();
            row_label(ui, "options.baseline_start");
            if let Some(v) = paper_field(
                ui,
                "options.baseline_start",
                units,
                meta.grid.baseline_start,
                0.1,
            ) {
                meta.grid.baseline_start = v.max(0.0);
            }
            ui.end_row();
            row_label(ui, "options.line_color");
            if let Some(c) = color_button(ui, "options.line_color", meta.grid.baseline_color) {
                meta.grid.baseline_color = c;
            }
            ui.end_row();
        });

    section(ui, "options.pixel_grid");
    ui.spacing_mut().slider_width = 150.0;
    egui::Grid::new("pixel_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            row_label(ui, "options.color");
            if let Some(c) = color_button(ui, "options.color", meta.grid.pixel_color) {
                meta.grid.pixel_color = c;
            }
            ui.end_row();
            row_label(ui, "options.opacity");
            crate::ui::tips::tip_over(
                ui,
                crate::ui::tips::pair("options.opacity", "options.opacity_tip"),
                |ui| {
                    let mut o = meta.grid.pixel_opacity * 100.0;
                    if ui
                        .add(crate::ui::Slider::new(&mut o, 0.0..=100.0).suffix(" %"))
                        .changed()
                    {
                        meta.grid.pixel_opacity = (o / 100.0).clamp(0.0, 1.0);
                    };
                },
            );

            ui.end_row();
        });
    ui.checkbox(&mut app.show_pixel_grid, tr("options.pixel_grid_800"))
        .rich_tip(crate::ui::tips::pair(
            "options.pixel_grid_800",
            "options.pixel_grid_800_tip",
        ));
    ui.checkbox(&mut app.snap.pixels, tr("options.snap_to_pixels"))
        .rich_tip(crate::ui::tips::pair(
            "options.snap_to_pixels",
            "options.snap_to_pixels_tip",
        ));
    set_metadata(app, meta);
}

/// Document Setup > Guidelines: visibility, snapping, default colours,
/// presets, and every guideline of the page.
pub fn guidelines_page(app: &mut App, ui: &mut Ui) {
    let mut meta = app.doc().metadata.clone();
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.show_guides, tr("options.show_guidelines"))
            .rich_tip(crate::ui::tips::pair(
                "options.show_guidelines",
                "options.show_guidelines_tip",
            ));
        ui.add_space(24.0);
        ui.checkbox(&mut app.snap.guides, tr("options.snap_to_guidelines"))
            .rich_tip(crate::ui::tips::pair(
                "options.snap_to_guidelines",
                "options.snap_to_guidelines_tip",
            ));
    });
    egui::Grid::new("guide_colors")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            row_label(ui, "options.default_guide_color");
            if let Some(c) = color_button(ui, "options.default_guide_color", meta.guides.color) {
                meta.guides.color = c;
            }
            ui.end_row();
            row_label(ui, "options.default_preset_guide_color");
            if let Some(c) = color_button(
                ui,
                "options.default_preset_guide_color",
                meta.guides.preset_color,
            ) {
                meta.guides.preset_color = c;
            }
            ui.end_row();
        });
    set_metadata(app, meta);

    section(ui, "options.guide_presets");
    let mut form = std::mem::take(&mut app.guide_presets);
    ui.horizontal(|ui| {
        ui.radio_value(&mut form.user_defined, false, tr("guides.presets"))
            .rich_tip(crate::ui::tips::pair(
                "guides.presets",
                "guides.presets_tip",
            ));
        ui.radio_value(&mut form.user_defined, true, tr("guides.user_defined"))
            .rich_tip(crate::ui::tips::pair(
                "guides.user_defined",
                "guides.user_defined_tip",
            ));
    });
    let units = app.units;
    if form.user_defined {
        ui.checkbox(&mut form.margins, tr("guides.margins"))
            .rich_tip(crate::ui::tips::pair(
                "guides.margins",
                "guides.margins_tip",
            ));
        ui.add_enabled_ui(form.margins, |ui| {
            let mut v = [
                form.margin_top,
                form.margin_bottom,
                form.margin_left,
                form.margin_right,
            ];
            edge_fields(ui, "preset_margins", units, &mut v, &mut form.margin_mirror);
            [
                form.margin_top,
                form.margin_bottom,
                form.margin_left,
                form.margin_right,
            ] = v;
        });
        ui.checkbox(&mut form.columns, tr("guides.columns"))
            .rich_tip(crate::ui::tips::pair(
                "guides.columns",
                "guides.columns_tip",
            ));
        ui.add_enabled_ui(form.columns, |ui| {
            egui::Grid::new("preset_columns")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    row_label(ui, "guides.number_of_columns");
                    crate::ui::tips::tip_over(
                        ui,
                        crate::ui::tips::pair(
                            "guides.number_of_columns",
                            "guides.number_of_columns_tip",
                        ),
                        |ui| {
                            ui.add_sized(
                                [70.0, 20.0],
                                crate::ui::field::NumField::new(&mut form.column_count)
                                    .range(1..=100),
                            );
                        },
                    );

                    ui.end_row();
                    row_label(ui, "guides.distance_between");
                    if let Some(v) = paper_field(
                        ui,
                        "guides.distance_between",
                        units,
                        form.column_gutter,
                        0.1,
                    ) {
                        form.column_gutter = v.max(0.0);
                    }
                    ui.end_row();
                });
        });
        ui.checkbox(&mut form.grid, tr("guides.grid"))
            .rich_tip(crate::ui::tips::pair("guides.grid", "guides.grid_tip"));
        ui.add_enabled_ui(form.grid, |ui| {
            egui::Grid::new("preset_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    row_label(ui, "guides.rows");
                    crate::ui::tips::tip_over(
                        ui,
                        crate::ui::tips::pair("guides.rows", "guides.rows_tip"),
                        |ui| {
                            ui.add_sized(
                                [70.0, 20.0],
                                crate::ui::field::NumField::new(&mut form.grid_rows).range(1..=200),
                            );
                        },
                    );

                    ui.end_row();
                    row_label(ui, "guides.columns_count");
                    crate::ui::tips::tip_over(
                        ui,
                        crate::ui::tips::pair("guides.columns_count", "guides.columns_count_tip"),
                        |ui| {
                            ui.add_sized(
                                [70.0, 20.0],
                                crate::ui::field::NumField::new(&mut form.grid_columns)
                                    .range(1..=200),
                            );
                        },
                    );

                    ui.end_row();
                    row_label(ui, "guides.row_gutter");
                    if let Some(v) =
                        paper_field(ui, "guides.row_gutter", units, form.grid_row_gutter, 0.1)
                    {
                        form.grid_row_gutter = v.max(0.0);
                    }
                    ui.end_row();
                    row_label(ui, "guides.column_gutter");
                    if let Some(v) = paper_field(
                        ui,
                        "guides.column_gutter",
                        units,
                        form.grid_column_gutter,
                        0.1,
                    ) {
                        form.grid_column_gutter = v.max(0.0);
                    }
                    ui.end_row();
                });
            ui.add_enabled(
                form.margins,
                egui::Checkbox::new(&mut form.grid_in_margins, tr("guides.grid_in_margins")),
            )
            .rich_tip(crate::ui::tips::pair(
                "guides.grid_in_margins",
                "guides.grid_in_margins_tip",
            ));
        });
        ui.checkbox(&mut form.bleed, tr("guides.bleed"))
            .rich_tip(crate::ui::tips::pair("guides.bleed", "guides.bleed_tip"));
        ui.add_enabled_ui(form.bleed, |ui| {
            let mut v = [
                form.bleed_top,
                form.bleed_bottom,
                form.bleed_left,
                form.bleed_right,
            ];
            edge_fields(ui, "preset_bleed", units, &mut v, &mut form.bleed_mirror);
            [
                form.bleed_top,
                form.bleed_bottom,
                form.bleed_left,
                form.bleed_right,
            ] = v;
        });
    } else {
        for p in NamedPreset::ALL {
            let mut on = form.named.contains(&p);
            if ui
                .checkbox(&mut on, tr(p.key()))
                .rich_tip_with(|| format!("{}\n{}", tr(p.key()), tr(&format!("{}_tip", p.key()))))
                .changed()
            {
                if on {
                    form.named.push(p);
                } else {
                    form.named.retain(|q| *q != p);
                }
            }
        }
    }
    // The pages the guidelines go to (#4118).
    ui.horizontal(|ui| {
        ui.label(tr("guides.scope"));
        crate::ui::keynav::ComboBox::from_id_salt("preset_scope")
            .selected_text(tr(form.scope.key()))
            .show_ui(ui, |ui| {
                for s in PresetScope::ALL {
                    ui.selectable_value(&mut form.scope, s, tr(s.key()));
                }
            })
            .response
            .rich_tip(crate::ui::tips::pair("guides.scope", "guides.scope_tip"));
    });
    if ui
        .ibutton("guides.apply_presets")
        .rich_tip(crate::ui::tips::pair(
            "guides.apply_presets",
            "guides.apply_presets_tip",
        ))
        .clicked()
    {
        apply_presets(app, &form);
    }
    app.guide_presets = form;

    section(ui, "options.guidelines_list");
    guide_list(app, ui, None, 220.0);
}

/// The guidelines a preset form describes for a page of `size` with
/// `bleed` round it, in `color`; margin and bleed guidelines keep the page
/// edge they are measured from, so they follow it when the page is
/// resized (#4268).
pub fn preset_guides(
    form: &PresetForm,
    size: tracedraw_core::geometry::Size,
    bleed: f64,
    color: tracedraw_core::Color,
) -> Vec<Guide> {
    let mut out: Vec<Guide> = Vec::new();
    let mut push = |line: GuideLine, anchor: Option<tracedraw_core::document::GuideAnchor>| {
        if !out.iter().any(|g| g.line == line) {
            let mut g = Guide::from(line);
            g.color = Some(color);
            g.anchor = anchor;
            out.push(g);
        }
    };
    type Anchor = Option<tracedraw_core::document::GuideAnchor>;
    let margins = |preset: GuidePreset, push: &mut dyn FnMut(GuideLine, Anchor)| {
        let anchors = preset.anchors();
        for (i, line) in preset.lines(size).into_iter().enumerate() {
            push(line, anchors.map(|a| a[i]));
        }
    };
    if form.user_defined {
        if form.margins {
            let [top, bottom, left, right] = form.margin_mirror.apply([
                form.margin_top,
                form.margin_bottom,
                form.margin_left,
                form.margin_right,
            ]);
            margins(
                GuidePreset::Margins {
                    top,
                    bottom,
                    left,
                    right,
                },
                &mut push,
            );
        }
        if form.bleed {
            // Margins of minus the bleed: the lines stand outside the page.
            let [top, bottom, left, right] = form.bleed_mirror.apply([
                form.bleed_top,
                form.bleed_bottom,
                form.bleed_left,
                form.bleed_right,
            ]);
            margins(
                GuidePreset::Margins {
                    top: -top.max(0.0),
                    bottom: -bottom.max(0.0),
                    left: -left.max(0.0),
                    right: -right.max(0.0),
                },
                &mut push,
            );
        }
        if form.columns {
            // Columns between the margins when margins are on.
            let (l, r) = if form.margins {
                (form.margin_left, form.margin_right)
            } else {
                (0.0, 0.0)
            };
            let inner =
                tracedraw_core::geometry::Size::new((size.width - l - r).max(0.0), size.height);
            for line in (GuidePreset::Columns {
                count: form.column_count,
                gutter: form.column_gutter,
            })
            .lines(inner)
            {
                if let GuideLine::Vertical { x } = line {
                    push(GuideLine::Vertical { x: x + l }, None);
                }
            }
        }
        if form.grid {
            // A modular grid: inside the margins when asked, with the
            // gutters between its rows and columns.
            let area = if form.grid_in_margins && form.margins {
                let [top, bottom, left, right] = form.margin_mirror.apply([
                    form.margin_top,
                    form.margin_bottom,
                    form.margin_left,
                    form.margin_right,
                ]);
                tracedraw_core::geometry::Rect::new(
                    left,
                    bottom,
                    (size.width - right).max(left),
                    (size.height - top).max(bottom),
                )
            } else {
                tracedraw_core::geometry::Rect::new(0.0, 0.0, size.width, size.height)
            };
            for line in crate::guides::modular_grid_lines(
                area,
                form.grid_rows,
                form.grid_columns,
                form.grid_row_gutter,
                form.grid_column_gutter,
            ) {
                push(line, None);
            }
        }
    } else {
        for p in &form.named {
            // The margin presets keep their edges; the others are laid
            // out as they are.
            let all_margins = p
                .presets(size, bleed)
                .iter()
                .all(|q| matches!(q, GuidePreset::Margins { .. }));
            if all_margins {
                for q in p.presets(size, bleed) {
                    margins(q, &mut push);
                }
            } else {
                for line in preset_lines(*p, size, bleed) {
                    push(line, None);
                }
            }
        }
    }
    out
}

/// A guideline as it lies on a page facing the other way: the left and
/// right sides swapped (#4118).
fn mirrored_guide(g: &Guide, size: tracedraw_core::geometry::Size) -> Guide {
    let mut g = g.clone();
    match g.anchor {
        Some(a) => {
            let a = a.mirrored();
            g.anchor = Some(a);
            g.line = a.line(size);
        }
        None => {
            if let GuideLine::Vertical { x } = g.line {
                g.line = GuideLine::Vertical { x: size.width - x };
            }
        }
    }
    g
}

/// Add the guidelines a preset form describes, in the preset colour, to
/// the pages its scope names (#4118): the guidelines being edited (the
/// active page's or every page's), or the own guidelines of every page
/// (the left-hand pages of facing pages mirrored, inside and outside
/// swapped), of the odd pages or of the even pages, each laid out on its
/// page's size. One undo step.
pub fn apply_presets(app: &mut App, form: &PresetForm) {
    let bleed = app.doc().metadata.bleed.max(0.0);
    let color = app.doc().metadata.guides.preset_color;
    if form.scope == PresetScope::Current {
        let size = app.page_size();
        let mut guides = app.page_guides();
        for g in preset_guides(form, size, bleed, color) {
            if !guides.iter().any(|o| o.line == g.line) {
                guides.push(g);
            }
        }
        app.set_guides(guides, "Add Guideline");
        return;
    }
    let start = app.doc().metadata.facing_start;
    let mut cmds = Vec::new();
    for (k, page) in app.doc().pages.iter().enumerate() {
        let take = match form.scope {
            PresetScope::OddPages => k % 2 == 0,
            PresetScope::EvenPages => k % 2 == 1,
            _ => true,
        };
        if !take {
            continue;
        }
        let left = tracedraw_core::spreads::side_of(k, start) == tracedraw_core::spreads::PageSide::Left;
        let mut guides = page.guides.clone();
        for g in preset_guides(form, page.size, bleed, color) {
            let g = if form.scope == PresetScope::AllMirrored && left {
                mirrored_guide(&g, page.size)
            } else {
                g
            };
            if !guides.iter().any(|o| o.line == g.line) {
                guides.push(g);
            }
        }
        if guides != page.guides {
            cmds.push(Command::SetGuides {
                page: page.id,
                guides,
            });
        }
    }
    if cmds.is_empty() {
        return;
    }
    if let Err(e) = app.engine.run_batch("Add Guideline", &cmds) {
        app.status = format!("Add Guideline: {e}");
    }
}

/// Text for a guideline in the lists: its position from the ruler origin
/// (and angle).
pub(crate) fn guide_text(app: &App, g: &Guide) -> String {
    let u = app.units;
    let (o, _) = g.line.point_and_direction();
    let r = app.to_ruler(o);
    let fmt = |v: f64| format!("{:.3} {}", u.world_from_mm(v), u.short());
    match g.line {
        GuideLine::Horizontal { .. } => fmt(r.y),
        GuideLine::Vertical { .. } => fmt(r.x),
        GuideLine::Angled { angle, .. } => format!("{}, {}, {:.1}°", fmt(r.x), fmt(r.y), angle),
    }
}

/// A selectable list of the page's guidelines (only those of `kind` when
/// given). Clicking selects the guideline on the page too.
fn guide_list(app: &mut App, ui: &mut Ui, kind: Option<GuideKind>, height: f32) {
    let guides = app.page_guides();
    let frame = egui::Frame::new()
        .fill(crate::theme::Tokens::field())
        .stroke(egui::Stroke::new(1.0, Tokens::control_border()))
        .inner_margin(egui::Margin::same(2));
    frame.show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(("guide_list", kind.map(|k| k as u8)))
            .max_height(height)
            .min_scrolled_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let shown: Vec<usize> = guides
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| kind.is_none_or(|k| k == GuideKind::of(&g.line)))
                    .map(|(i, _)| i)
                    .collect();
                for (i, g) in guides.iter().enumerate() {
                    if kind.is_some_and(|k| k != GuideKind::of(&g.line)) {
                        continue;
                    }
                    let selected = app.selected_guides.contains(&i);
                    let mut text = guide_text(app, g);
                    if kind.is_none() {
                        text = format!("{}  {}", tr(GuideKind::of(&g.line).key()), text);
                    }
                    if g.locked {
                        text.push_str(&format!("  ({})", tr("guides.locked")));
                    }
                    let r = ui.selectable_label(selected, text);
                    if r.clicked() {
                        let (ctrl, shift) =
                            ui.input(|i| (i.modifiers.command, i.modifiers.shift));
                        // Shift+click: every guideline listed from the one
                        // clicked last to this one (#2883).
                        let range = shift
                            .then(|| shift_range(&shown, &app.selected_guides, i, ctrl))
                            .flatten();
                        if let Some(sel) = range {
                            app.selected_guides = sel;
                        } else if ctrl {
                            if selected {
                                app.selected_guides.retain(|j| *j != i);
                            } else {
                                app.selected_guides.push(i);
                            }
                        } else {
                            app.selected_guides = vec![i];
                        }
                        app.guide_rotate = None;
                        let mut form = app.guide_form;
                        form.load(app, &g.line);
                        app.guide_form = form;
                    }
                }
            });
    });
}

/// The guidelines a Shift+click on `clicked` selects in a list showing
/// `shown` (#2883): those listed from the anchor (the one selected last)
/// to it, added to `selected` with Ctrl; the anchor stays last, for the
/// next Shift+click. `None` without an anchor in the list.
fn shift_range(shown: &[usize], selected: &[usize], clicked: usize, ctrl: bool) -> Option<Vec<usize>> {
    let a = selected.last().and_then(|a| shown.iter().position(|j| j == a))?;
    let h = shown.iter().position(|j| *j == clicked)?;
    let (lo, hi) = (a.min(h), a.max(h));
    let anchor = shown[a];
    let mut sel: Vec<usize> = if ctrl { selected.to_vec() } else { Vec::new() };
    for j in &shown[lo..=hi] {
        if !sel.contains(j) {
            sel.push(*j);
        }
    }
    sel.retain(|j| *j != anchor);
    sel.push(anchor);
    Some(sel)
}

/// Window > Panels > Guidelines: show and snap toggles, the guideline type, position fields,
/// Add and Modify, the list, delete and lock, colour and style.
pub fn guidelines_docker(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.itoggle(&mut app.show_guides, "guides.show")
            .rich_tip(tr("guides.show_tip"));
        ui.itoggle(&mut app.snap.guides, "guides.snap")
            .rich_tip(tr("guides.snap_tip"));
    });
    ui.add_space(4.0);
    app.sync_guide_form();
    let mut form = app.guide_form;
    // A value typed in x, y or angle moves the one selected guideline
    // as it is entered (Enter or leaving the field); Modify stays for
    // a change of type.
    let mut moved = false;
    ui.horizontal(|ui| {
        ui.label(tr("guides.type"));
        crate::ui::keynav::ComboBox::from_id_salt("guide_type")
            .selected_text(tr(form.kind.key()))
            .width(140.0)
            .show_ui(ui, |ui| {
                for k in GuideKind::ALL {
                    ui.selectable_value(&mut form.kind, k, tr(k.key()));
                }
            })
            .response
            .rich_tip(crate::ui::tips::pair("guides.type", "guides.type_desc"));
    });
    let units = app.units;
    // After a double click on a guideline its position field takes the
    // focus, the value selected, ready to be typed over (#2883): y for a
    // horizontal guideline, x for the others.
    let focus = std::mem::take(&mut app.guide_focus_field);
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    egui::Grid::new("guide_form")
        .num_columns(2)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            if form.kind != GuideKind::Horizontal {
                row_label(ui, "guides.x");
                let x = crate::ui::tips::tip_over(
                    ui,
                    crate::ui::tips::pair("toolbar.object_x", "guides.x_desc"),
                    |ui| distance_field_focused(ui, "guides.x", units, form.x, 0.1, focus),
                );
                if let Some(v) = x {
                    form.x = v;
                    moved = true;
                }
                ui.end_row();
            }
            if form.kind != GuideKind::Vertical {
                row_label(ui, "guides.y");
                let y = crate::ui::tips::tip_over(
                    ui,
                    crate::ui::tips::pair("toolbar.object_y", "guides.y_desc"),
                    |ui| {
                        let focus = focus && form.kind == GuideKind::Horizontal;
                        distance_field_focused(ui, "guides.y", units, form.y, 0.1, focus)
                    },
                );
                if let Some(v) = y {
                    form.y = v;
                    moved = true;
                }
                ui.end_row();
            }
            if form.kind == GuideKind::Angled {
                row_label(ui, "guides.angle");
                if ui
                    .add_sized(
                        [110.0, 20.0],
                        crate::ui::field::NumField::new(&mut form.angle)
                            .speed(1.0)
                            .max_decimals(2)
                            .suffix("°"),
                    )
                    .rich_tip(crate::ui::tips::pair("guides.angle", "guides.angle_desc"))
                    .changed()
                {
                    moved = true;
                }
                ui.end_row();
            }
        });
    app.guide_form = form;
    // Alt+A adds and Alt+M modifies from the keyboard, the hands kept off
    // the mouse (#2883).
    let (alt_add, alt_modify) = ui.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::ALT, egui::Key::A),
            i.consume_key(egui::Modifiers::ALT, egui::Key::M),
        )
    });
    let mut add = alt_add;
    if moved {
        let line = form.line(app);
        // Enter in a field with no guideline selected adds one there.
        if app.selected_guides.is_empty() {
            add |= enter;
        } else {
            app.move_guide_from_form(line);
        }
    }
    ui.horizontal(|ui| {
        if ui
            .ibutton("guides.add")
            .rich_tip(crate::ui::tips::pair("guides.add", "guides.add_desc"))
            .clicked()
        {
            add = true;
        }
        if add {
            let line = form.line(app);
            app.add_guide_from_form(line);
        }
        let single = match app.selected_guides.as_slice() {
            [i] => Some(*i),
            _ => None,
        };
        if ui
            .add_enabled(single.is_some(), crate::ui::btn_icons::Ib::new("guides.modify"))
            .rich_tip(crate::ui::tips::pair("guides.modify", "guides.modify_desc"))
            .rich_disabled_tip(crate::ui::tips::pair("guides.modify", "guides.modify_desc"))
            .clicked()
            || (alt_modify && single.is_some())
        {
            if let Some(i) = single {
                if let Some(g) = app.page_guides().get(i).cloned() {
                    if g.locked {
                        app.status = tr("status.guideline_locked");
                    } else {
                        let line = form.line(app);
                        app.replace_guide(i, g.with_line(line), "Move Guideline");
                    }
                }
            }
        }
    });
    ui.add_space(4.0);
    guide_list(app, ui, Some(form.kind), 160.0);
    ui.add_space(4.0);
    let any = !app.selected_guides.is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(any, crate::ui::btn_icons::Ib::new("guides.delete"))
            .rich_tip(crate::ui::tips::pair("guides.delete", "guides.delete_desc"))
            .rich_disabled_tip(crate::ui::tips::pair("guides.delete", "guides.delete_desc"))
            .clicked()
            && !app.delete_selected_guides()
        {
            app.status = tr("status.guideline_locked");
        }
        // Copies of the selected guidelines, moved by the duplicate offset.
        if ui
            .add_enabled(any, crate::ui::btn_icons::Ib::new("guides.duplicate"))
            .rich_tip(tr("guides.duplicate_tip"))
            .clicked()
        {
            app.duplicate_selected_guides();
        }
        // Every guideline of the type shown, one undo step.
        let of_kind = app
            .page_guides()
            .iter()
            .any(|g| GuideKind::of(&g.line) == form.kind);
        if ui
            .add_enabled(of_kind, crate::ui::btn_icons::Ib::new("guides.clear"))
            .rich_tip(tr("guides.clear_tip"))
            .clicked()
        {
            app.clear_guides_of_kind(form.kind);
        }
        let guides = app.page_guides();
        let all_locked = any
            && app
                .selected_guides
                .iter()
                .all(|i| guides.get(*i).is_some_and(|g| g.locked));
        let label = if all_locked {
            "guides.unlock"
        } else {
            "guides.lock"
        };
        let lock_tip = crate::ui::tips::pair("guides.lock_tip", "guides.lock_desc");
        if ui
            .add_enabled(any, crate::ui::btn_icons::Ib::new(label))
            .rich_tip(&lock_tip)
            .rich_disabled_tip(&lock_tip)
            .clicked()
        {
            let sel = app.selected_guides.clone();
            app.set_guides_locked(&sel, !all_locked);
        }
    });
    // Colour and style of the selected guidelines (or the default colour
    // when none is selected).
    let guides = app.page_guides();
    let first = app
        .selected_guides
        .first()
        .and_then(|i| guides.get(*i))
        .cloned();
    egui::Grid::new("guide_look")
        .num_columns(2)
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            row_label(ui, "guides.color");
            let current = first
                .as_ref()
                .map(|g| app.guide_color(g))
                .unwrap_or(app.doc().metadata.guides.color);
            let picked = crate::ui::tips::tip_over(
                ui,
                crate::ui::tips::pair("guides.color", "guides.color_desc"),
                |ui| color_button(ui, "guides.color", current),
            );
            if let Some(c) = picked {
                if any {
                    app.edit_selected_guides("Guideline Color", |g| g.color = Some(c));
                } else {
                    let mut meta = app.doc().metadata.clone();
                    meta.guides.color = c;
                    set_metadata(app, meta);
                }
            }
            ui.end_row();
            row_label(ui, "guides.style");
            let style = first.as_ref().map(|g| g.style).unwrap_or_default();
            ui.add_enabled_ui(any, |ui| {
                crate::ui::keynav::ComboBox::from_id_salt("guide_style")
                    .selected_text(tr(style_key(style)))
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for s in GuideStyle::ALL {
                            if ui.selectable_label(s == style, tr(style_key(s))).clicked() {
                                app.edit_selected_guides("Guideline Style", |g| g.style = s);
                            }
                        }
                    })
                    .response
                    .rich_tip(crate::ui::tips::pair("guides.style", "guides.style_desc"))
                    .rich_disabled_tip(crate::ui::tips::pair("guides.style", "guides.style_desc"));
            });
            ui.end_row();
        });
}

fn style_key(s: GuideStyle) -> &'static str {
    match s {
        GuideStyle::Solid => "guides.style_solid",
        GuideStyle::Dashed => "guides.style_dashed",
        GuideStyle::Dotted => "guides.style_dotted",
        GuideStyle::DashDot => "guides.style_dash_dot",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Size;

    #[test]
    fn the_form_counts_from_the_ruler_origin() {
        let mut app = App::headless();
        app.set_ruler_origin(Point::new(10.0, 20.0));
        let form = GuideForm {
            kind: GuideKind::Horizontal,
            x: 0.0,
            y: 5.0,
            angle: 0.0,
        };
        assert_eq!(form.line(&app), GuideLine::Horizontal { y: 25.0 });
        let mut back = GuideForm::default();
        back.load(&app, &GuideLine::Vertical { x: 13.0 });
        assert_eq!(back.kind, GuideKind::Vertical);
        assert!((back.x - 3.0).abs() < 1e-9);
    }

    /// Shift+click in the guideline list (#2883).
    #[test]
    fn shift_click_selects_the_guidelines_listed_between() {
        // The list shows guidelines 1, 3, 4, 6 (the others of another type).
        let shown = [1, 3, 4, 6];
        assert_eq!(shift_range(&shown, &[3], 6, false), Some(vec![4, 6, 3]));
        assert_eq!(shift_range(&shown, &[6], 1, false), Some(vec![1, 3, 4, 6]));
        // Ctrl keeps the others selected.
        assert_eq!(shift_range(&shown, &[0, 4], 6, true), Some(vec![0, 6, 4]));
        // No anchor in the list: nothing to range from.
        assert_eq!(shift_range(&shown, &[], 4, false), None);
        assert_eq!(shift_range(&shown, &[2], 4, false), None);
    }

    #[test]
    fn named_presets_add_their_lines() {
        let size = Size::new(210.0, 297.0);
        let margins = preset_lines(NamedPreset::OneInchMargins, size, 0.0);
        assert_eq!(margins.len(), 4);
        assert!(margins.contains(&GuideLine::Vertical { x: 210.0 - 25.4 }));
        let bleed = preset_lines(NamedPreset::BleedArea, size, 3.0);
        assert!(bleed.contains(&GuideLine::Vertical { x: -3.0 }));
        assert!(bleed.contains(&GuideLine::Horizontal { y: 300.0 }));
        let news = preset_lines(NamedPreset::ThreeColumnNewsletter, size, 0.0);
        // Four margins and four column edges between them.
        assert_eq!(news.len(), 8);
        let upper = preset_lines(NamedPreset::UpperLeftGrid, size, 0.0);
        assert!(upper.iter().all(|l| match l {
            GuideLine::Vertical { x } => *x <= 105.0 + 1e-9,
            GuideLine::Horizontal { y } => *y >= 148.5 - 1e-9,
            _ => false,
        }));
    }

    #[test]
    fn user_defined_columns_sit_between_the_margins() {
        let mut app = App::headless();
        let size = app.page_size();
        let form = PresetForm {
            user_defined: true,
            margins: true,
            margin_left: 10.0,
            margin_right: 10.0,
            columns: true,
            column_count: 2,
            column_gutter: 10.0,
            ..PresetForm::default()
        };
        apply_presets(&mut app, &form);
        let xs: Vec<f64> = app
            .page_guides()
            .iter()
            .filter_map(|g| match g.line {
                GuideLine::Vertical { x } => Some(x),
                _ => None,
            })
            .collect();
        let mid = size.width / 2.0;
        assert!(xs.iter().any(|x| (x - (mid - 5.0)).abs() < 1e-9), "{xs:?}");
        assert!(xs.iter().any(|x| (x - (mid + 5.0)).abs() < 1e-9), "{xs:?}");
        // Applying twice adds nothing new.
        let n = app.page_guides().len();
        apply_presets(&mut app, &form);
        assert_eq!(app.page_guides().len(), n);
    }

    /// User-defined bleed guidelines stand outside the page edges, and
    /// Mirror makes the followers take the value they follow (#3916).
    #[test]
    fn user_defined_bleed_guides_stand_outside_the_page_and_mirror() {
        let mut app = App::headless();
        let size = app.page_size();
        let form = PresetForm {
            user_defined: true,
            margins: true,
            margin_top: 10.0,
            margin_bottom: 99.0,
            margin_left: 20.0,
            margin_right: 99.0,
            margin_mirror: EdgeMirror::TopBottom,
            bleed: true,
            bleed_top: 3.5,
            bleed_bottom: 1.0,
            bleed_left: 2.0,
            bleed_right: 2.0,
            bleed_mirror: EdgeMirror::All,
            ..PresetForm::default()
        };
        apply_presets(&mut app, &form);
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        for g in app.page_guides() {
            match g.line {
                GuideLine::Vertical { x } => xs.push(x),
                GuideLine::Horizontal { y } => ys.push(y),
                _ => {}
            }
        }
        let has = |v: &[f64], want: f64| v.iter().any(|x| (x - want).abs() < 1e-9);
        // Margins: the bottom follows the top, the right is its own.
        assert!(has(&ys, size.height - 10.0) && has(&ys, 10.0), "{ys:?}");
        assert!(has(&xs, 20.0) && has(&xs, size.width - 99.0), "{xs:?}");
        // Bleed: 3.5 mm outside every edge.
        assert!(has(&ys, size.height + 3.5) && has(&ys, -3.5), "{ys:?}");
        assert!(has(&xs, -3.5) && has(&xs, size.width + 3.5), "{xs:?}");
        assert_eq!(xs.len() + ys.len(), 8);
        assert_eq!(
            EdgeMirror::LeftRight.apply([1.0, 2.0, 3.0, 4.0]),
            [1.0, 2.0, 3.0, 3.0]
        );
    }

    /// Margins applied to every page mirrored (#4118): the right-hand
    /// pages as typed (left the inside), the left-hand ones swapped;
    /// Odd Pages only reaches pages 1 and 3. Resizing a page moves its
    /// margin and bleed guidelines with its edges (#4268).
    #[test]
    fn presets_go_to_mirrored_or_odd_pages_and_follow_a_resize() {
        let mut app = App::headless();
        app.add_page();
        app.add_page();
        let pages: Vec<_> = app.doc().pages.iter().map(|p| p.id).collect();
        let size = app.page_size();
        let form = PresetForm {
            user_defined: true,
            margins: true,
            margin_top: 10.0,
            margin_bottom: 10.0,
            margin_left: 15.0,
            margin_right: 25.0,
            scope: PresetScope::AllMirrored,
            ..PresetForm::default()
        };
        apply_presets(&mut app, &form);
        let xs = |app: &App, k: usize| -> Vec<f64> {
            let mut v: Vec<f64> = app
                .doc()
                .page(pages[k])
                .unwrap()
                .guides
                .iter()
                .filter_map(|g| match g.line {
                    GuideLine::Vertical { x } => Some(x),
                    _ => None,
                })
                .collect();
            v.sort_by(f64::total_cmp);
            v
        };
        // Page 1 alone on the right, as typed; page 2 on the left, swapped.
        assert_eq!(xs(&app, 0), vec![15.0, size.width - 25.0]);
        assert_eq!(xs(&app, 1), vec![25.0, size.width - 15.0]);
        assert_eq!(xs(&app, 2), vec![15.0, size.width - 25.0]);
        app.undo();
        assert!(app.doc().pages.iter().all(|p| p.guides.is_empty()));
        apply_presets(
            &mut app,
            &PresetForm {
                scope: PresetScope::OddPages,
                ..form.clone()
            },
        );
        assert_eq!(xs(&app, 1), Vec::<f64>::new());
        assert_eq!(xs(&app, 2).len(), 2);
        // Page 3 made wider: its right margin moves with the edge.
        let wide = tracedraw_core::geometry::Size::new(size.width + 50.0, size.height);
        app.run(Command::ResizePage {
            page: pages[2],
            size: wide,
        });
        assert_eq!(xs(&app, 2), vec![15.0, wide.width - 25.0]);
        let ys: Vec<f64> = app.doc().page(pages[2]).unwrap().guides.iter().filter_map(|g| match g.line {
            GuideLine::Horizontal { y } => Some(y),
            _ => None,
        }).collect();
        assert!(ys.contains(&(size.height - 10.0)) && ys.contains(&10.0), "{ys:?}");
    }
}
