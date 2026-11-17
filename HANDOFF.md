# Handoff

Read this first in a new session. `BUILD_LOG.md` has the full history,
`DESIGN.md` is the authority for anything visual, `MASTER-PROMPT.md` is the brief.

## Where it lives

```
C:\Users\dodia\AppData\Local\hermes\projects\kuro-launcher
  src\                React: app shell, screens, panels, components, theme
  src-tauri\src\      Rust: lib.rs commands, error.rs taxonomy, settings.rs
  shots\              captures of verified states (m1-*, ship-*, final.png)
  tools\              cdp.mjs (drives the webview), capture.ps1 (real screen copy)
  docs\DEPENDENCIES.md one line per package, and the one ignored advisory
  deny.toml, pnpm-workspace.yaml (pnpm 11 settings live here, not package.json)
```

Repo: https://github.com/Wassuplol/kuro-launcher (public, MIT, default branch main).

## Where it stands

**M0 and M1 are shipped and verified.** CI is green on `c990b74`
(run `44136397501`): pnpm audit, tsc, cargo fmt, clippy -D warnings, cargo deny,
release build, exe artifact.

Built: window shell, theme tokens, four pages with honest empty states, intro,
the three first-run questions with local storage, mascot on empty states, Ctrl+K
palette with nine commands, settings panel, failure taxonomy, atomic settings
with a data root and portable mode, single instance, window state.

Not built: everything from M2 onward. There is no network code at all yet.

## Next: M2, the version manager

Discover clients in `%LOCALAPPDATA%\Roblox\Versions` and verify each is real,
resolve the LIVE version from `clientsettingscdn`, install via both paths
(official bootstrap default, direct CDN install with Authenticode verification
and atomic rename), pin a version, prune old ones. Studio detection is in the
same milestone.

## Commands

```
pnpm tauri build --no-bundle      # release exe; plain cargo build ships a localhost:1420 binary
pnpm exec tsc --noEmit            # types
cargo fmt && cargo clippy --all-targets -- -D warnings   # in src-tauri
```

**Stop kuro-launcher.exe before every build** or linking fails with os error 5:
`powershell -NoProfile -Command "Get-Process kuro-launcher | Stop-Process -Force"`.

## Verifying it

`tools/cdp.mjs` drives the running webview (`eval`, `shot`, `type`, `key`). It
needs a debug port: add `additionalBrowserArgs` with
`--remote-debugging-port=9333` to the window config, build, test, then **remove it
and rebuild** — the shipped config has no debug port and was re-checked with the
port closed. Full procedure and pitfalls: skill `tauri-app-verification`.

Two traps that cost time already: `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` alone is
ignored by Tauri, and the single-instance plugin makes a second launch exit
silently so the port never opens.

## Rules that are easy to break

- Antislop is `during` (global preference). Every UI, copy and comment ends with
  the Delivery Gate report.
- Never a dead control: Search has no field until a search backend exists.
- Failures reach the screen as the code plus the offered action, never a raw string.
- The mascot appears on the intro and on empty states only, never beside a failure.
- No Lottie yet: there is no authored file, so the intro is a GSAP timeline.
