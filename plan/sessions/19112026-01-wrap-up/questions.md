# Questions for the owner

Each with what was done meanwhile. Most urgent first.

## To do by hand

1. **Commit the root `Cargo.lock`.** `cargo install --locked` and
   `just install` fail until then (wasmtime, landlock, seccompiler,
   rustix, signal-hook, serde_json in `ptyrc-store`).
2. **A root dev profile**: `[profile.dev.package.cranelift-codegen]
   opt-level = 3`. Without it a script run costs 20 s in a debug build
   and the dev tests 130 s.
3. **`plan/store-model.md`** was edited by an agent (uncommitted hunk
   seen mid-session, since committed in `a9a0023`): read the diff.

## The script interpreter

4. **Boa or QuickJS?** Boa: pure Rust, 6.2 MiB, 63 ms to set 12000
   cells. QuickJS needs a wasm libc (wasi-sdk) in ptyrc's release
   build; not measured. Meanwhile: Boa.
5. **`unsafe` outside `ptyrc-server`**: the two guest crates need it
   for the FFI. And wasmtime's `deserialize` (an on-disk cache of the
   compiled interpreter, saving 1 s a start) is `unsafe`: left out.
   Allow it in `ptyrc-box`?

## Permissions: to confirm

6. Names as built: `limit-memory`, `limit-time`; `keys-arrows` covers
   arrows, home/end/page/insert/delete, f1–f12.
7. Colours: `front` and `screen-wide` amber, unticked like red;
   `files` plain. Ticking a degree ticks those below.
8. The pop-up hides every app drawing while up; its mark: double
   border, bold white on blue, ` ptyrc ` below. A screen under 24x8
   refuses the start.
9. New error code `cancelled` (esc).
10. Lowered `find`: whole words (`my_fish` is not found). A wait on a
    pattern not held answers at once, `matched: false`. Below `shape`,
    idle is always true.
11. `session.info` below `text`: protocol, server, size, headless;
    host colours from `shape`.
12. `app.enrol`: 30 s to present the token. Native `ptyrc app` falls
    back to the plain exec when the driver does not know the request.
13. Limits and bands: store apps only. Native apps are not capped.
14. `surface = "widget"`: bare sprites are clipped to nothing. A frame
    that disappears: nothing shown until the app is moved.
15. The hide key: ctrl+t `h`; ctrl+t is now held up to 350 ms even
    under a grab or `--no-hotkey`.
16. A sprite behind the text is not the app's "own thing" for the
    pointer (else a full-screen canvas reads the pointer for free).
17. Still TBD in the plan: `text` red or plain; `clock`, `storage`
    default or plain; two colours or three.

## The store

18. Headless and a stranger's app: granted as declared (as now), or
    refused unless `--allow`?
19. Should the driver refuse a store app without the full OS profile
    (`PTYRC_PROFILE=full` exists), or start and log (as now)?
20. A Rust build: require namespaces and cgroups? Landlock alone does
    not stop UDP. cargo as proposed, or `rustc` alone?
21. `ptyrc-app` for a store app's lock: a crate on crates.io, or
    vendored (as the template does)?
22. Registry: where is it hosted? Who may say `tested` (proposed: a
    `MAINTAINERS` list, never the author)? A `note` by the tester?
    Signed tiers (proposed: later, SSH)?
23. The 2 ms bound for a colour per cell (3.0–3.3 ms): what is left is
    the repair fed back to the host model. Leave it?
24. macOS: the runner's profile is a no-op there.
