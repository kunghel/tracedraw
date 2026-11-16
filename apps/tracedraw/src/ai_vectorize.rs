//! Vectorize with AI (#2713): a bitmap already in the drawing sent to a
//! vectorization service the user sets up with their own key (Tools >
//! Options > AI > Vectorization), its SVG answer placed over the bitmap
//! as a group of editable curves, in one undo step. TraceDraw has no AI
//! of its own; nothing is sent until the user asks.
//!
//! The request is the plain form upload such services take: a
//! `multipart/form-data` POST with the picture as the `image` field (PNG),
//! plus `mode=test` when the user asks for the service's free test
//! results. The answer is read as SVG. The key goes as HTTP basic
//! authentication when an API id is given with it (`id:secret`), else as
//! a bearer token. See docs/behavior/ai-vectorize.md.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::i18n::{tr, trf};
use crate::net::{HttpRequest, HttpResult};
use tracedraw_core::document::{Shape, ShapeKind};
use tracedraw_core::{Affine, Command, Rect, ShapeId, Vec2};

/// The address requests go to when none is set.
pub const DEFAULT_ENDPOINT: &str = "https://vectorizer.ai/api/v1/vectorize";

/// The most bytes of answer read.
const MAX_REPLY: u64 = 64 * 1024 * 1024;

/// The undo step's name.
const LABEL: &str = "Vectorize with AI";

/// The multipart boundary of the upload.
const BOUNDARY: &str = "tracedraw-ai-vectorize-boundary-3e81d2";

/// Options > AI > Vectorization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VectorizePrefs {
    /// Empty: [`DEFAULT_ENDPOINT`].
    pub endpoint: String,
    /// The API id that goes with the key (basic authentication); empty:
    /// the key alone, as a bearer token.
    pub api_id: String,
    /// The key, saved only when `remember_key`.
    pub key: String,
    pub remember_key: bool,
    /// Ask for the service's free test results (`mode=test`).
    pub test_mode: bool,
    /// The curves replace the bitmap instead of going over it.
    pub replace: bool,
}

impl VectorizePrefs {
    pub fn endpoint(&self) -> String {
        match self.endpoint.trim() {
            "" => DEFAULT_ENDPOINT.to_string(),
            e => e.to_string(),
        }
    }
}

/// Where Vectorize with AI stands.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum VectorizeStatus {
    #[default]
    Idle,
    Waiting,
    Done,
    Failed(String),
}

/// The session's state: the key typed (kept on disk only when asked),
/// the request on its way and the bitmap it is for.
#[derive(Default)]
pub struct VectorizeSession {
    pub key: String,
    pub status: VectorizeStatus,
    pending: Option<(crate::ai_retry::Call, ShapeId)>,
    /// Tests: answers requests at once instead of the network.
    pub fake: Option<fn(&HttpRequest) -> HttpResult>,
    /// Tests: the last request sent.
    pub sent: Option<HttpRequest>,
}

impl VectorizeSession {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

/// The upload of `png` to the service of `prefs`, with `key`.
pub fn build_request(prefs: &VectorizePrefs, key: &str, png: &[u8]) -> HttpRequest {
    let mut body = Vec::new();
    if prefs.test_mode {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"mode\"\r\n\r\ntest\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"image.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    let mut headers = vec![
        (
            "content-type".to_string(),
            format!("multipart/form-data; boundary={BOUNDARY}"),
        ),
        (
            "accept".to_string(),
            "image/svg+xml, application/json".to_string(),
        ),
    ];
    let key = key.trim();
    let id = prefs.api_id.trim();
    if !id.is_empty() {
        let pair = base64::engine::general_purpose::STANDARD.encode(format!("{id}:{key}"));
        headers.push(("authorization".to_string(), format!("Basic {pair}")));
    } else if !key.is_empty() {
        headers.push(("authorization".to_string(), format!("Bearer {key}")));
    }
    HttpRequest {
        method: "POST",
        url: prefs.endpoint(),
        headers,
        body: String::new(),
        bytes: body,
    }
}

/// The SVG of the service's answer, or why there is none.
pub fn parse_reply(result: HttpResult) -> Result<String, String> {
    let r = result.map_err(crate::ai::network_error)?;
    if !(200..300).contains(&r.status) {
        let v = serde_json::from_str::<serde_json::Value>(&r.body).ok();
        return Err(crate::ai::http_error(&r, v.as_ref()));
    }
    crate::ai_image::extract_svg(&r.body).ok_or_else(|| tr("aivec.no_svg"))
}

/// The transform that puts a drawing of `size` (its canvas from the
/// origin) onto `rect`; a drawing with no size is fitted by `bounds`.
fn fit(size: tracedraw_core::Size, bounds: Rect, rect: Rect) -> Option<Affine> {
    let from = if size.width > 1e-9 && size.height > 1e-9 {
        Rect::new(0.0, 0.0, size.width, size.height)
    } else if bounds.width() > 1e-9 && bounds.height() > 1e-9 {
        bounds
    } else {
        return None;
    };
    let (sx, sy) = (rect.width() / from.width(), rect.height() / from.height());
    Some(
        Affine::translate(Vec2::new(rect.x0, rect.y0))
            * Affine::scale_non_uniform(sx, sy)
            * Affine::translate(Vec2::new(-from.x0, -from.y0)),
    )
}

impl App {
    /// The key is typed and AI features are on.
    pub fn ai_vectorize_ready(&self) -> bool {
        self.ai_features_on() && !self.ai_vectorize.key.trim().is_empty()
    }

    /// Why Vectorize with AI is greyed, for its hover tip; none when it
    /// can run.
    pub fn ai_vectorize_tip(&self) -> Option<String> {
        if !self.ai_features_on() {
            Some(tr("aiimg.off"))
        } else if !self.ai_vectorize_ready() {
            Some(tr("aivec.set_up_tip"))
        } else {
            None
        }
    }

    /// The command can run on the selection now.
    pub fn ai_vectorize_enabled(&self) -> bool {
        self.ai_vectorize_ready() && self.ai_source_bitmap().is_some() && !self.ai_vectorize.busy()
    }

    /// Image > Vectorize with AI: send the selected bitmap to the
    /// vectorization service.
    pub fn ai_vectorize(&mut self) {
        if self.ai_vectorize.busy() {
            return;
        }
        let problem = if !self.ai_features_on() {
            Some(tr("aiimg.off"))
        } else if !self.ai_vectorize_ready() {
            Some(tr("aivec.not_set_up"))
        } else {
            None
        };
        if let Some(p) = problem {
            self.status = p.clone();
            self.ai_vectorize.status = VectorizeStatus::Failed(p);
            return;
        }
        let Some((shape, png)) = self.ai_source_bitmap() else {
            self.status = tr("aiimg.need_bitmap");
            return;
        };
        let http = build_request(&self.settings.ai_vectorize, &self.ai_vectorize.key, &png);
        self.ai_vectorize.sent = Some(http.clone());
        let call = crate::ai_retry::Call::start(http, MAX_REPLY, self.ai_vectorize.fake);
        self.ai_vectorize.pending = Some((call, shape));
        self.ai_vectorize.status = VectorizeStatus::Waiting;
        self.status = tr("aivec.sending");
        self.poll_ai_vectorize();
    }

    /// Look for the service's answer; true while it is still awaited.
    pub fn poll_ai_vectorize(&mut self) -> bool {
        use crate::ai_retry::Polled;
        let fake = self.ai_vectorize.fake;
        let Some((call, _)) = self.ai_vectorize.pending.as_mut() else {
            return false;
        };
        let mut again = Vec::new();
        let result = match call.poll(fake, std::time::Duration::ZERO, &mut again) {
            Polled::Pending => return true,
            Polled::Lost => Err(tr("ai.connection_lost")),
            Polled::Done(Err(e), _) => Err(crate::ai::network_error(e)),
            Polled::Done(Ok(r), tries) => match crate::ai_retry::gave_up(&r, tries) {
                Some(e) => Err(e),
                None => Ok(Ok(r)),
            },
        };
        let Some((_, shape)) = self.ai_vectorize.pending.take() else {
            return false;
        };
        let placed = result
            .and_then(parse_reply)
            .and_then(|svg| self.ai_place_vectorized(shape, &svg));
        match placed {
            Ok(()) => {
                self.ai_vectorize.status = VectorizeStatus::Done;
                self.status = tr("aivec.done");
            }
            Err(e) => {
                self.status = e.clone();
                self.ai_vectorize.status = VectorizeStatus::Failed(e);
            }
        }
        false
    }

    /// Put the curves of `svg` over bitmap `shape` (its place, size and
    /// turn), grouped, in one undo step; with Replace the bitmap goes in
    /// the same step.
    fn ai_place_vectorized(&mut self, shape: ShapeId, svg: &str) -> Result<(), String> {
        use tracedraw_core::geometry::Shape as _;
        let src = self
            .doc()
            .find_shape(shape)
            .cloned()
            .ok_or_else(|| tr("aiimg.source_gone"))?;
        let ShapeKind::Bitmap { rect, .. } = &src.kind else {
            return Err(tr("aiimg.source_gone"));
        };
        let layer = self
            .doc()
            .all_layers()
            .find(|l| l.shapes.iter().any(|s| s.id == shape))
            .map(|l| l.id)
            .or_else(|| self.active_layer())
            .ok_or_else(|| tr("aiimg.source_gone"))?;
        let parsed =
            tracedraw_io::svg_import::parse(svg, &mut tracedraw_core::id::IdSource::default())
                .map_err(|e| trf("aiimg.bad_svg", &[("e", &e)]))?;
        if parsed.shapes.is_empty() {
            return Err(tr("aivec.no_svg"));
        }
        let gid = self.engine.new_shape_id();
        let children: Vec<Shape> = parsed
            .shapes
            .into_iter()
            .map(|s| {
                let id = self.engine.new_shape_id();
                crate::app::reid_pub(s, id, &mut self.engine)
            })
            .collect();
        let mut group = Shape::new(gid, ShapeKind::Group { children });
        group.name = Some(tr("aivec.object_name"));
        let bounds = group.page_path().bounding_box();
        let t = fit(parsed.size, bounds, *rect).ok_or_else(|| tr("aivec.no_svg"))?;
        group.transform = src.transform * t;
        let mut cmds = vec![Command::AddShape {
            layer,
            shape: group,
        }];
        if self.settings.ai_vectorize.replace {
            cmds.push(Command::DeleteShapes {
                shapes: vec![shape],
            });
        }
        self.engine
            .run_batch(LABEL, &cmds)
            .map_err(|e| e.to_string())?;
        self.select(vec![gid]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::HttpResponse;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><path d="M0 0H20V20H0Z" fill="#ff0000"/><path d="M20 0H40V20H20Z" fill="#0000ff"/></svg>"##;

    fn svg_reply(_: &HttpRequest) -> HttpResult {
        Ok(HttpResponse {
            status: 200,
            body: SVG.to_string(),
            retry_after: None,
        })
    }

    fn refused(_: &HttpRequest) -> HttpResult {
        Ok(HttpResponse {
            status: 401,
            body: r#"{"error":{"message":"Invalid credentials"}}"#.to_string(),
            retry_after: None,
        })
    }

    /// A drawing with one 40 x 20 mm bitmap at (10, 30), selected.
    fn app_with_bitmap() -> (App, ShapeId) {
        crate::i18n::set_language("en");
        let mut app = App::headless();
        app.settings.ai_enabled = true;
        app.ai_vectorize.key = "secret".into();
        let img = image::RgbaImage::from_pixel(8, 4, image::Rgba([255, 0, 0, 255]));
        let png = crate::bitmap_fx::encode(&img).expect("png");
        let layer = app.active_layer().expect("layer");
        let id = app.engine.new_shape_id();
        let mut shape = Shape::new(
            id,
            ShapeKind::Bitmap {
                rect: Rect::new(10.0, 30.0, 50.0, 50.0),
                width_px: 8,
                height_px: 4,
                png: png.as_slice().into(),
                fx: None,
            },
        );
        shape.fill = tracedraw_core::Fill::None;
        shape.stroke = None;
        app.run(Command::AddShape { layer, shape });
        app.select(vec![id]);
        (app, id)
    }

    fn header<'a>(r: &'a HttpRequest, name: &str) -> Option<&'a str> {
        r.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn the_upload_carries_the_picture_and_the_key() {
        let png = b"\x89PNG fake".to_vec();
        let mut prefs = VectorizePrefs::default();
        let r = build_request(&prefs, " k1 ", &png);
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, DEFAULT_ENDPOINT);
        assert_eq!(header(&r, "authorization"), Some("Bearer k1"));
        assert!(header(&r, "content-type")
            .is_some_and(|v| v.starts_with("multipart/form-data; boundary=")));
        let body = String::from_utf8_lossy(&r.bytes).to_string();
        assert!(body.contains("name=\"image\"; filename=\"image.png\""));
        assert!(body.contains("\u{FFFD}PNG fake") || r.bytes.windows(9).any(|w| w == &png[..9]));
        assert!(!body.contains("name=\"mode\""));
        // An API id: basic authentication, id:key.
        prefs.api_id = "abc".into();
        prefs.test_mode = true;
        prefs.endpoint = "http://localhost:9000/vectorize".into();
        let r = build_request(&prefs, "k1", &png);
        assert_eq!(r.url, "http://localhost:9000/vectorize");
        let pair = base64::engine::general_purpose::STANDARD.encode("abc:k1");
        assert_eq!(
            header(&r, "authorization"),
            Some(format!("Basic {pair}").as_str())
        );
        assert!(String::from_utf8_lossy(&r.bytes).contains("name=\"mode\"\r\n\r\ntest"));
    }

    #[test]
    fn the_curves_go_over_the_bitmap_grouped_in_one_undo_step() {
        use tracedraw_core::geometry::Shape as _;
        let (mut app, bitmap) = app_with_bitmap();
        app.ai_vectorize.fake = Some(svg_reply);
        let before = app.engine.history_labels().0.len();
        app.ai_vectorize();
        assert!(!app.ai_vectorize.busy());
        assert_eq!(app.ai_vectorize.status, VectorizeStatus::Done);
        assert_eq!(
            app.engine.history_labels().0.len(),
            before + 1,
            "one undo step"
        );
        // The bitmap stays; a group of two curves is selected over it,
        // on the bitmap's own place.
        assert!(app.doc().find_shape(bitmap).is_some());
        let sel = app.selected_shapes();
        let [group] = sel.as_slice() else {
            panic!("one group selected");
        };
        let ShapeKind::Group { children } = &group.kind else {
            panic!("a group");
        };
        assert_eq!(children.len(), 2);
        assert_eq!(group.name.as_deref(), Some("AI vectors"));
        let b = group.page_path().bounding_box();
        assert!(
            (b.x0 - 10.0).abs() < 0.01 && (b.x1 - 50.0).abs() < 0.01,
            "{b:?}"
        );
        assert!(
            (b.y0 - 30.0).abs() < 0.01 && (b.y1 - 50.0).abs() < 0.01,
            "{b:?}"
        );
        // Undo takes it all back.
        app.undo();
        assert!(app.selected_shapes().is_empty() || app.doc().find_shape(group.id).is_none());
        assert!(app.doc().find_shape(group.id).is_none());
    }

    #[test]
    fn replace_removes_the_bitmap_in_the_same_step() {
        let (mut app, bitmap) = app_with_bitmap();
        app.settings.ai_vectorize.replace = true;
        app.ai_vectorize.fake = Some(svg_reply);
        let before = app.engine.history_labels().0.len();
        app.ai_vectorize();
        assert!(app.doc().find_shape(bitmap).is_none());
        assert_eq!(app.engine.history_labels().0.len(), before + 1);
        app.undo();
        assert!(app.doc().find_shape(bitmap).is_some());
    }

    #[test]
    fn nothing_is_sent_while_ai_is_off_not_set_up_or_no_bitmap() {
        let (mut app, _) = app_with_bitmap();
        app.ai_vectorize.fake = Some(svg_reply);
        app.settings.ai_enabled = false;
        assert!(!app.ai_vectorize_enabled());
        app.ai_vectorize();
        assert!(app.ai_vectorize.sent.is_none());
        app.settings.ai_enabled = true;
        app.ai_vectorize.key.clear();
        assert!(app.ai_vectorize_tip().is_some());
        app.ai_vectorize();
        assert!(app.ai_vectorize.sent.is_none());
        assert!(matches!(
            app.ai_vectorize.status,
            VectorizeStatus::Failed(_)
        ));
        app.ai_vectorize.key = "k".into();
        app.select(Vec::new());
        app.ai_vectorize();
        assert!(app.ai_vectorize.sent.is_none());
    }

    #[test]
    fn a_refusal_is_said_and_nothing_is_placed() {
        let (mut app, _) = app_with_bitmap();
        app.ai_vectorize.fake = Some(refused);
        let before = app.engine.history_labels().0.len();
        app.ai_vectorize();
        assert_eq!(app.engine.history_labels().0.len(), before);
        let VectorizeStatus::Failed(e) = &app.ai_vectorize.status else {
            panic!("failed");
        };
        assert!(e.contains("401"), "{e}");
        // An answer that is no drawing.
        assert_eq!(
            parse_reply(Ok(HttpResponse {
                status: 200,
                body: "{}".into(),
                retry_after: None
            })),
            Err(tr("aivec.no_svg"))
        );
    }
}
