// SPDX-License-Identifier: GPL-3.0-or-later
//! Emulator behaviour, feature by feature.

use ptyrc_vt::{Attrs, Charset, Color, Cursor, Rect, Vt, VtEvent};

fn vt(cols: u16, rows: u16, bytes: &str) -> Vt {
    let mut vt = Vt::new(cols, rows);
    vt.feed(bytes.as_bytes());
    vt
}

fn pos(vt: &Vt) -> (u16, u16) {
    let c = vt.cursor();
    (c.col, c.row)
}

#[test]
fn print_and_wrap() {
    let v = vt(5, 3, "abcdefg");
    assert_eq!(v.grid().text(), ["abcde", "fg", ""]);
    assert_eq!(pos(&v), (2, 1));
}

#[test]
fn pending_wrap() {
    let mut v = vt(5, 3, "abcde");
    assert_eq!(
        v.cursor(),
        Cursor {
            col: 4,
            row: 0,
            visible: true,
            pending_wrap: true
        }
    );
    v.feed(b"\r\nx");
    assert_eq!(v.grid().text(), ["abcde", "x", ""]);
    // A cursor move drops the pending wrap.
    let v = vt(5, 3, "abcde\x1b[Dx");
    assert_eq!(v.grid().text(), ["abcxe", "", ""]);
}

#[test]
fn no_autowrap() {
    let v = vt(5, 2, "\x1b[?7labcdefg");
    assert_eq!(v.grid().text(), ["abcdg", ""]);
    assert!(!v.cursor().pending_wrap);
}

#[test]
fn linefeed_scrolls_with_event() {
    let mut v = vt(4, 3, "a\r\nb\r\nc\r\nd\r\ne");
    assert_eq!(v.grid().text(), ["c", "d", "e"]);
    assert_eq!(
        v.take_events(),
        [VtEvent::Scroll {
            region: Rect::new(0, 0, 4, 3),
            dy: -2,
            scrollback: true
        }]
    );
    assert!(v.take_events().is_empty());
}

#[test]
fn margins_and_region_scroll() {
    let mut v = vt(4, 5, "1\r\n2\r\n3\r\n4\r\n5\x1b[2;4r\x1b[4;1H\n");
    assert_eq!(v.grid().text(), ["1", "3", "4", "", "5"]);
    assert_eq!(
        v.take_events(),
        [VtEvent::Scroll {
            region: Rect::new(0, 1, 4, 3),
            dy: -1,
            scrollback: false
        }]
    );
    // RI at the top margin scrolls down.
    v.feed(b"\x1b[2;1H\x1bM");
    assert_eq!(v.grid().text(), ["1", "", "3", "4", "5"]);
    assert_eq!(
        v.take_events(),
        [VtEvent::Scroll {
            region: Rect::new(0, 1, 4, 3),
            dy: 1,
            scrollback: false
        }]
    );
    assert_eq!(v.margins(), (1, 4));
}

#[test]
fn su_sd_il_dl() {
    let mut v = vt(3, 4, "a\r\nb\r\nc\r\nd\x1b[S");
    assert_eq!(v.grid().text(), ["b", "c", "d", ""]);
    v.feed(b"\x1b[2T");
    assert_eq!(v.grid().text(), ["", "", "b", "c"]);
    v.take_events();
    v.feed(b"\x1b[3;1H\x1b[L");
    assert_eq!(v.grid().text(), ["", "", "", "b"]);
    assert_eq!(
        v.take_events(),
        [VtEvent::Scroll {
            region: Rect::new(0, 2, 3, 2),
            dy: 1,
            scrollback: false
        }]
    );
    v.feed(b"\x1b[1;1H\x1b[2M");
    assert_eq!(v.grid().text(), ["", "b", "", ""]);
    assert_eq!(
        v.take_events(),
        [VtEvent::Scroll {
            region: Rect::new(0, 0, 3, 4),
            dy: -2,
            scrollback: false
        }]
    );
}

#[test]
fn cursor_moves() {
    let v = vt(10, 5, "\x1b[3;4H");
    assert_eq!(pos(&v), (3, 2));
    let v = vt(10, 5, "\x1b[3;4H\x1b[A\x1b[2C\x1b[B\x1b[B\x1b[3D");
    assert_eq!(pos(&v), (2, 3));
    let v = vt(10, 5, "\x1b[99;99H");
    assert_eq!(pos(&v), (9, 4));
    let v = vt(10, 5, "\x1b[3;4H\x1b[7G\x1b[2d");
    assert_eq!(pos(&v), (6, 1));
    let v = vt(10, 5, "\x1b[3;4H\x1b[E");
    assert_eq!(pos(&v), (0, 3));
    let v = vt(10, 5, "abc\x08\x08x");
    assert_eq!(v.grid().text()[0], "axc");
}

#[test]
fn erase() {
    let v = vt(5, 3, "abcde\r\nfghij\r\nklmno\x1b[2;3H\x1b[K");
    assert_eq!(v.grid().text(), ["abcde", "fg", "klmno"]);
    let v = vt(5, 3, "abcde\r\nfghij\r\nklmno\x1b[2;3H\x1b[1K");
    assert_eq!(v.grid().text(), ["abcde", "   ij", "klmno"]);
    let v = vt(5, 3, "abcde\r\nfghij\r\nklmno\x1b[2;3H\x1b[J");
    assert_eq!(v.grid().text(), ["abcde", "fg", ""]);
    let v = vt(5, 3, "abcde\r\nfghij\r\nklmno\x1b[2;3H\x1b[1J");
    assert_eq!(v.grid().text(), ["", "   ij", "klmno"]);
    let mut v = vt(5, 3, "abcde\r\nfghij\r\nklmno\x1b[2;3H\x1b[2J");
    assert_eq!(v.grid().text(), ["", "", ""]);
    assert!(v.take_events().contains(&VtEvent::Clear));
    v.feed(b"abc\x1b[J");
    assert!(!v.take_events().contains(&VtEvent::Clear));
    v.feed(b"\x1b[H\x1b[J");
    assert!(v.take_events().contains(&VtEvent::Clear));
    let v = vt(5, 1, "abcde\x1b[1;2H\x1b[2X");
    assert_eq!(v.grid().text(), ["a  de"]);
}

#[test]
fn erase_uses_pen_background() {
    let v = vt(4, 1, "\x1b[44mab\x1b[K");
    assert_eq!(v.grid().cell(3, 0).style.bg, Color::Indexed(4));
    assert!(!v.grid().cell(3, 0).is_empty());
}

#[test]
fn insert_delete_chars() {
    let v = vt(6, 1, "abcdef\x1b[1;2H\x1b[2@");
    assert_eq!(v.grid().text(), ["a  bcd"]);
    let v = vt(6, 1, "abcdef\x1b[1;2H\x1b[2P");
    assert_eq!(v.grid().text(), ["adef"]);
    let v = vt(6, 1, "abc\x1b[1;2H\x1b[4hXY\x1b[4lZ");
    assert_eq!(v.grid().text(), ["aXYZc"]);
}

#[test]
fn sgr() {
    let v = vt(
        20,
        1,
        "\x1b[1;3;4;31;42ma\x1b[0mb\x1b[38;5;200;48;2;1;2;3mc\x1b[91;103md",
    );
    let g = v.grid();
    let a = g.cell(0, 0).style;
    assert!(
        a.attrs
            .contains(Attrs::BOLD | Attrs::ITALIC | Attrs::UNDERLINE)
    );
    assert_eq!((a.fg, a.bg), (Color::Indexed(1), Color::Indexed(2)));
    assert_eq!(g.cell(1, 0).style, Default::default());
    let c = g.cell(2, 0).style;
    assert_eq!((c.fg, c.bg), (Color::Indexed(200), Color::Rgb(1, 2, 3)));
    let d = g.cell(3, 0).style;
    assert_eq!((d.fg, d.bg), (Color::Indexed(9), Color::Indexed(11)));
}

#[test]
fn sgr_colon_forms() {
    let v = vt(
        20,
        1,
        "\x1b[4:3;58:2::10:20:30;38:2:1:2:3ma\x1b[4:0;58:5:7;38:5:9mb\x1b[21mc\x1b[24md",
    );
    let g = v.grid();
    let a = g.cell(0, 0).style;
    assert!(a.attrs.contains(Attrs::UNDERLINE | Attrs::UL_CURLY));
    assert_eq!(a.ul, Color::Rgb(10, 20, 30));
    assert_eq!(a.fg, Color::Rgb(1, 2, 3));
    let b = g.cell(1, 0).style;
    assert!(!b.attrs.contains(Attrs::UNDERLINE));
    assert_eq!((b.ul, b.fg), (Color::Indexed(7), Color::Indexed(9)));
    assert!(
        g.cell(2, 0)
            .style
            .attrs
            .contains(Attrs::UNDERLINE | Attrs::UL_DOUBLE)
    );
    assert_eq!(
        g.cell(3, 0).style.attrs.0 & (Attrs::UNDERLINE | Attrs::UL_SHAPES),
        0
    );
}

#[test]
fn sgr_round_trip() {
    let seqs = [
        "\x1b[1;2;3;4:4;5;7;8;9;53;35;105;58:5:3m",
        "\x1b[38;2;9;8;7;48;5;99;4m",
        "\x1b[m",
        "\x1b[21;58:2::1:2:3m",
    ];
    for seq in seqs {
        let a = vt(4, 1, seq);
        let mut out = Vec::new();
        ptyrc_vt::write_sgr(&mut out, &a.pen());
        let mut b = Vt::new(4, 1);
        b.feed(&out);
        assert_eq!(a.pen(), b.pen(), "{seq:?}");
    }
}

#[test]
fn save_restore_cursor() {
    let v = vt(10, 5, "\x1b[2;3H\x1b[31m\x1b7\x1b[5;5H\x1b[0m\x1b8x");
    assert_eq!(pos(&v), (3, 1));
    assert_eq!(v.grid().cell(2, 1).style.fg, Color::Indexed(1));
    let v = vt(10, 5, "\x1b[2;3H\x1b[s\x1b[H\x1b[u");
    assert_eq!(pos(&v), (2, 1));
}

#[test]
fn alt_screen() {
    let mut v = vt(5, 2, "main\x1b[?1049hALT");
    assert!(v.modes().alt_screen);
    assert_eq!(v.grid().text()[0], "    A");
    assert_eq!(v.grid().text()[1], "LT");
    v.feed(b"\x1b[?1049l");
    assert!(!v.modes().alt_screen);
    assert_eq!(v.grid().text(), ["main", ""]);
    assert_eq!(pos(&v), (4, 0));
    assert_eq!(
        v.take_events(),
        [VtEvent::AltScreen(true), VtEvent::AltScreen(false)]
    );
    // Alt-screen scrolling never reaches the scrollback.
    let mut v = vt(5, 2, "\x1b[?1049ha\r\nb\r\nc");
    v.take_events().iter().for_each(|e| {
        if let VtEvent::Scroll { scrollback, .. } = e {
            assert!(!scrollback);
        }
    });
}

#[test]
fn origin_mode() {
    let v = vt(10, 6, "\x1b[2;4r\x1b[?6h\x1b[1;1Hx\x1b[9;1Hy");
    assert_eq!(v.grid().text(), ["", "x", "", "y", "", ""]);
    assert!(v.modes().origin);
}

#[test]
fn charsets() {
    let v = vt(10, 1, "\x1b(0lqk\x1b(Bq\x1b)0\x0eq\x0fq");
    assert_eq!(v.grid().text(), ["┌─┐q─q"]);
    assert_eq!(v.modes().g1, Charset::DecSpecial);
}

#[test]
fn tabs() {
    let v = vt(20, 1, "a\tb\tc");
    assert_eq!(v.grid().text(), ["a       b       c"]);
    let v = vt(20, 1, "\x1b[3g\x1b[1;5H\x1bH\ra\tb");
    assert_eq!(v.grid().text(), ["a   b"]);
    let v = vt(20, 1, "\x1b[1;12H\x1b[Zx");
    assert_eq!(pos(&v), (9, 0));
}

#[test]
fn wide_and_combining() {
    let v = vt(6, 2, "a世b");
    let g = v.grid();
    assert_eq!((g.cell(1, 0).width, g.cell(2, 0).width), (2, 0));
    assert_eq!(g.text()[0], "a世b");
    assert_eq!(pos(&v), (4, 0));
    // Does not fit on the last column: wraps whole.
    let v = vt(4, 2, "abc世");
    assert_eq!(v.grid().text(), ["abc", "世"]);
    // Combining mark joins the previous glyph, also under a pending wrap.
    let v = vt(3, 1, "e\u{301}xy\u{308}");
    assert_eq!(v.grid().cell(0, 0).text, "e\u{301}");
    assert_eq!(v.grid().cell(2, 0).text, "y\u{308}");
    // Overwriting half a wide glyph blanks the other half.
    let v = vt(6, 1, "世\x1b[1;2Hx");
    assert_eq!(v.grid().text(), [" x"]);
}

#[test]
fn utf8_split_and_invalid() {
    let mut v = Vt::new(8, 1);
    let bytes = "é世".as_bytes();
    for b in bytes {
        v.feed(&[*b]);
    }
    assert!(v.in_ground());
    assert_eq!(v.grid().text(), ["é世"]);
    let mut v = Vt::new(8, 1);
    v.feed(b"a\xffb\xe4\xb8c");
    assert_eq!(v.grid().text(), ["a\u{fffd}b\u{fffd}c"]);
}

#[test]
fn in_ground() {
    let mut v = Vt::new(8, 1);
    assert!(v.in_ground());
    for (chunk, ground) in [
        (&b"\x1b"[..], false),
        (b"[", false),
        (b"3", false),
        (b"1m", true),
        (b"\xe4", false),
        (b"\xb8\x96", true),
        (b"\x1b]0;ti", false),
        (b"tle\x1b", false),
        (b"\\", true),
        (b"\x1bPq#0", false),
        (b"\x1b\\", true),
    ] {
        v.feed(chunk);
        assert_eq!(v.in_ground(), ground, "{chunk:?}");
    }
    assert_eq!(v.title(), "title");
    assert_eq!(v.grid().text(), ["世"]);
    assert_eq!(v.pen().fg, Color::Indexed(1));
}

#[test]
fn title_and_bell() {
    let mut v = vt(8, 1, "\x1b]2;héllo\x07\x07");
    assert_eq!(v.title(), "héllo");
    assert_eq!(v.take_events(), [VtEvent::Title, VtEvent::Bell]);
}

#[test]
fn hyperlinks() {
    let v = vt(
        8,
        1,
        "\x1b]8;id=1;http://a\x1b\\x\x1b]8;;\x1b\\y\x1b]8;id=1;http://a\x07z",
    );
    let g = v.grid();
    let id = g.cell(0, 0).style.link;
    assert_ne!(id, 0);
    assert_eq!(g.cell(1, 0).style.link, 0);
    assert_eq!(g.cell(2, 0).style.link, id);
    let link = v.link(id).unwrap();
    assert_eq!(
        (link.params.as_str(), link.uri.as_str()),
        ("id=1", "http://a")
    );
    assert!(v.link(0).is_none());
    // SGR 0 keeps the link.
    let v = vt(8, 1, "\x1b]8;;u\x07\x1b[0mx");
    assert_ne!(v.grid().cell(0, 0).style.link, 0);
}

#[test]
fn modes() {
    let v = vt(
        8,
        2,
        "\x1b[?1h\x1b=\x1b[?2004h\x1b[?1002h\x1b[?1006h\x1b[?1004h\x1b[?2026h\x1b[?25l\x1b[5 q",
    );
    let m = v.modes();
    assert!(m.app_cursor && m.app_keypad && m.bracketed_paste && m.mouse);
    assert!(m.focus_events && m.sync_update);
    assert_eq!(m.cursor_shape, 5);
    assert!(!v.cursor().visible);
    let v = vt(8, 2, "\x1b[?1000;1002h\x1b[?1000l");
    assert!(v.modes().mouse);
    let v = vt(8, 2, "\x1b[?1000h\x1b[?1000l\x1b[?2026h\x1b[?2026l");
    assert!(!v.modes().mouse && !v.modes().sync_update);
}

#[test]
fn rep_and_unknown() {
    let v = vt(8, 1, "a\x1b[3b");
    assert_eq!(v.grid().text(), ["aaaa"]);
    // Unknown or private sequences are consumed.
    let v = vt(
        20,
        1,
        "a\x1b[>4;2m\x1b[?u\x1b[>1u\x1b[=5u\x1b[22;0;0t\x1b_Gf=1;xx\x1b\\\x1bP+q\x1b\\\x1b[?12$pb",
    );
    assert_eq!(v.grid().text(), ["ab"]);
    assert_eq!(v.pen(), Default::default());
}

#[test]
fn can_aborts_and_c0_inside_csi() {
    let v = vt(8, 2, "\x1b[3\x18a\x1b[\r1Cb");
    assert_eq!(v.grid().text(), ["ab", ""]);
}

#[test]
fn resize() {
    let mut v = vt(6, 3, "abcdef\r\n123456\r\nxyz");
    v.resize(4, 2);
    // The cursor row stays visible: the top row leaves.
    assert_eq!(v.grid().text(), ["1234", "xyz"]);
    assert_eq!(pos(&v), (3, 1));
    assert_eq!(v.margins(), (0, 2));
    v.resize(8, 4);
    assert_eq!(v.grid().text(), ["1234", "xyz", "", ""]);
}

#[test]
fn reset() {
    let mut v = vt(6, 3, "abc\x1b[31m\x1b[?1049h\x1b[2;3r\x1bc");
    assert_eq!(v.grid().text(), ["", "", ""]);
    assert_eq!(v.pen(), Default::default());
    assert!(!v.modes().alt_screen);
    assert_eq!(v.margins(), (0, 3));
    assert!(v.take_events().contains(&VtEvent::Clear));
}

#[test]
fn clone_and_set_grid() {
    let a = vt(6, 2, "ab\x1b[3");
    let mut b = a.clone();
    b.feed(b"1mc");
    assert_eq!(b.grid().cell(2, 0).style.fg, Color::Indexed(1));
    let mut c = Vt::new(6, 2);
    c.set_grid(b.grid());
    assert_eq!(c.grid(), b.grid());
    assert!(!a.same_state(&b));
    assert!(a.same_state(&a.clone()));
}
