# Mem's Kuro Launcher

A third-party front end for Roblox. It gets you from a cold PC to inside the right
server, on the right account, with the right settings, and keeps working across
client updates. It never modifies the Roblox client.

Status: early. The shell, the theme and the first-run sequence are built. Version
management, accounts and launching come next, in that order.

## What it is not

It is not a cheat, a client mod or a bypass. It does not patch, re-sign or inject
into `RobloxPlayerBeta.exe`, and it does not touch Hyperion. It does not offer
region changes, fake presence or hardware spoofing. The only thing it does to
Roblox is speak the same protocols the website speaks.

## Screens

Captured from this build, not from a mockup.

**Intro**

![Intro](shots/m1-intro.png)

**First run, three questions**

![First run](shots/m1-onboarding-1.png)

**Home, with nothing connected yet**

![Home](shots/m1-home.png)

**Command palette on Ctrl K**

![Command palette](shots/m1-palette.png)

## Build from source

Requirements: Windows 10 or 11, Node 22 or newer, pnpm 10 or newer, Rust stable,
and the Microsoft C++ build tools with the Windows SDK.

```
pnpm install
pnpm tauri dev          # development window
pnpm tauri build --no-bundle   # release executable
```

The release executable lands in `src-tauri/target/release/kuro-launcher.exe`.
A full installer build (`pnpm tauri build`) produces NSIS and MSI packages.

## Where things live

One data root holds everything the app writes: `%APPDATA%\com.wassuplol.kuro`.
Deleting that folder resets the app cleanly. Put a file named `portable` beside
the executable and the whole root moves next to the executable instead.

## Security model

The target, and what is already true.

True in this build:

- There is no network code at all. Nothing is fetched and nothing is sent.
- The page never receives a secret. The Rust half owns every privileged action.
- Settings are written through a temporary file and a rename, so a crash cannot
  leave a half-written file behind.

Not built yet, and enforced as each layer lands:

1. Session cookies sealed with Windows DPAPI, bound to the Windows user, stored
   as ciphertext only.
2. A cookie only ever sent to `roblox.com`, never to a proxy and never to a
   server of ours. There is no server of ours.
3. Logs and diagnostics that redact the home path, cookies, tickets and keys.
4. An updater that checks a signature made with this project's own key.

## Licence

MIT. See `LICENSE`.

Mem's Kuro Launcher is an unofficial third-party launcher. It is not affiliated
with, endorsed by or sponsored by Roblox Corporation. Roblox is a trademark of
Roblox Corporation.
