// SPDX-License-Identifier: GPL-3.0-or-later
//! The registry's static page, from `index.json`: one HTML file, no
//! external resource, no script. Every text from a manifest or a pointer
//! is escaped.

use std::fmt::Write;

use crate::index::{Index, IndexApp};
use crate::manifest::Colour;
use crate::registry::Tier;

/// `&`, `<`, `>`, `"`, `'` as entities; control characters dropped.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = "\
body{font:15px/1.45 system-ui,sans-serif;max-width:52rem;margin:2rem auto;padding:0 1rem;color:#222;background:#fff}
h1{font-size:1.4rem}h2{font-size:1.1rem;margin:2rem 0 .2rem}
code,.mono{font-family:ui-monospace,monospace;font-size:.9em}
.origin{color:#555}.summary{margin:.3rem 0}
ul{margin:.3rem 0;padding-left:1.2rem}
.plain{color:inherit}.amber{color:#9a6700}.red{color:#c0161d;font-weight:600}
.untested{color:#c0161d}.tested{color:#9a6700}.approved{color:#1a7f37}.yanked{color:#c0161d;text-decoration:line-through}
table{border-collapse:collapse;font-size:.9em}td,th{padding:.1rem .6rem .1rem 0;text-align:left}
@media (prefers-color-scheme:dark){body{color:#ddd;background:#16181c}.origin{color:#aaa}.amber,.tested{color:#d4a72c}.red,.untested,.yanked{color:#ff7b72}.approved{color:#56d364}}
";

/// The page of `index`, titled `title`.
pub fn render(index: &Index, title: &str) -> String {
    let mut h = String::new();
    let t = escape(title);
    let _ = write!(
        h,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\n\
         <title>{t}</title>\n<style>\n{STYLE}</style>\n</head>\n<body>\n<h1>{t}</h1>\n\
         <p>{} apps. A tier is a word on a commit: <span class=\"untested\">untested</span> \
         (nobody vouches), <span class=\"tested\">tested</span> (it runs and looks fine), \
         <span class=\"approved\">approved</span> (that version was read). \
         <code>ptyrc store add ORIGIN</code> installs the best.</p>\n",
        index.apps.len()
    );
    for a in &index.apps {
        app(&mut h, a);
    }
    h.push_str("</body>\n</html>\n");
    h
}

fn tier_span(t: Tier) -> String {
    format!("<span class=\"{t}\">{t}</span>")
}

fn app(h: &mut String, a: &IndexApp) {
    let name = a.id.as_deref().unwrap_or(&a.origin);
    let version = a.version.as_deref().map(|v| format!(" {}", escape(v)));
    let _ = writeln!(
        h,
        "<section>\n<h2>{}{}</h2>\n<div class=\"origin mono\">{}</div>",
        escape(name),
        version.unwrap_or_default(),
        escape(&a.origin)
    );
    if a.yanked {
        h.push_str("<p class=\"yanked\">yanked</p>\n");
    }
    if let Some(s) = &a.summary {
        let _ = writeln!(h, "<p class=\"summary\">{}</p>", escape(s));
    }
    let mut facts = Vec::new();
    if let Some(t) = a.tier {
        facts.push(format!("tier {}", tier_span(t)));
    }
    if let Some(s) = &a.surface {
        facts.push(format!("surface {}", escape(s)));
    }
    if let Some(l) = &a.license {
        facts.push(escape(l));
    }
    if let Some(t) = &a.tape {
        facts.push(format!("tape <code>{}</code>", escape(t)));
    }
    if !facts.is_empty() {
        let _ = writeln!(h, "<p>{}</p>", facts.join(" · "));
    }
    let _ = writeln!(
        h,
        "<p><code>ptyrc store add {}</code></p>",
        escape(&a.origin)
    );
    if a.permissions.is_empty() {
        h.push_str("<p>Asks no permission.</p>\n");
    } else {
        h.push_str("<ul class=\"permissions\">\n");
        for p in &a.permissions {
            let class = match p.colour {
                Colour::Plain => "plain",
                Colour::Amber => "amber",
                Colour::Red => "red",
            };
            let words = match p.words.is_empty() {
                true => String::new(),
                false => format!(
                    " ({})",
                    p.words
                        .iter()
                        .map(|w| escape(w))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            let _ = writeln!(
                h,
                "<li class=\"{class}\"><code>{}</code>{words}: {}</li>",
                escape(&p.name),
                escape(&p.why)
            );
        }
        h.push_str("</ul>\n");
    }
    if !a.versions.is_empty() {
        h.push_str("<table>\n<tr><th>commit</th><th>version</th><th>tier</th><th>date</th><th>by</th></tr>\n");
        for v in &a.versions {
            let tier = match v.yanked {
                true => format!("<span class=\"yanked\">{}</span> yanked", v.tier),
                false => tier_span(v.tier),
            };
            let version = match (&v.version, &v.error) {
                (Some(x), _) => escape(x),
                (None, Some(e)) => format!(
                    "<span class=\"red\" title=\"{}\">unreadable</span>",
                    escape(e)
                ),
                (None, None) => String::new(),
            };
            let _ = writeln!(
                h,
                "<tr><td class=\"mono\" title=\"{c}\">{}</td><td>{version}</td><td>{tier}</td><td>{}</td><td>{}</td></tr>",
                escape(&v.commit.chars().take(12).collect::<String>()),
                escape(v.date.as_deref().unwrap_or("")),
                escape(v.by.as_deref().unwrap_or("")),
                c = escape(&v.commit),
            );
        }
        h.push_str("</table>\n");
    }
    h.push_str("</section>\n");
}
