# Build log

What was built, what was actually exercised, and what is still missing.

## M0: repo, CI, theme, shell (done)

**What exists**

- Tauri 2.12.6, Rust stable, React 19.3, TypeScript 7, Vite 8, Tailwind 4.
- Window shell: 44px title bar (wordmark, page name, commands button, window
  controls), 60px rail with four pages, one content area. `decorations: false`,
  dragging and double-click maximise handled by the title bar itself.
- Theme tokens in `src/theme`: the palette, radii and motion from the identity,
  with Inter Variable bundled locally (Latin and Cyrillic verified in the font's
  own cmap, weights 100 to 900).
- Four pages, each with an honest empty state. No dead controls: Search has no
  field until a search backend exists.
- Rust half: `error.rs` (the failure taxonomy), `settings.rs` (atomic writes into
  one data root, portable mode by a `portable` marker), `lib.rs` (five commands,
  single instance, window state).
- MIT licence, `deny.toml`, GitHub Actions: pnpm audit, `tsc --noEmit`,
  `cargo fmt --check`, `cargo clippy -D warnings`, `cargo deny check`, release
  build, exe uploaded as an artifact.

## M1: intro, onboarding, mascot, palette (done)

- Intro: 900ms GSAP timeline (gold hairline, wordmark, mascot), skippable from
  400ms, once per process, static frame under reduced motion, disabled by
  "Skip the intro".
- Onboarding: three questions, all skippable, nothing leaves the machine. The
  answers are stored in `settings.json` and change real things: the greeting, the
  charts named on Home, the advanced-mode default.
- Mascot: welcome pose in the intro, at-rest pose on empty states, never on a
  button or in a dialog.
- Command palette on Ctrl+K: nine commands that all do something, fuzzy match,
  keyboard driven.
- Settings panel: name, genres, intent, re-run the questions, forget the answers
  (two step), skip the intro, advanced mode, data folder, about.

## Verified by driving the release build

The release binary was driven over the WebView2 debug port, so every line below
is a real interaction against the shipped code, not a code reading.

| Check | Result |
|---|---|
| First run shows the intro, then question 1 | intro captured mid-animation, then "What should I call you?" |
| Typing a name and pressing Enter advances | value `Memo`, counter 3 of 24, then step 2 |
| Genre chips toggle and persist | picked RPG and Horror, Home reads "Popular games in RPG and Horror load from Roblox." |
| Intent changes a real default | "Building something" set `advanced: true` on disk |
| Finish closes onboarding and greets | "Good evening, Memo." |
| Settings survive a restart | killed and relaunched, no onboarding, name and charts still there |
| `settings.json` on disk | written with name, genres, intent, onboarded, advanced |
| Rail navigation | Home, Search, Library, Friends all switch |
| Ctrl+1 to 4 | each navigates to its page |
| `/` | navigates to Search (after the fix below) |
| Ctrl+K | opens the palette with nine commands |
| Escape | closes one layer per press (settings, then palette) |
| Palette command | "Open settings" opens the settings panel |
| Open folder | Explorer opened at `%APPDATA%\com.wassuplol.kuro` |
| Skip the intro toggle | wrote `instantStart: true` |
| Corrupt settings file | app still starts, toast reads `config_unreadable · Reset your answers`, settings panel shows the recovery banner |
| Reset settings | rewrote the defaults to disk, cleared the banner, confirmed with a toast |
| No horizontal overflow | 1180x760 viewport, scroll size identical |
| Bundled font active | `document.fonts.check('13px "Inter Variable"')` true |
| Shipped binary, no debug port | real screen capture renders the shell, port 9333 refuses |

## Bugs found by running it, and fixed

1. **The whole app rendered nothing.** Radix tooltips need a `TooltipProvider`;
   without one the first render threw and the window stayed black. Found by
   reading the console over CDP, not by reading the code.
2. **`/` did nothing.** `react-hotkeys-hook` matches physical key codes, so the
   binding had to be `slash`, not `/`.
3. **Two buttons labelled "Open"** in one settings panel. Renamed to "Start" and
   "Open folder".
4. **Palette list clipped its last row** under the footer, and the selected row
   was too quiet to follow with the eye. Taller list, stronger selected state.
5. **Empty states sat high with a dead lower half** and the text hugged the left
   edge while the mascot floated mid-page. Now centred as one group.
6. **A corrupt settings file stopped the app from starting**, so the recovery UI
   could never appear. Boot now falls back to defaults, records the failure in
   `AppInfo`, and the settings panel offers a reset that rewrites a clean file.

## Deviations from the master prompt, with reasons

- **No Lottie in the intro.** The stack locks Lottie for the entrance, but there
  is no authored Lottie file in the assets, and hand-writing one would be a fake
  asset pretending to be a designed animation. The entrance is a GSAP timeline
  instead, which is also in the locked stack for exactly this. When an authored
  Lottie exists, `lottie-react` drops into the same 900ms slot.
- **No `tauri-plugin-store`.** Settings are written by `settings.rs` with a
  temporary file and a rename, because portable mode needs a custom root and the
  file format is ours anyway.
- **No `tauri-plugin-opener`.** One command spawns Explorer directly, so the page
  never receives a filesystem path.
- **No `tauri-plugin-log` or tracing yet.** Nothing in M0 or M1 logs anything
  worth keeping. Both arrive with `logs.rs` in M4, where the redaction test lives.
- **`error.rs` is its own module.** The taxonomy outgrew `lib.rs` on the first
  command.

## The verification harness

`tools/cdp.mjs` drives the running build over the Chrome DevTools Protocol
(`eval`, `shot`, `type`, `key`). It needs the webview started with a debug port,
so verification used a temporary `additionalBrowserArgs` in `tauri.conf.json`;
that key is **not** in the shipped config, and the shipped binary was rebuilt and
re-checked with the port closed. `tools/capture.ps1` grabs the window from real
screen pixels, because `PrintWindow` returns stale frames for WebView2.

## Not done yet

- Search, discovery, servers: M6.
- Accounts, Quick Login, the vault: M3.
- Version discovery and launch: M2 and M4.
- The assistant and her thinking pose: M8.
- Installer, portable zip, updater: M9.
