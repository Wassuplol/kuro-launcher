# Dependencies, and the job each one owns

Every package needs one sentence. If a package is not on this list, it is not in
the build. Packages from the locked stack that have not arrived yet are listed at
the bottom with the milestone that brings them.

## Runtime

| Package | Job |
|---|---|
| react, react-dom | rendering |
| @tauri-apps/api | the only bridge to the Rust half |
| @tanstack/react-query | server data, caching and retries for Roblox responses |
| zustand | shell state: current page, panels, settings |
| motion | React transitions for panels and pages |
| gsap | the intro timeline, and nothing else |
| lucide-react | the icon set, stroke 1.6 |
| cmdk | the command palette and its fuzzy match |
| sonner | toasts for failures that are not worth a screen |
| @radix-ui/react-tooltip | rail labels on hover |
| @radix-ui/react-switch | the one accessible toggle |
| react-hotkeys-hook | the keyboard map: Ctrl 1 to 4, Ctrl K, / and Escape |
| clsx, tailwind-merge | className composition without duplicate utilities |

## Development

| Package | Job |
|---|---|
| @tauri-apps/cli | `pnpm tauri build` and `dev` |
| vite | bundler |
| @vitejs/plugin-react | React fast refresh and JSX transform |
| tailwindcss, @tailwindcss/vite | utility CSS from the tokens in `src/theme` |
| typescript | type checking, run as part of `pnpm build` |
| @types/react, @types/react-dom, @types/node | types |

## Arrives with its milestone

| Package | Milestone |
|---|---|
| @tanstack/react-virtual | M6, lists above 100 rows |
| react-hook-form, zod | M3, the account import form |
| fuse.js | M6, local filtering of cached results |
| anime.js | M7, SVG presence rings |
| lottie-react | M1 follow-up, once an authored Lottie file exists |
| dayjs | M7, "last seen 2h ago" |
| @radix-ui/react-dialog, dropdown-menu, popover, tabs | the panel that first needs each one |

## One ignored advisory

`GHSA-fc5q-39xc-vjpf` (picomatch, high, ReDoS through repeated extglobs) has no
patched release. It arrives through vite, which is a devDependency, so it runs on
the build machine and nothing from it ships in the binary. The exception is
declared in `pnpm-workspace.yaml`, where pnpm 11 reads settings, and it is the
only one. Everything else still fails the audit at high.
