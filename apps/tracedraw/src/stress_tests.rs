//! Stress measurements (ignored by default): large generated drawings,
//! timed through the app's own paths. Run with
//! `cargo test -p tracedraw --release stress_ -- --ignored --nocapture`.

use crate::app::{App, DockerTab};
use egui::Pos2;
use std::time::Instant;
use tracedraw_core::document::{Shape, ShapeKind};
use tracedraw_core::geometry::{Affine, Rect, Size, Vec2};
use tracedraw_core::{Color, Fill, Stroke};

fn frame(ctx: &egui::Context, app: &mut App) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(1600.0, 1000.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
    out.textures_delta.clear();
}

fn t<R>(label: &str, f: impl FnOnce() -> R) -> R {
    let s = Instant::now();
    let r = f();
    eprintln!(
        "STRESS {label:<40} {:>9.1} ms",
        s.elapsed().as_secs_f64() * 1e3
    );
    r
}

fn rect_shape(ids: &mut tracedraw_core::id::IdSource, i: usize) -> Shape {
    let x = (i % 200) as f64 * 1.0;
    let y = (i / 200 % 280) as f64 * 1.0;
    let mut s = Shape::new(
        ids.shape(),
        ShapeKind::Rect {
            rect: Rect::new(0.0, 0.0, 0.8, 0.8),
            radius: 0.0,
            corners: None,
        },
    );
    s.transform = Affine::translate((x, y));
    s.fill = Fill::Solid(Color::Rgb {
        r: (i % 256) as f32 / 255.0,
        g: 0.3,
        b: 0.6,
    });
    s.stroke = Some(Stroke::new(Color::BLACK, 0.1));
    s
}

/// A drawing of `pages` pages, `per_page` rectangles on each.
fn doc_with(pages: usize, per_page: usize, nest: usize) -> tracedraw_core::Document {
    let mut doc = App::localized_document("stress", Size::new(210.0, 297.0));
    let mut ids = doc.ids().clone();
    let template = doc.pages[0].clone();
    for p in 0..pages {
        if p > 0 {
            let mut page = template.clone();
            page.id = ids.page();
            for l in &mut page.layers {
                l.id = ids.layer();
            }
            doc.pages.push(page);
        }
        let mut shapes: Vec<Shape> = (0..per_page).map(|i| rect_shape(&mut ids, i)).collect();
        for _ in 0..nest {
            let g = Shape::new(ids.shape(), ShapeKind::Group { children: shapes });
            shapes = vec![g];
        }
        doc.pages[p].layers[0].shapes = shapes;
    }
    doc.set_ids(ids);
    doc
}

fn open(doc: &tracedraw_core::Document, name: &str) -> (egui::Context, App, std::path::PathBuf) {
    let dir = std::env::temp_dir().join("td-stress");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}.tdraw"));
    t(&format!("{name}: save_native"), || {
        tracedraw_io::save_native(doc, &path).unwrap()
    });
    eprintln!(
        "STRESS {name}: file {} KB",
        std::fs::metadata(&path)
            .map(|m| m.len() / 1024)
            .unwrap_or(0)
    );
    let ctx = crate::theme::ui_context();
    let mut app = App::headless();
    app.show_welcome = false;
    frame(&ctx, &mut app);
    t(&format!("{name}: open_path + frames"), || {
        app.open_path(path.clone());
        for _ in 0..2000 {
            frame(&ctx, &mut app);
            if app.opening.is_none() && app.has_document() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });
    (ctx, app, path)
}

fn ops(name: &str, ctx: &egui::Context, app: &mut App) {
    t(&format!("{name}: zoom_to_page + frame"), || {
        app.zoom_to_page();
        frame(ctx, app);
    });
    t(&format!("{name}: idle frame"), || frame(ctx, app));
    t(&format!("{name}: open Objects docker + frame"), || {
        app.open_docker(DockerTab::Objects);
        frame(ctx, app);
    });
    t(&format!("{name}: idle frame (docker)"), || frame(ctx, app));
    t(&format!("{name}: select_all"), || app.select_all());
    t(&format!("{name}: frame after select_all"), || {
        frame(ctx, app)
    });
    t(&format!("{name}: nudge"), || app.nudge(Vec2::new(1.0, 0.0)));
    t(&format!("{name}: frame after nudge"), || frame(ctx, app));
    t(&format!("{name}: undo"), || app.undo());
    t(&format!("{name}: frame after undo"), || frame(ctx, app));
    t(&format!("{name}: redo"), || app.redo());
    t(&format!("{name}: frame after redo"), || frame(ctx, app));
    t(&format!("{name}: 4x zoom in + frames"), || {
        for _ in 0..4 {
            app.view.zoom_at(Pos2::new(800.0, 500.0), 2.0);
            frame(ctx, app);
        }
    });
    t(&format!("{name}: 10 pans + frames"), || {
        for _ in 0..10 {
            app.view.origin += egui::vec2(30.0, 20.0);
            frame(ctx, app);
        }
    });
    t(&format!("{name}: 50 nudges"), || {
        for _ in 0..50 {
            app.nudge(Vec2::new(0.1, 0.0));
        }
    });
    t(&format!("{name}: 50 undos"), || {
        for _ in 0..50 {
            app.undo();
        }
    });
    let doc = app.engine.document().clone();
    let dir = std::env::temp_dir().join("td-stress");
    t(&format!("{name}: save_pdf"), || {
        tracedraw_io::save_pdf(&doc, dir.join(format!("{name}.pdf"))).unwrap()
    });
    t(&format!("{name}: save_svg p1"), || {
        tracedraw_io::save_svg(&doc, 0, dir.join(format!("{name}.svg"))).unwrap()
    });
    t(&format!("{name}: delete_selection"), || {
        app.delete_selection()
    });
    t(&format!("{name}: frame after delete"), || frame(ctx, app));
}

#[test]
#[ignore]
fn stress_flat_objects() {
    for n in [5_000usize, 10_000, 20_000, 50_000] {
        let name = format!("flat{n}");
        let doc = doc_with(1, n, 0);
        let (ctx, mut app, _) = open(&doc, &name);
        ops(&name, &ctx, &mut app);
    }
}

#[test]
#[ignore]
fn stress_many_pages() {
    for pages in [50usize, 100, 200] {
        let name = format!("pages{pages}");
        let doc = doc_with(pages, 100, 0);
        let (ctx, mut app, _) = open(&doc, &name);
        ops(&name, &ctx, &mut app);
        t(&format!("{name}: go to last page + frame"), || {
            app.goto_page(pages - 1);
            frame(&ctx, &mut app);
        });
    }
}

#[test]
#[ignore]
fn stress_deep_groups() {
    for nest in [50usize, 200, 330, 1000] {
        let name = format!("deep{nest}");
        let doc = doc_with(1, 200, nest);
        let (ctx, mut app, _) = open(&doc, &name);
        ops(&name, &ctx, &mut app);
    }
}

/// One slow operation repeated, for sampling with a debugger:
/// `STRESS_OP=nudge|frame|undo|pan` (`STRESS_NOSEL` leaves nothing
/// selected).
#[test]
#[ignore]
fn stress_profile_one() {
    let n: usize = std::env::var("STRESS_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    let op = std::env::var("STRESS_OP").unwrap_or_else(|_| "nudge".into());
    let doc = doc_with(1, n, 0);
    let (ctx, mut app, _) = open(&doc, "profile");
    app.zoom_to_page();
    if std::env::var_os("STRESS_DOCKER").is_some() {
        app.open_docker(DockerTab::Objects);
    }
    if std::env::var_os("STRESS_NOSEL").is_none() {
        app.select_all();
    }
    frame(&ctx, &mut app);
    eprintln!("STRESS ready");
    for i in 0..300 {
        t(&format!("{op} {i}"), || match op.as_str() {
            "nudge" => app.nudge(Vec2::new(0.1, 0.0)),
            "undo" => {
                app.nudge(Vec2::new(0.1, 0.0));
                app.undo()
            }
            "pan" => {
                app.view.origin += egui::vec2(3.0, 2.0);
                frame(&ctx, &mut app)
            }
            _ => frame(&ctx, &mut app),
        });
    }
}

/// Opens (`Document::from_json`) a drawing of `STRESS_NEST` nested groups
/// on a thread of `STRESS_STACK` KB, then renders and drops it, for
/// finding the depth that overflows a stack.
#[test]
#[ignore]
fn stress_nest_on_stack() {
    let nest: usize = std::env::var("STRESS_NEST")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let kb: usize = std::env::var("STRESS_STACK")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024);
    let json = doc_with(1, 3, nest).to_json().unwrap();
    std::thread::Builder::new()
        .stack_size(kb * 1024)
        .spawn(move || {
            let doc = tracedraw_core::Document::from_json(&json).expect("reads");
            eprintln!("STRESS read nest {nest}");
            let pic = tracedraw_io::whole_page_picture(&doc, 256);
            eprintln!("STRESS rendered {}", pic.is_some());
            let copy = doc.clone();
            drop(copy);
            eprintln!("STRESS dropped");
        })
        .unwrap()
        .join()
        .unwrap();
}
