// SPDX-License-Identifier: GPL-3.0-or-later
//! What the runner is told of the app's manifest (`--manifest JSON`,
//! read by the driver or `ptyrc dev test`, never by the guest): the
//! part the guest may know, handed to `init` under [`MEMBER`], and the
//! limits granted, applied up to [`CEILINGS`].

use crate::core::Limits;
use ptyrc_rules::grants::{Degree, Grants, Permission};
use serde_json::{Map, Value, json};
use std::time::Duration;

/// Member of the `init` JSON: `{id, version, permissions, granted,
/// limits, frame?}`. Wins over a member of the arguments with its name.
pub const MEMBER: &str = "ptyrc:manifest";

/// `[limits]` keys: `(name, default, ceiling, permission)`. The ceiling
/// holds whatever the manifest says; `ptyrc-store` (`LIMITS`) refuses a
/// manifest past it. Pinned by `tests/protocol.rs`.
pub const CEILINGS: &[(&str, u64, u64, &str)] = &[
    ("calls-per-second", 1000, 10_000, "limit-time"),
    ("canvas-screens", 4, 16, "limit-memory"),
    ("init-ms", 2000, 10_000, "limit-time"),
    ("log-mib", 1, 16, "limit-memory"),
    ("memory-mib", 64, 512, "limit-memory"),
    ("tick-ms", 50, 200, "limit-time"),
];

/// The permission that opens a `[limits]` key; `None`: not a key.
pub fn limit_permission(name: &str) -> Option<&'static str> {
    CEILINGS.iter().find(|l| l.0 == name).map(|l| l.3)
}

/// The manifest as given to the runner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Given {
    pub id: String,
    pub version: String,
    /// `(name, words)`, as declared; `words` empty but for `words`.
    pub permissions: Vec<(String, Vec<String>)>,
    /// Held now; `None`: not told (headless: from the grants).
    pub granted: Option<Vec<String>>,
    /// `[limits]` as declared: `(key, value)`.
    pub limits: Vec<(String, u64)>,
    /// The app's frame at the start, as the driver resolved it; `None`:
    /// not told.
    pub frame: Option<u64>,
}

impl Given {
    /// `{"id","version","permissions":[{"name","words"?}],"granted"?,
    /// "limits":{KEY: N},"frame"?}`.
    pub fn parse(text: &str) -> Result<Given, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("--manifest: {e}"))?;
        let s = |k: &str| v[k].as_str().map(str::to_owned).unwrap_or_default();
        let strings = |v: &Value| -> Vec<String> {
            let a = v.as_array().into_iter().flatten();
            a.filter_map(|w| w.as_str().map(str::to_owned)).collect()
        };
        let mut permissions = Vec::new();
        for p in v["permissions"].as_array().into_iter().flatten() {
            let name = p["name"]
                .as_str()
                .ok_or("--manifest: a permission has no name")?;
            permissions.push((name.to_owned(), strings(&p["words"])));
        }
        let mut limits = Vec::new();
        for (k, n) in v["limits"].as_object().into_iter().flatten() {
            if limit_permission(k).is_none() {
                return Err(format!("--manifest: limits: {k}: not a limit"));
            }
            let n = n
                .as_u64()
                .ok_or_else(|| format!("--manifest: limits: {k}: a number"))?;
            limits.push((k.clone(), n));
        }
        let frame = match v.get("frame") {
            None | Some(Value::Null) => None,
            Some(f) => Some(f.as_u64().ok_or("--manifest: frame: a frame id")?),
        };
        Ok(Given {
            id: s("id"),
            version: s("version"),
            permissions,
            granted: v.get("granted").map(strings),
            limits,
            frame,
        })
    }

    /// The limits granted: each key whose permission is in `granted`,
    /// up to its ceiling; the rest at the default.
    pub fn limits(&self, granted: &[String]) -> Limits {
        let mut l = Limits::default();
        for (key, value) in &self.limits {
            let Some(&(_, default, ceiling, permission)) = CEILINGS.iter().find(|c| c.0 == key)
            else {
                continue;
            };
            if !granted.iter().any(|g| g == permission) {
                continue;
            }
            let n = (*value).clamp(default, ceiling);
            match key.as_str() {
                "calls-per-second" => l.calls = n as u32,
                "canvas-screens" => l.canvas_screens = n as u32,
                "init-ms" => l.init = Duration::from_millis(n),
                "log-mib" => l.log = (n as usize) << 20,
                "memory-mib" => l.memory = (n as usize) << 20,
                "tick-ms" => l.late = Duration::from_millis(n),
                _ => {}
            }
        }
        l
    }

    /// The member of `init`: what the app may know of itself. `frame`:
    /// 0 below `layout` (a degree of the ladder: `shape`, `words`,
    /// `text` are above it), as `frame.get` and `draw.list` tell it.
    pub fn member(&self, granted: &[String]) -> Value {
        let permissions: Vec<Value> = (self.permissions.iter())
            .map(|(name, words)| match words.is_empty() {
                true => json!({"name": name}),
                false => json!({"name": name, "words": words}),
            })
            .collect();
        let limits: Map<String, Value> = (self.limits.iter())
            .filter(|(k, _)| limit_permission(k).is_some_and(|p| granted.iter().any(|g| g == p)))
            .map(|(k, v)| {
                let c = CEILINGS.iter().find(|c| c.0 == k).map_or(*v, |c| c.2);
                (k.clone(), json!((*v).min(c)))
            })
            .collect();
        let mut member = json!({
            "id": self.id,
            "version": self.version,
            "permissions": permissions,
            "granted": granted,
            "limits": limits,
        });
        if let Some(frame) = self.frame {
            let held = granted.iter().filter_map(|g| Permission::parse(g));
            let layout = Grants::of(held).degree() >= Degree::Layout;
            member["frame"] = json!(if layout { frame } else { 0 });
        }
        member
    }
}

/// `args` with the member set; anything else than an object counts as
/// `{}`.
pub fn with_member(args: Value, member: Value) -> Value {
    let mut out = match args {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    out.insert(MEMBER.to_owned(), member);
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granted_up_to_the_ceiling() {
        let g = Given::parse(
            r#"{"id":"a","version":"1","permissions":[{"name":"words","words":["fish"]}],
                "limits":{"memory-mib":4096,"tick-ms":100}}"#,
        )
        .unwrap();
        let none = g.limits(&[]);
        assert_eq!(none.memory, 64 << 20);
        let mem = g.limits(&["limit-memory".into()]);
        assert_eq!(mem.memory, 512 << 20);
        assert_eq!(mem.late, Duration::from_millis(50));
        let time = g.limits(&["limit-time".into()]);
        assert_eq!(time.late, Duration::from_millis(100));
        let m = g.member(&["words".into(), "limit-time".into()]);
        assert_eq!(m["permissions"][0]["words"], json!(["fish"]));
        assert_eq!(m["limits"], json!({"tick-ms": 100}));
        assert!(Given::parse(r#"{"limits":{"disk-mib":1}}"#).is_err());
        assert_eq!(m.get("frame"), None);
    }

    /// The frame told whole from `layout` up; else 0, the one frame.
    #[test]
    fn the_frame_at_the_start() {
        let g = Given::parse(r#"{"id":"a","version":"1","frame":3}"#).unwrap();
        assert_eq!(g.frame, Some(3));
        assert_eq!(g.member(&["layout".into()])["frame"], json!(3));
        for degree in ["shape", "words", "text"] {
            assert_eq!(g.member(&[degree.into()])["frame"], json!(3), "{degree}");
        }
        assert_eq!(g.member(&["clock".into()])["frame"], json!(0));
        assert_eq!(g.member(&[])["frame"], json!(0));
        assert!(Given::parse(r#"{"frame":"active"}"#).is_err());
        assert_eq!(Given::parse(r#"{"frame":null}"#).unwrap().frame, None);
    }
}
