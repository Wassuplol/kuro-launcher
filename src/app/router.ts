export const PAGES = [
  "home",
  "stage",
  "search",
  "library",
  "sessions",
  "friends",
  "launch",
  "accounts",
  "flags",
  "versions",
] as const;

/**
 * Pages that need a target rather than a door: a game page is opened from a tile
 * with an id, so it is not in `PAGES` and gets no rail entry and no palette row.
 */
export const DETAIL_PAGES = ["game"] as const;

export type Page = (typeof PAGES)[number] | (typeof DETAIL_PAGES)[number];

export const PAGE_LABEL: Record<Page, string> = {
  home: "Home",
  stage: "Stage",
  search: "Search",
  library: "Library",
  sessions: "Sessions",
  friends: "Friends",
  launch: "Launch",
  accounts: "Accounts",
  flags: "Flags",
  versions: "Versions",
  game: "Game",
};

/**
 * The one place a page learns its hotkey.
 *
 * The rail prints it, the palette prints it and the window binds it, all from here. The
 * palette used to number the pages by their position in the list, which read `Ctrl 4`
 * beside Sessions, whose real key is `Ctrl 0`: a shortcut that is displayed and does not
 * work is worse than one that is not displayed.
 *
 * Stage has no key on purpose. Ctrl 1 through 8 are spoken for, and a signature surface
 * that costs one click in the rail does not need to evict one that people already know.
 */
export const PAGE_SHORTCUT: Partial<Record<Page, string>> = {
  home: "1",
  search: "2",
  library: "3",
  sessions: "0",
  friends: "4",
  launch: "5",
  accounts: "6",
  flags: "7",
  versions: "8",
};

export function pageFromHash(hash: string): Page {
  const name = hash.replace(/^#\/?/, "").split("?")[0] ?? "";
  const known: readonly string[] = [...PAGES, ...DETAIL_PAGES];
  return known.includes(name) ? (name as Page) : "home";
}

/** Keeps the address bar in step so a future deep link can open a page. */
export function reflect(page: Page): void {
  const next = `#/${page}`;
  if (window.location.hash !== next) {
    window.history.replaceState(null, "", next);
  }
}
