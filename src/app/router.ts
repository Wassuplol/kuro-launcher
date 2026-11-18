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

export type Page = (typeof PAGES)[number];

export const PAGE_LABEL: Record<Page, string> = {
  home: "Home",
  search: "Search",
  library: "Library",
  friends: "Friends",
  launch: "Launch",
  accounts: "Accounts",
  flags: "Flags",
  versions: "Versions",
};

export function pageFromHash(hash: string): Page {
  const name = hash.replace(/^#\/?/, "").split("?")[0] ?? "";
  return (PAGES as readonly string[]).includes(name) ? (name as Page) : "home";
}

/** Keeps the address bar in step so a future deep link can open a page. */
export function reflect(page: Page): void {
  const next = `#/${page}`;
  if (window.location.hash !== next) {
    window.history.replaceState(null, "", next);
  }
}
