/**
 * Links that leave the app.
 *
 * Three audiences, three links.
 *
 * - `kuro://` is for someone who has Kuro: it can name an account and, later, a flag
 *   profile, which Roblox's own links cannot.
 * - Roblox's `games/start` URL is for everyone else. It opens the exact server in the
 *   browser or hands it to the installed client, and needs nothing from this project.
 * - The short link on this repository's Pages site is the same invite with a card in
 *   front of it, so a person sees which game and which server before anything launches.
 */

/** Where the short links live. One constant, because a second copy would drift. */
export const INVITE_BASE = "https://wassuplol.github.io/kuro-launcher/join/";

/** Roblox's own join URL. Works for anyone, with or without this launcher. */
export function robloxJoin(place: number, server?: string | null): string {
  const base = `https://www.roblox.com/games/start?placeId=${place}`;
  return server ? `${base}&gameInstanceId=${encodeURIComponent(server)}` : base;
}

/** The short link: a card, then the same destination. */
export function inviteLink(place: number, server?: string | null): string {
  const url = new URL(INVITE_BASE);
  url.searchParams.set("p", String(place));
  if (server) url.searchParams.set("s", server);
  return url.toString();
}

/** Copies text, and answers whether it landed. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
