# Design direction

The owner's brief, transcribed from the master prompt, section 7. This file is the
authority for colour, type, density and motion. The antislop filter applies on top
of it; it does not replace it.

## Identity

A dark instrument panel, not a storefront. Quiet surfaces, one warm metal accent,
type that stays out of the way. Precision is where the fanciness comes from, not
more effects.

Dark only. There is no light theme, not even a token stub, because the window sits
beside a game client that is also dark.

> Reading this as: a desktop launcher shell for someone who plays Roblox, in an
> instrument-panel visual language, dial ENERGY 2 / RHYTHM 3 / MOTION 2.

## Dials

**ENERGY 2 / RHYTHM 3 / MOTION 2**

- Energy 2: present but not loud. One accent, nothing glows unless it is live.
- Rhythm 3: dense lists and tight rows, with deliberate breaks: the rail, the
  empty states, the intro.
- Motion 2: present everywhere, loud nowhere. Entrance and transition only.

## Palette

| Token | Value | Role | Contrast |
|---|---|---|---|
| page | `#0B0D12` | background | base |
| panel | `#12141A` | cards, panels | base |
| raised | `#1A1D25` | hover, inset surfaces | base |
| body | `#E8EAF0` | primary text | 16.16:1 on page |
| muted | `#A8B0C4` | secondary text | 8.95:1 on page |
| gold | `#E7C169` | action and live state only | 11.32:1 on page |
| deep gold | `#B8873C` | borders, icons | 6.08:1 on page |
| success | `#8FD9B6` | live, connected | 11.82:1 on page |
| danger text | `#F3A0A0` | destructive label | 9.58:1 on page |
| danger fill | `#A62F33` | destructive button, white text | 6.83:1 |

Gold means action or live state. Red means destructive. There is no third accent.

## Type

Inter Variable, Latin and Cyrillic, weights 400 and 600 only. Tabular figures
wherever a number changes. Wordmark: KURO in caps, 0.14em tracking, weight 500.
The full product name appears only in About and in Add/Remove Programs.

## Density

Rail 60, header 44, list row 40, tile 128 minimum. Radius 6 for controls, 10 for
cards, 14 for modals. Borders are 1px at 8 percent white. Separation is a border,
never a shadow.

## Motion

| Token | Value |
|---|---|
| micro | 120 ms, hover and press |
| enter | 180 ms, panels and rows |
| settle | 260 ms, modals and the assistant |
| intro budget | 900 ms total |
| easing | cubic-bezier(0.2, 0.8, 0.2, 1) |
| stagger | 24 ms, capped at 8 items |
| reduced motion | 0 ms, cross fade only, the system setting wins |

## Voice

Sentence case. Verb first. No exclamation marks. No "oops". Numbers plain. An
error is information, not an event. The mascot never appears next to a failure.

## Mascot, Aiko Kuro

Chibi anime, 3.2 heads, adult body language, office attire. Snow white ponytail
with a gold crown cuff, blunt hime bangs with a centre V, two side locks, gold cat
shaped eyes, charcoal suit, ivory silk, gold accents, crown lapel pin.

| Pose | Asset | Used by |
|---|---|---|
| welcome | aikowelcome.webp | intro, about |
| at rest | atrestaiko.webp | empty states |
| thinking | aikothinking.webp | assistant working |
| exit | none, by design | errors show nobody |

Never on a button, never inside a dialog, never more than once per screen, never
beside a destructive confirm.

## Decisions and their reasons

One line each, as the keystone rule asks.

- **Rail with a gold edge bar on the active page.** The bar marks real state, so
  it is a signal rather than a stripe. It costs 2px and replaces a highlight box.
- **Page name in the title bar, not repeated in the page body.** The header is
  already the page label, and repeating it wastes the densest row on screen.
- **No search field until search works.** A field that queries nothing is a dead
  control. The Search page explains the absence instead of imitating a search box.
- **Empty states say why they are empty and name what fills them.** A blank panel
  reads as a bug; a named cause reads as a product.
- **The mascot appears only on empty states and the intro.** She marks "nothing
  here yet" and "first run", so her presence always means the same thing.
- **One glass surface, the command palette.** It sits over content and needs
  separation; nothing else blurs.
- **A 1px gold hairline in the intro.** It draws the eye to the wordmark, which is
  the only thing happening during those 900 ms.
- **Gold fill on the primary button, page-coloured text.** Gold is the action
  colour, and dark text on gold clears AA by a wide margin.
- **Inter Variable, bundled locally.** Locked by the brief, and a local file means
  no network fetch at startup and no flash of a fallback face.
- **Borders instead of shadows.** Dense panels read better with hairlines; a
  shadow on every card would flatten the elevation language.
- **Lucide at stroke 1.6, 16 and 20px.** The brief locks it; one weight keeps the
  rail and inline icons from competing.
