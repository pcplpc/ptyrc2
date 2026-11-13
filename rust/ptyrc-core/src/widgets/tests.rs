// SPDX-License-Identifier: GPL-3.0-or-later

use super::rooms::rooms;
use super::*;
use crate::frames::HeatView;
use crate::scene::{Run, StyleSpec};
use crate::types::{CursorSample, FrameKind, FramePos, FrameState, TextMatch, TextQuery};
use ptyrc_vt::{Cell, Style};
use std::collections::HashMap;

const PANE: FrameId = 3;

/// Frames, and when each screen cell last changed.
struct Mock {
    infos: Vec<SurfaceInfo>,
    changed: HashMap<(u16, u16), Duration>,
    panel: Vec<Rect>,
}

fn surface(id: FrameId, rect: Rect) -> SurfaceInfo {
    SurfaceInfo {
        id,
        kind: if id == RAW {
            FrameKind::Raw
        } else {
            FrameKind::Pane
        },
        state: FrameState::Stable,
        rect,
        origin: FramePos::default(),
        usable: true,
    }
}

impl Surfaces for Mock {
    fn info(&self, id: FrameId) -> Option<SurfaceInfo> {
        self.infos.iter().find(|i| i.id == id).copied()
    }

    fn resolve(&self, sel: &FrameSel) -> Option<FrameId> {
        match sel {
            FrameSel::Id(id) => self.info(*id).map(|i| i.id),
            FrameSel::Name(n) if n == "raw" => Some(RAW),
            FrameSel::Name(_) => Some(PANE).filter(|id| self.info(*id).is_some()),
        }
    }

    fn find(&self, _id: FrameId, _query: &TextQuery, _known: bool) -> Vec<TextMatch> {
        Vec::new()
    }

    fn place(&self, _: FrameId, _: u16, _: u16, _: Prefer, _: &[Rect]) -> Option<(u16, u16)> {
        None
    }

    fn heat(&self, id: FrameId, now: Duration) -> Option<HeatView> {
        let r = self.info(id)?.rect;
        let n = r.w as usize * r.h as usize;
        let mut view = HeatView {
            cols: r.w,
            rows: r.h,
            safe: vec![1.0; n],
            activity: vec![0.0; n],
            occupancy: vec![0.0; n],
            still: Vec::with_capacity(n),
            class: Vec::with_capacity(n),
        };
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                let at = self.changed.get(&(x, y)).copied().unwrap_or_default();
                let still = now.saturating_sub(at).as_secs_f32();
                view.still.push(still);
                view.class.push(if still < 10.0 && at > Duration::ZERO {
                    HeatClass::Hot
                } else if self.panel.iter().any(|p| p.contains(x, y)) {
                    HeatClass::Panel
                } else {
                    HeatClass::Free
                });
            }
        }
        Some(view)
    }

    fn cursor(&self) -> (u16, u16) {
        (0, 0)
    }

    fn cursor_trail(&self) -> Vec<CursorSample> {
        Vec::new()
    }
}

struct Rig {
    w: Widgets,
    scene: Scene,
    grid: Grid,
    mock: Mock,
    now: Duration,
    cursor: Option<(u16, u16)>,
    /// Every info `tick` returned.
    events: Vec<WidgetInfo>,
}

impl Rig {
    fn new(cols: u16, rows: u16) -> Rig {
        Rig {
            w: Widgets::new(WidgetConfig::default()),
            scene: Scene::new(),
            grid: Grid::new(cols, rows),
            mock: Mock {
                infos: vec![surface(RAW, Rect::new(0, 0, cols, rows))],
                changed: HashMap::new(),
                panel: Vec::new(),
            },
            now: Duration::from_secs(20),
            cursor: None,
            events: Vec::new(),
        }
    }

    fn tick(&mut self) -> Vec<WidgetInfo> {
        let out = self.w.tick(
            self.now,
            &self.grid,
            self.cursor,
            &self.mock,
            &mut self.scene,
        );
        self.scene.tick(self.now, &self.mock);
        self.events.extend(out.iter().cloned());
        out
    }

    /// Let `ms` pass, ticking at every deadline.
    fn run(&mut self, ms: u64) {
        let end = self.now + Duration::from_millis(ms);
        let mut n = 0;
        while let Some(wake) = self.w.next_wake() {
            if wake > end {
                break;
            }
            self.now = wake.max(self.now);
            self.tick();
            n += 1;
            assert!(n < 1000, "the loop spins");
        }
        self.now = end;
    }

    /// The app writes `text` at a screen cell.
    fn write(&mut self, x: u16, y: u16, text: &str) {
        self.grid.put_ascii(x, y, text.as_bytes(), Style::default());
        for i in 0..text.len() as u16 {
            self.mock.changed.insert((x + i, y), self.now);
        }
        self.w.touch();
        self.tick();
    }

    /// The app clears a row.
    fn clear(&mut self, y: u16) {
        self.grid.clear_row(y, &Cell::blank(Style::default()));
        for x in 0..self.grid.cols() {
            self.mock.changed.insert((x, y), self.now);
        }
        self.w.touch();
        self.tick();
    }

    fn declare(&mut self, owner: OwnerId, tag: &str, sizes: &[[u16; 2]]) {
        let decl = Declare {
            widgets: sizes
                .iter()
                .map(|s| VariantDecl {
                    tag: tag.into(),
                    size: *s,
                    ..VariantDecl::default()
                })
                .collect(),
            border: None,
        };
        self.w.declare(owner, decl).unwrap();
    }

    fn show(&mut self, owner: OwnerId, show: Show) -> WidgetId {
        let id = self.w.show(owner, show).unwrap();
        self.tick();
        id
    }

    fn info(&self, id: WidgetId) -> WidgetInfo {
        self.w.info(id).unwrap()
    }

    fn rect(&self, id: WidgetId) -> Option<[u16; 4]> {
        self.info(id).rect
    }

    /// The composed screen as text.
    fn shown(&self) -> Vec<String> {
        self.scene.compose(&self.grid, &self.mock).text()
    }
}

fn text_of(content: &Content) -> Vec<String> {
    content
        .cells
        .as_ref()
        .unwrap()
        .iter()
        .map(|runs| runs.iter().map(|r| r.t.as_str()).collect())
        .collect()
}

fn overlaps(a: [u16; 4], b: [u16; 4]) -> bool {
    Rect::new(a[0], a[1], a[2], a[3]).intersects(&Rect::new(b[0], b[1], b[2], b[3]))
}

// Rooms.

fn brute_rooms(open: &[bool], w: usize, h: usize) -> Vec<Rect> {
    let all = |x0: usize, y0: usize, x1: usize, y1: usize| {
        (y0..y1).all(|y| (x0..x1).all(|x| open[y * w + x]))
    };
    let mut out = Vec::new();
    for y0 in 0..h {
        for x0 in 0..w {
            for y1 in y0 + 1..=h {
                for x1 in x0 + 1..=w {
                    if !all(x0, y0, x1, y1) {
                        continue;
                    }
                    let grows = (x0 > 0 && all(x0 - 1, y0, x1, y1))
                        || (x1 < w && all(x0, y0, x1 + 1, y1))
                        || (y0 > 0 && all(x0, y0 - 1, x1, y1))
                        || (y1 < h && all(x0, y0, x1, y1 + 1));
                    if !grows {
                        out.push(Rect::new(
                            x0 as u16,
                            y0 as u16,
                            (x1 - x0) as u16,
                            (y1 - y0) as u16,
                        ));
                    }
                }
            }
        }
    }
    out
}

fn sorted(mut v: Vec<Rect>) -> Vec<(u16, u16, u16, u16)> {
    let mut out: Vec<_> = v.drain(..).map(|r| (r.x, r.y, r.w, r.h)).collect();
    out.sort();
    out
}

#[test]
fn rooms_are_the_maximal_rectangles() {
    // An L of text: two rooms.
    let rows = ["#.....", "#.....", "......"];
    let open: Vec<bool> = rows
        .iter()
        .flat_map(|r| r.chars().map(|c| c == '.'))
        .collect();
    assert_eq!(sorted(rooms(&open, 6, 3)), vec![(0, 2, 6, 1), (1, 0, 5, 3)]);
    // Random masks against brute force, each room once.
    let mut seed = 7u64;
    for _ in 0..300 {
        let (w, h) = (1 + (seed % 7) as usize, 1 + (seed / 7 % 6) as usize);
        let open: Vec<bool> = (0..w * h)
            .map(|_| {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                !(seed >> 33).is_multiple_of(4)
            })
            .collect();
        let found = sorted(rooms(&open, w, h));
        let mut unique = found.clone();
        unique.dedup();
        assert_eq!(found, unique, "a room twice");
        assert_eq!(found, sorted(brute_rooms(&open, w, h)), "{w}x{h} {open:?}");
    }
    assert!(rooms(&[false; 12], 4, 3).is_empty());
    assert_eq!(sorted(rooms(&[true; 12], 4, 3)), vec![(0, 0, 4, 3)]);
}

#[test]
fn rooms_of_a_large_screen_are_cheap() {
    // 200x60, text on every other row: one pass, no blow-up.
    let (w, h) = (200, 60);
    let open: Vec<bool> = (0..w * h)
        .map(|i| (i / w) % 2 == 1 || i % w > 120)
        .collect();
    let started = std::time::Instant::now();
    let found = rooms(&open, w, h);
    assert!(started.elapsed() < Duration::from_millis(50));
    assert!(found.len() < 64);
}

// Borders.

#[test]
fn border_title_cut_and_absent() {
    let plain = BorderSpec::default();
    let c = compose(&plain, "clock", Some(&Content::rows(&["12:00"])), 9, 1);
    assert_eq!(text_of(&c), ["┌ clock ──┐", "│12:00    │", "└─────────┘"]);
    // Cut: room is 9 - 2 = 7 cells.
    let c = compose(&plain, "a long title", None, 9, 1);
    assert_eq!(text_of(&c)[0], "┌ a long… ┐");
    // Under 3 cells of room: no title.
    let c = compose(&plain, "clock", None, 4, 1);
    assert_eq!(text_of(&c)[0], "┌────┐");
    let c = compose(&plain, "clock", None, 5, 1);
    assert_eq!(text_of(&c)[0], "┌ cl… ┐");
    // Empty title: none.
    let c = compose(&plain, "", None, 6, 1);
    assert_eq!(text_of(&c)[0], "┌──────┐");
    // Aligned right and centred.
    let mut right = plain.clone();
    right.title.align = Align::Right;
    assert_eq!(
        text_of(&compose(&right, "ab", None, 10, 1))[0],
        "┌────── ab ┐"
    );
    right.title.align = Align::Center;
    assert_eq!(
        text_of(&compose(&right, "ab", None, 10, 1))[0],
        "┌─── ab ───┐"
    );
}

#[test]
fn border_patterns_style_and_none() {
    let mut b = BorderSpec::lines("╔╗╚╝", "═╤", "║╟");
    b.bottom = "═╧".into();
    b.right = "║╢".into();
    let c = compose(&b, "", None, 5, 3);
    assert_eq!(
        text_of(&c),
        ["╔═╤═╤═╗", "║     ║", "╟     ╢", "║     ║", "╚═╧═╧═╝"]
    );
    // The title keeps the phase of the pattern after it.
    let c = compose(&b, "x", None, 8, 1);
    assert_eq!(text_of(&c)[0], "╔ x ╤═╤═╤╗");
    // Dim border, opaque inside: every cell has a background.
    let c = compose(
        &BorderSpec::default(),
        "t",
        Some(&Content::rows(&["hi"])),
        4,
        1,
    );
    let cells = c.cells.as_ref().unwrap();
    assert!(cells[0][0].style.dim);
    assert!(c.opaque);
    assert!(cells.iter().flatten().all(|r| r.style.bg.is_some()));
    // `none`: the outer size is the inner one, no title.
    let c = compose(
        &BorderSpec::none(),
        "clock",
        Some(&Content::rows(&["abcdef", "x"])),
        4,
        2,
    );
    assert_eq!(text_of(&c), ["abcd", "x   "]);
}

#[test]
fn content_clipped_padded_and_styled() {
    let content = Content {
        cells: Some(vec![
            vec![
                Run {
                    t: "ab".into(),
                    style: StyleSpec::fg(ptyrc_vt::Color::Indexed(1)),
                },
                Run {
                    t: "c日".into(),
                    style: StyleSpec::default(),
                },
            ],
            vec![],
            vec![Run {
                t: "beyond".into(),
                style: StyleSpec::default(),
            }],
        ]),
        ..Content::default()
    };
    // A wide glyph that does not fit is a blank; rows past the size go.
    let c = compose(&BorderSpec::none(), "", Some(&content), 4, 2);
    assert_eq!(text_of(&c), ["abc ", "    "]);
    let row = &c.cells.as_ref().unwrap()[0];
    assert_eq!(row[0].t, "ab");
    assert_eq!(
        row[0].style.fg,
        Some(crate::scene::ColorSpec(ptyrc_vt::Color::Indexed(1)))
    );
    let c = compose(&BorderSpec::none(), "", Some(&content), 5, 1);
    assert_eq!(text_of(&c), ["abc日"]);
}

#[test]
fn presets_are_single_cell_glyphs() {
    let w = Widgets::new(WidgetConfig::default());
    assert_eq!(
        w.borders(),
        ["plain", "round", "heavy", "double", "ascii", "none"]
    );
    for (name, spec) in presets() {
        spec.check().unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let wide = BorderPatch {
        top: Some("日".into()),
        ..BorderPatch::default()
    };
    assert!(wide.apply(&BorderSpec::default()).check().is_err());
    let partial = BorderPatch {
        base: Some("none".into()),
        top: Some("-".into()),
        ..BorderPatch::default()
    };
    assert!(partial.apply(&BorderSpec::none()).check().is_err());
}

// Declarations.

#[test]
fn declare_checks_and_replaces() {
    let mut w = Widgets::new(WidgetConfig::default());
    let v = |tag: &str, size: [u16; 2], name: Option<&str>| VariantDecl {
        tag: tag.into(),
        name: name.map(String::from),
        size,
        content: None,
    };
    let decl = |widgets| Declare {
        widgets,
        border: None,
    };
    let err = |r: Result<Vec<String>, WidgetError>| match r {
        Err(WidgetError::BadParams(m)) => m,
        other => panic!("{other:?}"),
    };
    assert!(err(w.declare(1, decl(vec![v("a", [0, 1], None)]))).contains("from 1x1 to 200x60"));
    assert!(err(w.declare(1, decl(vec![v("a", [201, 1], None)]))).contains("200x60"));
    assert!(
        err(w.declare(1, decl(vec![v("a", [3, 1], None), v("a", [3, 1], None)])))
            .contains("declared twice")
    );
    assert!(err(w.declare(1, decl(vec![v("", [3, 1], None)]))).contains("tag"));
    let many = (0..17).map(|i| v("a", [i + 1, 1], None)).collect();
    assert!(err(w.declare(1, decl(many))).contains("16 at most"));
    let bad_border = Declare {
        widgets: vec![],
        border: Some(Border::Name("paintng".into())),
    };
    assert!(err(w.declare(1, bad_border)).contains("no such border: paintng"));
    // Nothing was applied.
    assert!(w.tags_of(1).is_empty());

    let tags = w
        .declare(
            1,
            decl(vec![
                v("clock", [5, 1], None),
                v("clock", [8, 3], Some("big")),
            ]),
        )
        .unwrap();
    assert_eq!(tags, ["clock"]);
    let tags = w.declare(1, decl(vec![v("disk", [10, 2], None)])).unwrap();
    assert_eq!(tags, ["clock", "disk"]);
    let id = w.show(1, Show::new("clock")).unwrap();
    assert_eq!(w.variants_named(id), ["5x1", "big"]);
    w.declare(1, decl(vec![v("clock", [4, 1], None)])).unwrap();
    assert_eq!(w.variants_named(id), ["4x1"]);
    let many: Vec<VariantDecl> = (0..63).map(|i| v(&format!("t{i}"), [1, 1], None)).collect();
    assert!(err(w.declare(1, decl(many))).contains("64 a connection"));
    // Tags are per connection.
    assert!(matches!(
        w.show(2, Show::new("clock")),
        Err(WidgetError::BadParams(m)) if m == "no such tag: clock"
    ));
}

#[test]
fn null_members_are_absent() {
    let show: Show = serde_json::from_value(serde_json::json!({
        "tag": "t", "frame": null, "prefer": null, "fill": null, "title": null,
        "border": null, "layer": null,
    }))
    .unwrap();
    assert_eq!(show, Show::new("t"));
    let decl: Declare =
        serde_json::from_value(serde_json::json!({"widgets": null, "border": null})).unwrap();
    assert_eq!(decl, Declare::default());
    let set: Set = serde_json::from_value(
        serde_json::json!({"id": 1, "variant": null, "content": null, "title": "x"}),
    )
    .unwrap();
    assert_eq!(set.title.as_deref(), Some("x"));
    assert!(serde_json::from_value::<Show>(serde_json::json!({"tag": "t", "rows": []})).is_err());
}

#[test]
fn show_and_set_errors() {
    let mut rig = Rig::new(40, 10);
    rig.declare(1, "clock", &[[5, 1]]);
    let mut show = Show::new("clock");
    show.fill = 0.05;
    assert!(matches!(
        rig.w.show(1, show),
        Err(WidgetError::BadParams(_))
    ));
    let mut show = Show::new("clock");
    show.frame = FrameSel::Name("activ".into());
    assert_eq!(
        rig.w.show(1, show),
        Err(WidgetError::NoFrame("activ".into()))
    );
    let mut show = Show::new("clock");
    show.border = Some(Border::Object(BorderPatch {
        corners: Some("++".into()),
        ..BorderPatch::default()
    }));
    assert!(
        matches!(rig.w.show(1, show), Err(WidgetError::BadParams(m)) if m.contains("four glyphs"))
    );

    // A screen full of text: hidden.
    for y in 0..10 {
        rig.write(0, y, &"x".repeat(40));
    }
    let id = rig.show(1, Show::new("clock"));
    assert_eq!(rig.info(id).state, WidgetState::Hidden);
    let set = |content: Option<Content>, variant: Option<&str>, title: Option<&str>| Set {
        id,
        variant: variant.map(String::from),
        content,
        title: title.map(String::from),
    };
    assert!(matches!(
        rig.w.set(1, set(Some(Content::rows(&["x"])), None, None)),
        Err(WidgetError::NotUsable(_))
    ));
    assert!(matches!(
        rig.w.set(1, set(None, None, None)),
        Err(WidgetError::BadParams(_))
    ));
    assert!(matches!(
        rig.w.set(1, set(Some(Content::rows(&["x"])), Some("6x1"), None)),
        Err(WidgetError::BadParams(m)) if m == "no such variant: 6x1"
    ));
    rig.w
        .set(1, set(Some(Content::rows(&["x"])), Some("5x1"), None))
        .unwrap();
    rig.w.set(1, set(None, None, Some("time"))).unwrap();
    assert_eq!(
        rig.w.set(2, set(None, None, Some("t"))),
        Err(WidgetError::NoWidget(id))
    );
    let mut wrong = set(None, None, Some("t"));
    wrong.id = 99;
    assert_eq!(rig.w.set(1, wrong), Err(WidgetError::NoWidget(99)));
    assert_eq!(WidgetError::NoWidget(99).code(), "no-sprite");
    rig.tick();
    assert_eq!(rig.info(id).title, "time");
}

// Layout.

#[test]
fn alone_on_a_blank_screen_fill_and_centre() {
    let mut rig = Rig::new(80, 24);
    rig.declare(1, "t", &[[10, 3], [30, 8], [60, 16]]);
    let id = rig.show(1, Show::new("t"));
    // Room 1920 cells, half is 960: 62x18 (1116) is the nearest.
    let info = rig.info(id);
    assert_eq!(info.state, WidgetState::Placed);
    assert_eq!(info.variant.as_deref(), Some("60x16"));
    assert_eq!(info.size, Some([60, 16]));
    assert_eq!(info.rect, Some([9, 3, 62, 18]));
    assert_eq!(info.border.as_deref(), Some("plain"));
    assert_eq!(info.title, "t");
    let sprite = info.sprite.unwrap();
    assert!(rig.scene.contains(sprite));
    assert_eq!(rig.scene.list()[0].layer, -1);
    assert_eq!(
        rig.shown()[3],
        format!("{}┌ t {}┐", " ".repeat(9), "─".repeat(57))
    );

    // fill 0.1: 192 cells: 32x10 (320) beats 12x5 (60).
    let mut show = Show::new("t");
    show.fill = 0.1;
    rig.show(1, show);
    assert_eq!(rig.rect(id), Some([24, 7, 32, 10]));
    assert_eq!(rig.w.list().len(), 1, "shown again: the same widget");
}

#[test]
fn text_cursor_panel_and_hot_cells_are_not_open() {
    let mut rig = Rig::new(40, 12);
    // Text on the left half, everywhere.
    for y in 0..12 {
        rig.write(0, y, &"x".repeat(18));
    }
    rig.run(2000);
    rig.declare(1, "t", &[[6, 2]]);
    let id = rig.show(1, Show::new("t"));
    // Room: columns 18..40 (22), gap 2 from the text, none at the edge:
    // usable 20 columns, a widget of 8 centred: column 20 + 6.
    let [x, _, w, h] = rig.rect(id).unwrap();
    assert_eq!((x, w, h), (26, 8, 4));

    // The cursor row: never under a widget.
    let mut rig = Rig::new(20, 7);
    rig.cursor = Some((0, 2));
    rig.declare(1, "t", &[[16, 1]]);
    let id = rig.show(1, Show::new("t"));
    // Rows 0 and 1 are too few: under it, a row of gap.
    assert_eq!(rig.rect(id), Some([1, 4, 18, 3]));

    // A panel everywhere: hidden.
    let mut rig = Rig::new(20, 6);
    rig.mock.panel.push(Rect::new(0, 0, 20, 6));
    rig.declare(1, "t", &[[4, 1]]);
    let id = rig.show(1, Show::new("t"));
    assert_eq!(rig.info(id).state, WidgetState::Hidden);

    // Cells cleared just now: open after `quiet_ms`, with no change on
    // screen in between.
    let mut rig = Rig::new(20, 6);
    for y in 0..6 {
        rig.write(0, y, &"x".repeat(20));
    }
    rig.declare(1, "t", &[[4, 1]]);
    let id = rig.show(1, Show::new("t"));
    assert_eq!(rig.info(id).state, WidgetState::Hidden);
    for y in 0..6 {
        rig.clear(y);
    }
    rig.run(500);
    assert_eq!(rig.info(id).state, WidgetState::Hidden);
    rig.run(600);
    assert_eq!(rig.info(id).state, WidgetState::Placed);
}

#[test]
fn rooms_ranked_by_variant_then_prefer() {
    // Two rooms, left 30 columns, right 12: the left gives the larger.
    let mut rig = Rig::new(50, 10);
    for y in 0..10 {
        rig.write(30, y, &"x".repeat(8));
    }
    rig.run(2000);
    rig.declare(1, "t", &[[4, 2], [20, 4]]);
    let id = rig.show(1, Show::new("t"));
    assert_eq!(rig.info(id).variant.as_deref(), Some("20x4"));
    assert!(rig.rect(id).unwrap()[0] < 30);

    // Two equal rooms: `prefer` picks; `any`: the leftmost.
    let mut rig = Rig::new(50, 10);
    for y in 0..10 {
        rig.write(21, y, &"x".repeat(8));
    }
    rig.run(2000);
    rig.declare(1, "t", &[[6, 2]]);
    let id = rig.show(1, Show::new("t"));
    assert!(rig.rect(id).unwrap()[0] < 21);
    let mut show = Show::new("t");
    show.prefer = Prefer::TopRight;
    rig.show(1, show);
    assert!(rig.rect(id).unwrap()[0] > 29);
}

#[test]
fn prefer_names_the_side_of_the_room() {
    let mut rig = Rig::new(80, 24);
    rig.declare(1, "t", &[[10, 3]]);
    let mut at = |prefer: Prefer| {
        let mut show = Show::new("t");
        show.prefer = prefer;
        let id = rig.show(1, show);
        rig.rect(id).unwrap()
    };
    assert_eq!(at(Prefer::Any), [34, 9, 12, 5]);
    assert_eq!(at(Prefer::Center), [34, 9, 12, 5]);
    assert_eq!(at(Prefer::TopRight), [68, 0, 12, 5]);
    assert_eq!(at(Prefer::Top), [34, 0, 12, 5]);
    assert_eq!(at(Prefer::Left), [0, 9, 12, 5]);
    assert_eq!(at(Prefer::BottomLeft), [0, 19, 12, 5]);

    // Not on an edge of the view: the gap is kept.
    let mut rig = Rig::new(80, 24);
    for y in 0..24 {
        rig.write(60, y, &"x".repeat(20));
    }
    rig.write(0, 0, &"x".repeat(60));
    rig.run(2000);
    rig.declare(1, "t", &[[10, 3]]);
    let mut show = Show::new("t");
    show.prefer = Prefer::TopRight;
    let id = rig.show(1, show);
    assert_eq!(rig.rect(id), Some([46, 2, 12, 5]));
}

#[test]
fn placed_one_shrinks_to_let_a_hidden_one_in() {
    let mut rig = Rig::new(40, 10);
    rig.declare(1, "a", &[[10, 2], [30, 6]]);
    rig.declare(2, "b", &[[10, 2]]);
    let a = rig.show(1, Show::new("a"));
    assert_eq!(rig.info(a).variant.as_deref(), Some("30x6"));
    // No room around `a` for `b`: `a` takes its smaller variant.
    let b = rig.show(2, Show::new("b"));
    assert_eq!(rig.info(a).variant.as_deref(), Some("10x2"));
    assert_eq!(rig.info(b).state, WidgetState::Placed);
    let (ra, rb) = (rig.rect(a).unwrap(), rig.rect(b).unwrap());
    let [x, y, w, h] = ra;
    assert!(!overlaps(
        [x.saturating_sub(2), y.saturating_sub(1), w + 4, h + 2],
        rb
    ));
    assert!(rig.events.iter().any(|e| e.id == a && e.rect == Some(ra)));
    assert_eq!(rig.scene.list().len(), 2);
    // Settled: nothing moves.
    rig.run(5000);
    assert_eq!((rig.rect(a), rig.rect(b)), (Some(ra), Some(rb)));
    assert_eq!(rig.w.next_wake(), None);
    // `b` goes: `a` grows back.
    assert!(rig.w.hide(2, b));
    rig.run(2600);
    assert_eq!(rig.info(a).variant.as_deref(), Some("30x6"));

    // No smaller variant makes the room: the first stays, as it is.
    let mut rig = Rig::new(20, 6);
    rig.declare(1, "a", &[[14, 2]]);
    rig.declare(2, "b", &[[14, 2]]);
    let a = rig.show(1, Show::new("a"));
    let first = rig.rect(a);
    let b = rig.show(2, Show::new("b"));
    rig.run(1000);
    assert_eq!(rig.rect(a), first);
    assert_eq!(rig.info(b).state, WidgetState::Hidden);
}

#[test]
fn pressed_then_evicted_elsewhere_smaller_or_hidden() {
    let mut rig = Rig::new(40, 10);
    rig.declare(1, "t", &[[4, 1], [20, 4]]);
    let id = rig.show(1, Show::new("t"));
    let [x, y, w, _] = rig.rect(id).unwrap();
    assert_eq!(rig.info(id).variant.as_deref(), Some("20x4"));
    // Text under it: pressed, it stays drawn, the text shows over it.
    rig.write(x + 2, y + 2, "TEXT");
    rig.run(200);
    assert_eq!(rig.rect(id).unwrap()[..2], [x, y]);
    assert!(rig.shown()[(y + 2) as usize].contains("TEXT"));
    // Pressed 300 ms: laid out again, elsewhere.
    rig.run(150);
    let moved = rig.rect(id).unwrap();
    assert_ne!(moved[..2], [x, y]);
    assert!(!overlaps(moved, [x + 2, y + 2, 4, 1]));
    assert!(
        rig.events
            .iter()
            .any(|e| e.id == id && e.rect == Some(moved))
    );
    let _ = w;

    // Text on most of the screen: smaller.
    for row in 0..10 {
        rig.write(0, row, &"x".repeat(32));
    }
    rig.run(400);
    assert_eq!(rig.info(id).variant.as_deref(), Some("4x1"));
    // Nothing left: hidden; the sprite goes.
    for row in 0..10 {
        rig.write(0, row, &"x".repeat(40));
    }
    rig.run(400);
    let info = rig.info(id);
    assert_eq!(info.state, WidgetState::Hidden);
    assert_eq!((info.rect, info.sprite, info.variant), (None, None, None));
    assert!(rig.scene.is_empty());
}

#[test]
fn grown_after_two_seconds_never_for_the_same_size() {
    let mut rig = Rig::new(60, 12);
    // A column of text keeps the widget in a small room on the right.
    for y in 0..12 {
        rig.write(0, y, &"x".repeat(44));
    }
    rig.run(2000);
    rig.declare(1, "t", &[[6, 2], [30, 6]]);
    let id = rig.show(1, Show::new("t"));
    assert_eq!(rig.info(id).variant.as_deref(), Some("6x2"));
    let small = rig.rect(id).unwrap();
    // The text goes: a larger variant fits, after 2 s of looks only.
    for y in 0..12 {
        rig.clear(y);
    }
    rig.run(1500);
    assert_eq!(rig.rect(id), Some(small));
    rig.run(1000);
    assert_eq!(rig.rect(id), Some(small));
    rig.run(1200);
    assert_eq!(rig.info(id).variant.as_deref(), Some("30x6"));
    // Settled: it stays; a quiet screen needs no wake.
    let big = rig.rect(id);
    rig.run(5000);
    assert_eq!(rig.rect(id), big);
    assert_eq!(rig.w.next_wake(), None);

    // Same size elsewhere: never moves.
    let mut rig = Rig::new(60, 12);
    for y in 0..12 {
        rig.write(0, y, &"x".repeat(44));
    }
    rig.run(2000);
    rig.declare(1, "t", &[[6, 2]]);
    let id = rig.show(1, Show::new("t"));
    let first = rig.rect(id);
    for y in 0..12 {
        rig.clear(y);
    }
    rig.run(6000);
    assert_eq!(rig.rect(id), first);
}

#[test]
fn two_owners_pave_without_overlap() {
    let mut rig = Rig::new(80, 24);
    rig.declare(1, "a", &[[10, 3], [30, 8]]);
    rig.declare(2, "b", &[[10, 3], [30, 8]]);
    rig.declare(2, "c", &[[10, 3], [30, 8]]);
    let a = rig.show(1, Show::new("a"));
    let b = rig.show(2, Show::new("b"));
    let c = rig.show(2, Show::new("c"));
    rig.run(500);
    let rects: Vec<[u16; 4]> = [a, b, c].iter().map(|id| rig.rect(*id).unwrap()).collect();
    for i in 0..3 {
        for j in i + 1..3 {
            // Apart by the gap at least.
            let [x, y, w, h] = rects[i];
            let grown = [x.saturating_sub(2), y.saturating_sub(1), w + 4, h + 2];
            assert!(!overlaps(grown, rects[j]), "{rects:?}");
        }
    }
    // One owner leaves: its widget goes, the others stay.
    rig.w.drop_owner(1);
    rig.tick();
    assert!(rig.w.info(a).is_none());
    assert!(
        rig.events
            .iter()
            .any(|e| e.id == a && e.state == WidgetState::Gone)
    );
    assert_eq!(rig.w.list().len(), 2);
    assert_eq!(rig.scene.list().len(), 2);
}

#[test]
fn declaration_replaced_lays_out_again() {
    let mut rig = Rig::new(60, 20);
    rig.declare(1, "t", &[[30, 8]]);
    let id = rig.show(1, Show::new("t"));
    assert_eq!(rig.info(id).size, Some([30, 8]));
    rig.declare(1, "t", &[[10, 2]]);
    rig.run(300);
    let info = rig.info(id);
    assert_eq!(info.size, Some([10, 2]));
    assert_eq!(
        info.variants,
        [VariantInfo {
            name: "10x2".into(),
            size: [10, 2]
        }]
    );
    // A variant of the same name and size, given no content, keeps its
    // own; another size is blank.
    let set = |rig: &mut Rig, name: &str, size: [u16; 2], rows: Option<&[&str]>| {
        let decl = Declare {
            widgets: vec![VariantDecl {
                tag: "k".into(),
                name: Some(name.into()),
                size,
                content: rows.map(Content::rows),
            }],
            border: Some(Border::Name("none".into())),
        };
        rig.w.declare(2, decl).unwrap();
        rig.run(300);
    };
    set(&mut rig, "v", [4, 1], Some(&["abcd"]));
    let k = rig.show(2, Show::new("k"));
    let text = |rig: &Rig| {
        let [x, y, w, _] = rig.rect(k).unwrap();
        let row = &rig.shown()[y as usize];
        row.chars()
            .skip(x as usize)
            .take(w as usize)
            .collect::<String>()
    };
    assert_eq!(text(&rig), "abcd");
    set(&mut rig, "v", [4, 1], None);
    assert_eq!(text(&rig), "abcd");
    set(&mut rig, "v", [3, 1], None);
    assert_eq!(text(&rig).trim_end(), "");

    // The connection's border, given at declare: the widget takes it.
    rig.w
        .declare(
            1,
            Declare {
                widgets: vec![],
                border: Some(Border::Name("none".into())),
            },
        )
        .unwrap();
    rig.run(300);
    let info = rig.info(id);
    assert_eq!(info.border.as_deref(), Some("none"));
    assert_eq!(info.rect.unwrap()[2..], [10, 2]);
}

#[test]
fn set_content_title_and_hide() {
    let mut rig = Rig::new(40, 10);
    let decl = Declare {
        widgets: vec![VariantDecl {
            tag: "clock".into(),
            name: Some("hm".into()),
            size: [5, 1],
            content: Some(Content::rows(&["--:--"])),
        }],
        border: Some(Border::Name("round".into())),
    };
    rig.w.declare(1, decl).unwrap();
    let id = rig.show(1, Show::new("clock"));
    let [x, y, _, _] = rig.rect(id).unwrap();
    let row = |rig: &Rig, dy: u16| {
        rig.shown()[(y + dy) as usize][x as usize..]
            .chars()
            .take(7)
            .collect::<String>()
    };
    assert_eq!(row(&rig, 1), "│--:--│");
    rig.w
        .set(
            1,
            Set {
                id,
                content: Some(Content::rows(&["12:34"])),
                title: Some("".into()),
                ..Set::default()
            },
        )
        .unwrap();
    assert!(rig.w.next_wake().is_some());
    rig.tick();
    assert_eq!(row(&rig, 0), "╭─────╮");
    assert_eq!(row(&rig, 1), "│12:34│");
    // The sprite was updated in place.
    assert_eq!(rig.scene.list().len(), 1);

    // Back layer: what the app writes over it shows.
    rig.write(x + 1, y + 1, "AB");
    rig.tick();
    assert_eq!(row(&rig, 1), "│AB:34│");

    assert!(!rig.w.hide(2, id));
    assert!(rig.w.hide(1, id));
    let out = rig.tick();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].state, WidgetState::Gone);
    assert!(rig.scene.is_empty());
    assert!(rig.w.list().is_empty());
    // The declaration stays: shown again, a new widget.
    let again = rig.show(1, Show::new("clock"));
    assert_ne!(again, id);
}

#[test]
fn sprite_removed_by_hand_hides_the_widget() {
    let mut rig = Rig::new(40, 10);
    rig.declare(1, "t", &[[5, 1]]);
    let id = rig.show(1, Show::new("t"));
    let sprite = rig.info(id).sprite.unwrap();
    assert!(rig.scene.remove(sprite));
    let out = rig.tick();
    assert!(
        out.iter()
            .any(|e| e.id == id && e.state == WidgetState::Gone)
    );
    assert!(rig.w.info(id).is_none());
}

#[test]
fn frame_layer_and_view_cells() {
    let mut rig = Rig::new(40, 12);
    rig.mock.infos.push(surface(PANE, Rect::new(20, 2, 20, 10)));
    rig.declare(1, "t", &[[6, 2]]);
    let mut show = Show::new("t");
    show.frame = FrameSel::Id(PANE);
    show.layer = Some(Layer::FRONT);
    let id = rig.show(1, show);
    let info = rig.info(id);
    assert_eq!(info.frame, Some(PANE));
    let [x, y, w, h] = info.rect.unwrap();
    assert!(x >= 20 && y >= 2 && x + w <= 40 && y + h <= 12);
    let sprite = &rig.scene.list()[0];
    assert_eq!(sprite.layer, 1);
    assert_eq!(sprite.frame, Some(PANE));
    assert_eq!(
        sprite.anchor,
        Anchor::At {
            at: [(x - 20) as i64, (y - 2) as i64],
            space: Space::View
        }
    );
    // The frame not usable: kept, drawn again when it is.
    rig.mock.infos[1].usable = false;
    rig.w.touch();
    rig.run(1000);
    assert_eq!(rig.info(id).state, WidgetState::Placed);
    // The frame shrinks under it: out of the view, laid out again.
    rig.mock.infos[1].usable = true;
    rig.mock.infos[1].rect = Rect::new(20, 2, 12, 6);
    rig.w.touch();
    rig.run(1000);
    let [x, y, w, h] = rig.rect(id).unwrap();
    assert!(x + w <= 32 && y + h <= 8, "{:?}", rig.rect(id));
}

#[test]
fn quiet_screen_costs_nothing() {
    let mut rig = Rig::new(80, 24);
    assert_eq!(rig.w.next_wake(), None);
    rig.declare(1, "t", &[[10, 3]]);
    rig.show(1, Show::new("t"));
    rig.run(3000);
    assert_eq!(rig.w.next_wake(), None);
    // Changes: looks, `look_ms` apart.
    rig.write(0, 0, "x");
    rig.now += Duration::from_millis(10);
    rig.write(1, 0, "x");
    let wake = rig.w.next_wake().unwrap();
    assert_eq!(wake, rig.now + Duration::from_millis(240));
    rig.run(3000);
    assert_eq!(rig.w.next_wake(), None);
}

#[test]
#[ignore]
fn look_cost_at_200x60() {
    let mut rig = Rig::new(200, 60);
    for y in (0..60).step_by(3) {
        rig.write(y * 7 % 120, y, &"word ".repeat(12));
    }
    rig.run(2000);
    for owner in 1..=8 {
        rig.declare(owner, "t", &[[8, 2], [20, 5], [40, 10]]);
        rig.show(owner, Show::new("t"));
    }
    let started = std::time::Instant::now();
    for _ in 0..20 {
        rig.w.urgent = true;
        rig.tick();
    }
    eprintln!("look: {:?}", started.elapsed() / 20);

    // The worst case: a widget hidden, no smaller variant of the others
    // lets it in.
    rig.declare(9, "t", &[[150, 50]]);
    rig.show(9, Show::new("t"));
    let started = std::time::Instant::now();
    for _ in 0..20 {
        rig.w.urgent = true;
        rig.tick();
    }
    let placed = rig.w.list().iter().filter(|i| i.rect.is_some()).count();
    eprintln!(
        "look, one hidden: {:?}, {placed} of 9 placed",
        started.elapsed() / 20
    );
}
