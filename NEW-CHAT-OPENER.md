Building MEM'S KURO LAUNCHER, a third-party Roblox launcher (Tauri 2 + Rust + React, MIT).

Read these two first, they override your assumptions:
- C:\Users\dodia\AppData\Local\hermes\projects\kuro-launcher\MASTER-PROMPT.md
- C:\Users\dodia\AppData\Local\hermes\projects\roblox-shell\SPEC-launcher-v1.md

A working earlier build lives in ...\projects\roblox-shell (proven: OAuth sign-in, friends with
pagination, DPAPI vault, per-host rate budgets). Read it for protocol facts, do not copy its
structure.

Start with M0 in section 14: repo scaffold, theme tokens, window shell, rail, four empty pages,
GitHub Actions, MIT licence. Then M1: intro, onboarding, mascot.

Environment, already verified, do not rediscover:
- Windows 11, terminal is bash. Build with `pnpm tauri build --no-bundle`.
- Stop any running kuro-shell.exe before building or linking fails with os error 5. If it will
  not die (started elevated), rename the exe and build beside it.
- The user launches the desktop app himself. If you need to click-test it, ask him to run it
  NOT as administrator, or background input will not reach it.
- Never spend money on a paid API (image generation) without asking first.
- His chat client does not inline MEDIA images: send file:///C:/... links instead.
- Prove every claim with tool output. Never fabricate or estimate a result.

Ready assets: roblox-shell\assets\mascot\final (welcome, at-rest, thinking, icon ladder and
.ico). The icon is already wired in the old project's src-tauri\icons, copy it over.

Owner is Mem. Casual, scannable bullets, maximum scope, end-to-end verification, ask before
anything irreversible.
