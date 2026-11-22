// SPDX-License-Identifier: GPL-3.0-or-later
//! The twin's pond, as digests: written by
//! `plan/sessions/22112026-01-migration/scenes-pond/twin.py`.

#![allow(clippy::type_complexity)]

/// `drops(seed, t, w, h, rate)`: the digest of the ripples `key,x,y,t0,life,rings,reach;`.
pub const DROPS: &[(i64, f64, usize, usize, f64, u32)] = &[
    (3, 50.0, 80, 24, 240.0, 1656930181),
    (4, 50.0, 80, 24, 240.0, 2673474725),
    (3, 7.25, 160, 48, 24.0, 3956521552),
    (977, 1234.5, 200, 60, 10.0, 2750476106),
];

/// Showers: `drops(seed + 1, t, 80, 24, shower, *SHOWERS)` of the pond of `Random(seed)`.
pub const SHOWERS: &[(u64, f64, u32)] = &[
    (1, 9.0, 2763591664),
    (1, 31.0, 2166136261),
    (6, 77.7, 806347759),
];

/// `shadow(x, y, reach, index(text, 40, 12))`: the digest of `near;far` at 6 decimals, 0 for None.
pub const SHADOWS: &[(&str, f64, f64, f64, u32)] = &[
    ("words", 36.0, 26.0, 40.0, 2578315995),
    ("words", 13.0, 26.0, 30.0, 4251917002),
    ("words", 40.0, 2.0, 30.0, 2083635745),
    ("line", 36.0, 26.0, 40.0, 2911098210),
    ("block", 30.0, 20.0, 30.0, 682441095),
    ("block", 60.0, 50.0, 12.0, 2350982910),
];

/// Runs: seed, keywords, `(w, h)`, text, ticks of 1/20 s, the pointer
/// `(tick, x, y)`, pictures `(tick, digest)`; then the koi's last kisses' digest.
pub const RUNS: &[(
    u64,
    &str,
    (usize, usize),
    &str,
    u32,
    &[(u32, f64, f64)],
    &[(u32, u32)],
    u32,
)] = &[
    (
        1,
        r#"{}"#,
        (40, 12),
        "none",
        0,
        &[],
        &[(0, 641994631)],
        2166136261,
    ),
    (
        1,
        r#"{}"#,
        (80, 24),
        "none",
        160,
        &[],
        &[(0, 209128793), (40, 3753372864), (160, 3360858889)],
        2166136261,
    ),
    (
        2,
        r#"{"pads":30,"drops":200}"#,
        (80, 24),
        "words",
        60,
        &[],
        &[(20, 3224981805), (60, 1033389497)],
        2166136261,
    ),
    (
        3,
        r#"{"koi":true,"pads":0,"drops":0}"#,
        (80, 24),
        "block",
        400,
        &[],
        &[(100, 2163253292), (400, 2409350098)],
        26018877,
    ),
    (
        4,
        r#"{"koi":true,"lotus":true,"rain":true,"moon":true,"adrift":true,"striders":true,"phase":0.5,"season":"spring"}"#,
        (80, 24),
        "words",
        200,
        &[(50, 81.0, 50.0), (60, 90.0, 52.0)],
        &[
            (0, 3781173323),
            (55, 2343385672),
            (120, 665763314),
            (200, 3643901239),
        ],
        2166136261,
    ),
    (
        5,
        r#"{"koi":true,"striders":true,"pads":0,"drops":0,"season":"autumn","adrift":true}"#,
        (120, 40),
        "none",
        120,
        &[(30, 100.0, 80.0)],
        &[(31, 1668809124), (90, 2850942080)],
        918731109,
    ),
    (
        6,
        r#"{"moon":true,"phase":0.3,"night":20.0,"current":2.0}"#,
        (80, 24),
        "none",
        100,
        &[],
        &[(0, 82287755), (50, 3556846932), (100, 2551279358)],
        2166136261,
    ),
    (
        7,
        r#"{"lotus":true,"pads":30,"moon":true,"phase":0.8,"season":"winter","adrift":true}"#,
        (200, 60),
        "words",
        20,
        &[],
        &[(20, 2394541277)],
        2166136261,
    ),
    (
        8,
        r#"{"koi":true,"rain":true}"#,
        (40, 12),
        "line",
        300,
        &[(100, 40.0, 20.0)],
        &[(150, 2174702057), (300, 1489396426)],
        883131962,
    ),
];

/// `Fly(Random(7), 80, 24)`: 800 steps of 1/8 s, then 60 of 1/20 s by the
/// pointer at (40, 12): the digest of the cells `x,y;`; its inks.
pub const FLY: (u32, (u8, u8)) = (2736063272, (116, 73));
