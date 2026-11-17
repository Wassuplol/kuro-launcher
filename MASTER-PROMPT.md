# MEM'S KURO LAUNCHER, MASTER PROMPT

You are building a desktop launcher for Roblox: a third party front end that replaces the
website's job, not the client. Read this whole document before writing code. When something
here conflicts with your instinct, this document wins. When this document is silent, ask.

Repo: github.com/Wassuplol/kuro-launcher
Licence: MIT
App id: com.wassuplol.kuro
Owner: Mem (the user). He is not a developer. Explain in plain words, act, verify, report.

---

## 1. Mission

Get a person from a cold PC to inside the right server, on the right account, with the right
settings, and keep that working across Roblox client updates, without ever modifying the
client binary.

Parity with the official experience is the floor, not the goal. If a thing the official
launcher does is broken, we do it properly. If it does not exist, we consider adding it only
when it serves the sentence above.

---

## 2. Non-negotiables

These are not preferences. Violating one means the build is wrong even if it compiles.

1. Never patch, hex edit, re-sign or inject into `RobloxPlayerBeta.exe`, `RobloxStudio*.exe`
   or anything Hyperion protected. No DLL injection, no hooks.
2. Never invent a FastFlag. A flag exists for us only if the installed client's own string
   table contains the key. We expose what Roblox already reads, nothing else.
3. Never ship, log, print or transmit a session cookie or an authentication ticket outside
   the launch path. The cookie lives in the DPAPI vault and goes only to Roblox.
4. Never bypass region locks, fake presence, spoof hardware, or offer anything that reads as
   "get around Roblox". That is the line between a launcher and a cheat.
5. Never repack, mirror or host a Roblox client. Direct install fetches from Roblox's own
   CDN, verifies the Authenticode signature, and installs atomically.
6. Never hide a rate limit. If an endpoint allows 3 per minute, the number is on screen.
7. Never fabricate a result. If a tool call fails, report the failure. No invented output.

---

## 3. Stack, locked

| Layer | Choice |
|---|---|
| Shell | Tauri 2, Rust stable |
| Frontend | React 19, TypeScript, Vite |
| Styling | Tailwind 4 with CSS variables |
| Primitives | Radix (dialog, dropdown, tooltip, popover, tabs, switch) |
| Motion | `motion` for React transitions, GSAP for the intro timeline only, anime.js for SVG only. One owner per element, never two libraries on the same node |
| Icons | Lucide, stroke 1.6, 16 and 20 px |
| State | TanStack Query for server data, Zustand for shell state |
| Lists | TanStack Virtual above 100 rows |
| Forms | react-hook-form plus zod |
| Palette | cmdk |
| Toasts | sonner |
| Misc | clsx, tailwind-merge, fuse.js, react-hotkeys-hook, lottie-react, dayjs |
| HTTP | reqwest with per host budgets and a disk cache |
| Data | SQLite via rusqlite |
| Crypto | DPAPI via windows-rs |
| Process | sysinfo |
| Logs | tracing plus tracing-subscriber |
| Tauri plugins | deep-link, single-instance, updater, shell, dialog, fs, opener, notification, log, store, window-state |
| Packaging | NSIS installer, MSI, portable zip, all from one build |
| Updates | GitHub Releases signed with OUR minisign key. Never a boilerplate key |
| CI | GitHub Actions, Windows x64, cargo deny plus pnpm audit on every push |

Do not add a dependency that duplicates one above. Do not add a dependency for a 20 line
function. Every new package needs one sentence explaining the job it owns.

---

## 4. Architecture

One process, two halves, hard boundary between them.

Rust owns everything privileged: version manager, vault, HTTP, launch pipeline, flag writer,
protocol handler, log tailer, process tracking. React owns rendering and nothing privileged.
No secret ever reaches the frontend.

```
src-tauri/src/
  main.rs          entry
  lib.rs           command registration, AppState
  settings.rs      persisted user config
  vault.rs         DPAPI accounts
  versions.rs      client discovery, CDN resolve, install, pin, prune
  launch.rs        ticket, URI build, spawn, track, restore
  flags.rs         catalog from client, profiles, precedence, apply/restore
  rbx/             API clients: games, users, friends, presence, thumbnails
  social.rs        friends, presence, sniper, party
  assistant.rs     model calls and the tool surface
  protocols.rs     registry takeover and handback
  logs.rs          client log tailing, redaction
src/
  app/             shell, routing, layout
  screens/         home, search, library, friends, detail
  panels/          accounts, settings, assistant, palette, onboarding
  components/      primitives composed with tokens
  lib/             ipc, types, formatters, hooks
  theme/           tokens, dials, type scale
```

Every command returns `Result<T, LaunchError>` where `LaunchError` is the taxonomy below,
serialised with a `code` and a human `message`. The UI never renders a raw error string.

---

## 5. Capabilities

Seventy one capabilities across nine stages. Ship them in milestone order, not in numeric
order. Each one is testable, and the test is named with it.

### Stage 00 Install
L-01 per user install, no admin, no UAC ever.
L-02 portable mode, profile and logs beside the exe.
L-03 one data root holding accounts, profiles, cache, logs. Deleting it resets cleanly.
L-04 self update: signed, resumable, rollback to the previous build.
L-05 single instance plus tray. Second launch raises the first.
L-06 opt in start with Windows, registered as a task, not a Run key.

### Stage 01 Register
L-07 own `roblox-player:` handler, accepting the same string the official one does.
L-08 own `roblox:` handler for deep links from Discord, browser and phone shares.
L-09 own `roblox-studio:` and `roblox-studio-auth:`.
L-10 takeover and handback of every handler, one click each, no registry archaeology.
L-11 HKCU only, never HKLM.
L-12 re assert our handlers after a Roblox update rewrites them, and log it.

### Stage 02 Supply
L-13 discover installed clients in %LOCALAPPDATA%\Roblox\Versions, verify each is real.
L-14 resolve current version per channel from clientsettingscdn.
L-15 install a missing version, BOTH paths, user selectable: official bootstrap (default) and
       direct install from Roblox's CDN with signature verification and atomic rename.
       Any failure falls back to the bootstrap automatically.
L-16 official channels only. LIVE, zcanary, zintegration. No invented channels.
L-17 pin a version until told otherwise.
L-18 prune old versions, never the running one.
L-19 integrity check and repair by hash.
L-20 Studio detect, install, open a place or a local .rbxl.
L-21 update awareness, notify, never migrate mid session.

### Stage 03 Identity
L-22 Quick Login: create a token, show the 6 character code and the QR, poll status, exchange
       for a real cookie through auth.roblox.com/v2/login. Nothing typed into our app.
L-23 cookie import: paste once, normalise, verify against users/authenticated.
L-24 DPAPI vault, ciphertext on disk only, bound to the Windows user.
L-25 per account session health, does it still mint a ticket.
L-26 remove locally, with an honest note that the server session remains.
L-27 default and last used account.
L-28 unlimited accounts, switch at launch.

### Stage 04 Launch
L-29 accept a place id, universe id, website URL, roblox:// link, private link, or friend.
L-30 mint a ticket per account, including the CSRF 403 dance.
L-31 build the launch URI in exactly one of the five modes below.
L-32 spawn the right client version, detached, no console window.
L-33 preflight: client present, session valid, mutex free, no duplicate request.
L-34 idempotence: pressing Play twice does not spawn twice.

| Mode | request= | Params |
|---|---|---|
| normal | RequestGame | placeId |
| exact server | GameJob, verify against RequestGameJob | placeId, gameId |
| private server | RequestPrivateGame | placeId, accessCode |
| follow friend | RequestFollowUser | userId |
| any plus | plus linkCode | |

### Stage 05 Configure
L-35 catalog built by RE from the installed client, official keys only, with their defaults.
L-36 read the client's real files before showing any value.
L-37 per flag override, unset means client default.
L-38 named profiles, apply one or mix manually.
L-39 scope: account profile beats game profile beats global profile beats client default,
       resolved per flag, with provenance shown per row.
L-40 diff against default, one click back to vanilla.
L-41 import and export a profile as a file.
L-42 apply atomically with a snapshot, restore on exit, restore after crash, restore on next
       launcher start.
L-43 refuse to apply while a client is running, and say why.

### Stage 06 Run
L-44 track instances: pid, account, game, server, uptime.
L-45 hold `Local\ROBLOX_singletonMutex`, cap the count, release on close.
L-46 opt in Discord presence with the game and server name.
L-47 kill one, kill all, never orphan the mutex.
L-48 local playtime and history only, no telemetry.

### Stage 07 Recover
L-49 named failures, no bare stack trace in the UI.
L-50 retry paths: expired session offers Quick Login, missing client offers install.
L-51 open and filter the newest client log.
L-52 one click diagnostic bundle, redacted.

### Stage 08 Maintain
L-53 repair install: protocols, config, catalog.
L-54 separate resets: forget one account, clear profiles, wipe everything.
L-55 clean uninstall, no leftover handlers or tasks.
L-56 migrate from Bloxstrap and from our own previous build.

### Added after review
L-57 rejoin the last server, one click from the tray.
L-58 launch queue with cooldown and a visible ETA.
L-59 focus the existing instance instead of spawning a duplicate.
L-60 vanilla panic button: launch with zero overrides and restore every file we touched.
L-61 instance tags: each window labelled account plus game plus server.
L-62 crash detection under 10 seconds, log tail, offer rejoin.
L-63 snapshot client config before the first write, keep it forever.
L-64 version rollback when a new LIVE misbehaves.
L-65 guarded argument passthrough, documented, off by default.
L-66 degraded mode: offline, cached library still browsable.
L-67 accessibility floor: keyboard only, visible focus, labelled controls.
L-68 quiet startup: no splash window, no console, tray only.
L-69 intro sequence, rules in section 7.
L-70 mascot, rules in section 7.
L-71 friends pagination to exhaustion, no cap, whatever the total is.

---

## 6. The launch pipeline

States, in order. A failure at any state goes to Failed(cause) and ALWAYS restores config.

```
Idle
  Preflight        client present, session ok, mutex free, no duplicate
  Resolve          id, url, deeplink, private link or friend to a concrete plan
  Ticket           mint, CSRF retry once
  BuildUri         one of five modes, correct encoding
  ApplyConfig      snapshot first, then write
  Spawn            detached, timeout 15 s
  Running          tracked by pid
  Restore          unconditional, success or not
  Idle
```

Two rules that are not negotiable:
1. Config is never applied before a ticket exists. Never dirty a client for a launch that will
   fail at auth.
2. Restore runs on success, failure, cancel, crash, and the next launcher start.

### Failure taxonomy

Every failure is one of these, with a cause line and an offered action. The UI renders the
code plus the action, never a raw error.

| Code | Cause | Offered |
|---|---|---|
| client_missing | no usable version folder | install, choose another version |
| client_corrupt | hash mismatch | repair, install fresh |
| session_expired | ticket refused, cookie dead | Quick Login |
| session_flagged | 403 without a ticket, account restricted | state it plainly, no workaround |
| ticket_throttled | too many attempts | countdown |
| mutex_held | another instance owns the singleton | take over, or launch anyway |
| already_running | same account and place | focus it |
| queue_full | join queue at cap | position and ETA |
| flag_rejected | client ignored an unknown key | list the flags, offer vanilla |
| early_exit | process died under 10 s | log tail, rejoin, rollback |
| network | CDN unreachable | retry, degraded mode |
| update_in_flight | Roblox is installing | wait, or pin the old version |

### Flag precedence

account profile, then game profile, then global profile, then the client's own default.
Resolved per flag. Each row in the UI shows which layer supplied the value. No silent stacking.

Example: if the game profile sets the frame cap to 240 and the global sets 60, the result is
240 and the row reads "from game".

---

## 7. Identity

Dark only. No light theme, not even a stub. Fancy comes from precision, not from more effects.

| Token | Value | Role |
|---|---|---|
| page | #0B0D12 | background |
| panel | #12141A | cards, panels |
| raised | #1A1D25 | hover, inset surfaces |
| body | #E8EAF0 | primary text, 16.16:1 |
| muted | #A8B0C4 | secondary text, 8.95:1 |
| gold | #E7C169 | action and live state only, 11.32:1 |
| deep gold | #B8873C | borders, icons, 6.08:1 |
| success | #8FD9B6 | 11.82:1 |
| danger text | #F3A0A0 | 9.58:1 |
| danger fill | #A62F33 with white text | 6.83:1 |

Gold means action or live state. Red means destructive. There is no third accent.

Type: Inter Variable, Latin and Cyrillic, weights 400 and 600 only, tabular figures for counts.
Wordmark: KURO in caps, 0.14em tracking, weight 500. The full product name appears only in
About and in Add/Remove Programs.

Dials: ENERGY 2 (dark, one accent, nothing glows unless live), RHYTHM 3 (dense lists, tight
rows), MOTION 2 (present everywhere, loud nowhere).

| Motion token | Value |
|---|---|
| micro | 120 ms, hover and press |
| enter | 180 ms, panels and rows |
| settle | 260 ms, modals and the assistant |
| intro budget | 900 ms total |
| easing | cubic-bezier(0.2, 0.8, 0.2, 1) |
| stagger | 24 ms, capped at 8 items |
| reduced motion | everything 0 ms, cross fade only, system setting wins |

Density: rail 60, header 44, list row 40, tile 128 minimum, radius 6 controls and 10 cards and
14 modals, borders 1 px at 8 percent white. Separation is a border, never a shadow.

Voice: sentence case, verb first, no exclamation marks, no "oops", numbers plain. An error is
information, not an event. The mascot never appears next to a failure.

### Mascot, Aiko Kuro

Chibi anime, 3.2 heads, adult body language, office attire. Snow white ponytail with a gold
crown cuff, blunt hime bangs with a centre V, two side locks, gold cat shaped eyes, charcoal
suit, ivory silk, gold accents, crown lapel pin.

| Pose | Asset | Used by |
|---|---|---|
| welcome | aikowelcome.webp | intro, about |
| at rest | atrestaiko.webp | empty states |
| thinking | aikothinking.webp | assistant working |
| exit | none, by design | errors show nobody |

Never on a button, never inside a dialog, never more than once per screen, never beside a
destructive confirm.

### Intro and first run

Intro covers preflight that is already happening, and never adds delay: 1.2 s warm, 2.5 s cold,
skippable at 400 ms, once per process, no audio by default, Lottie for the entrance, static
frame under reduced motion, "instant start" disables it.

First run asks three questions, each changing something real, all skippable, nothing sent off
the machine:

| Question | Configures |
|---|---|
| What should I call you? | greeting, stored locally |
| What do you play most? | which public charts seed Home, nothing invented |
| Why are you here? | panel visibility and defaults: launch faster, manage accounts, tune performance, building something |

---

## 8. Screens

One rail, four pages, one overlay stack. No breadcrumbs, no nested tabs.

| Screen | Holds | Empty state |
|---|---|---|
| Home | Continue row (every account, tagged), then Popular | popular only |
| Search | query, count, virtualised grid, games and people | "type two characters" |
| Library | accounts strip, favourites, history | mascot at rest |
| Friends | add field, uncapped list, presence | mascot at rest |
| Detail | modal, deep linkable, hero, stats, actions | n/a |
| Servers | ping buckets, players, Join, refresh with its limit printed | "no public servers" |
| Accounts | health, Quick Login, import | "no account yet" |
| Settings | endpoint, keys, generation, flags, appearance | n/a |
| Assistant | chat, tool chips, model and tuning chips | three suggested actions |
| Palette | commands, fuzzy match | recent commands |

Keyboard: `/` search, `Ctrl+K` palette, `Ctrl+J` assistant, `Ctrl+1..4` pages, `Enter` launch,
`Backspace` leaves detail, `Esc` closes one layer per press.

Loading is a skeleton matching the real layout. Errors are named. Offline shows a banner and
keeps cached content browsable.

---

## 9. Social

| Feature | Rule |
|---|---|
| Friends | uncapped pagination, virtualised, total shown, online first then A-Z |
| Presence | In Kuro Launcher (our own, not yet in game), In <game> with player count, In Studio with place, Online, Offline last seen <time>, Activity hidden |
| Join | pins the exact job id, 1.4 s between attempts, visible ETA |
| Add | exact username, resolve, confirm, pending state shown |
| Remove | two step, never one stray click |
| Profile | in-app mini card with avatar, display, mutuals, Join, open on site |
| Party | several of your accounts into the SAME job id |
| Sniper | friends and non friends by user id or username, public servers only |

Rules: never send a request without a click, never show presence for someone not on your list,
never auto join on another's behalf, never hide a throttle. No blocking, reporting or DMs, those
live on the site where the consequence is visible. Privacy blocked presence says "this person
hides their activity", never a false negative.

---

## 10. Assistant

She is the mascot, the first run voice, and the assistant. One character, one register: calm,
precise, never gushing, never claims an action she did not take, admits uncertainty in one line.

| Tool | Does | Confirm |
|---|---|---|
| launch_game | start a place on a chosen account | no |
| join_friend | join a friend's exact server | no |
| snipe | find a user and join their server | no |
| search_games | query Roblox | no |
| install_version | download a channel or version | yes, states size and channel |
| set_flags | change client flags | yes, diff before, undo after |
| switch_account | change the default account | no |
| party_launch | several accounts into one job | yes, states the count |
| read_logs | tail the newest log, redacted | no |
| open_settings / open_panel | navigate | no |
| get_status | versions, sessions, instances, queue | no |

Hard rules: she never sees a cookie, a ticket or a token. Any write to the client is diff
first, undo after. No arbitrary shell, no arbitrary URL, the tool list is closed. Money and
deletion always confirm. Tool calls are visible chips, not prose claims.

Never sees: session cookies, authentication tickets, other accounts' sessions, your filesystem
outside our data root, your IP, your hardware ids.
Sees: game titles and ids, your display name, installed versions, your own flags, redacted logs.

Panel: tokens stream with a working stop, tool chips are expandable, the model chip opens the
picker and the choice persists, the tuning chip shows temperature and opens Settings. No key
configured shows one CTA to Settings, not a broken chat. Attachments (images, PDFs) ship in
v1.1 and only appear when the chosen model accepts them.

---

## 11. Data model

SQLite in the data root. WAL. Migrations are versioned and forward only.

```
accounts        user_id PK, name, display, thumb, cookie BLOB (DPAPI), added_at, last_ok_at
profiles        id PK, name, kind (global|game|account), target_id, created_at
flag_overrides  profile_id, key, value, updated_at, PRIMARY KEY(profile_id, key)
favourites      universe_id PK, place_id, name, added_at, source, remote_id, synced_at
recent          place_id PK, user_id, ts
servers_seen    place_id, server_id, players, ping, seen_at
cache           k PK, body, ts
instances       pid PK, user_id, place_id, server_id, started_at, ended_at
```

Favourites are local in v1 and built for sync: `source` is 'local' or 'roblox', `remote_id`
and `synced_at` stay null until we reconcile against the account's own favourites.

---

## 12. Security

| Concern | Rule |
|---|---|
| Cookie at rest | DPAPI, per Windows user, ciphertext only |
| Cookie in memory | only inside the launch path, never cloned into state that outlives it |
| Cookie in transit | roblox.com only, never a proxy, never our own server |
| Logs | redact home path, cookies, tickets, keys; write a test that proves it |
| Diagnostics | same redaction, human review before sharing |
| Frontend | no secrets, no tokens, no file paths outside our root |
| Dependencies | pinned, cargo deny and pnpm audit in CI |
| Updater | our key, signature verified before install, refused otherwise |

---

## 13. Distribution

| Lane | For | How |
|---|---|---|
| Installer | everyone | NSIS plus MSI, per user install, registers protocols, adds to Add/Remove |
| Portable | USB, no install | zip, profile beside the exe, protocols registered on request only |
| Source | people who change things | clone, pnpm install, pnpm tauri dev, documented in one page |

MIT licence, repo at github.com/Wassuplol/kuro-launcher, releases on GitHub with both artifacts,
CI building every tag, and the README treated as a showpiece: what it is, a screenshot of each
screen, three install paths, the security model in five lines, and the legal note that this is
an unofficial third party launcher that never modifies the client.

---

## 14. Milestones

Ship in this order. Do not start a milestone before the previous one is clickable.

| # | Milestone | Done when |
|---|---|---|
| M0 | repo, CI, licence, theme tokens, window shell, rail | app opens, empty pages, builds in Actions |
| M1 | intro, onboarding, mascot assets, palette | first run asks three questions and remembers them |
| M2 | version manager | discovers, resolves LIVE, installs via both paths, pins, prunes |
| M3 | vault and identity | Quick Login round trip, cookie import, health, remove |
| M4 | launch pipeline | all five modes against real games, restore verified, multi instance |
| M5 | flags | catalog from the client, profiles, precedence, diff, undo, crash safe |
| M6 | discovery | home, search with people, detail, servers, library, favourites |
| M7 | social | uncapped friends, presence, join, sniper, party |
| M8 | assistant | tools, confirmations, diff and undo, streaming |
| M9 | release | installer, portable, updater with our key, README, release notes |

---

## 15. Definition of done

A capability is done when all five are true, and you can show each one:

1. It runs in a release build, not just dev.
2. Its failure path was triggered on purpose and showed the right named reason.
3. It survives closing and reopening the app.
4. It survives a client update.
5. Its acceptance line in this document is literally satisfied, quoted in your report.

---

## 16. How to work

- Verify with tools, never from memory. If you claim a number, measure it.
- Finish the artifact. A plan is not a deliverable.
- When you break something, fix it and record the lesson where it loads next time.
- Ask before anything irreversible, anything that spends money, or anything on the user's
  account. Everything else, act.
- Report in short blocks: what changed, what you verified, what is left. No process narration.
- Apply the antislop rules to every screen and every line of copy. UI text, code comments and
  documentation all count.
- When a provider endpoint is rate limited or hostile, print the limit in the UI and cache
  aggressively. Never let the user think it is a bug.
