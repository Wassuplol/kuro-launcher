export const PAGES = [
  "home",
  "search",
  "library",
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
  search: "Search",
  library: "Library",
  friends: "Friends",
  launch: "Launch",
  accounts: "Accounts",
  flags: "Flags",
  versions: "Versions",
  game: "Game",
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
