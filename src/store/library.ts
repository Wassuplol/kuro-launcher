import { toast } from "sonner";
import { create } from "zustand";

import { api, CommandError, detailOf, failureDetail } from "../lib/ipc";
import type { GameCard, Played } from "../lib/types";

/**
 * The library: what an account starred on Roblox, and what this machine launched.
 *
 * Two sources, two honesty levels. Favourites are Roblox's own list, cached for a
 * couple of minutes because they change when somebody clicks a star. Recent plays
 * are this app's own record, because Roblox has no anonymous endpoint for them and
 * "what I played here" is a claim this app can actually support.
 *
 * The page keeps the account it read and refetches when that changes, so switching
 * accounts never shows a previous account's shelf under a new name.
 */
type LibraryState = {
  /** The account this shelf belongs to, so a mismatch is visible rather than silent. */
  account: number | null;

  favourites: GameCard[];
  next: string | null;
  note: string | null;
  fetched: number | null;
  cached: boolean;
  loading: boolean;
  problem: { message: string; detail?: string } | null;

  history: Played[];
  historyLoaded: boolean;
  historyBusy: boolean;

  load: (account: number | null, force?: boolean) => Promise<void>;
  more: () => Promise<void>;
  loadHistory: () => Promise<void>;
  record: (entry: Omit<Played, "at">) => Promise<void>;
  clearHistory: () => Promise<void>;
};

function failureOf(
  value: unknown,
  fallback: string,
): { message: string; detail?: string } {
  const failure = value instanceof CommandError ? value : null;
  return {
    message: failure?.message ?? fallback,
    detail: failure ? detailOf(failure.code, failure.action) : undefined,
  };
}

export const useLibrary = create<LibraryState>()((set, get) => ({
  account: null,
  favourites: [],
  next: null,
  note: null,
  fetched: null,
  cached: false,
  loading: false,
  problem: null,

  history: [],
  historyLoaded: false,
  historyBusy: false,

  load: async (account, force = false) => {
    if (account === null) {
      set({
        account: null,
        favourites: [],
        next: null,
        note: null,
        fetched: null,
        problem: null,
        loading: false,
      });
      return;
    }

    // A different account is a different shelf, so the guard is per account and a
    // refresh is explicit.
    if (!force && get().account === account && get().favourites.length > 0) return;
    if (get().loading) return;

    const changed = get().account !== account;
    set({
      account,
      loading: true,
      problem: null,
      ...(changed ? { favourites: [], next: null, note: null, fetched: null } : {}),
    });

    try {
      const shelf = await api.libraryFavorites(account, null);
      set({
        favourites: shelf.games,
        next: shelf.next,
        note: shelf.note ?? null,
        fetched: shelf.fetched,
        cached: shelf.cached,
        loading: false,
      });
    } catch (value) {
      set({ loading: false, problem: failureOf(value, "Your favourites could not be read.") });
    }
  },

  more: async () => {
    const { account, next, favourites, loading } = get();
    if (!account || !next || loading) return;

    set({ loading: true });
    try {
      const shelf = await api.libraryFavorites(account, next);
      const seen = new Set(favourites.map((game) => game.universe));
      set({
        favourites: [
          ...favourites,
          ...shelf.games.filter((game) => !seen.has(game.universe)),
        ],
        next: shelf.next,
        note: shelf.note ?? get().note,
        loading: false,
      });
    } catch (value) {
      set({ loading: false, problem: failureOf(value, "The next page did not arrive.") });
    }
  },

  loadHistory: async () => {
    if (get().historyBusy) return;
    set({ historyBusy: true });
    try {
      const log = await api.libraryRecent();
      set({ history: log.entries, historyLoaded: true, historyBusy: false });
    } catch {
      // A history that cannot be read is not a broken library: the shelf above it
      // is the part that matters, so this stays quiet and empty.
      set({ historyBusy: false, historyLoaded: true });
    }
  },

  record: async (entry) => {
    try {
      const log = await api.libraryRecord(entry);
      set({ history: log.entries, historyLoaded: true });
    } catch (value) {
      // Never let a lost log line turn a launch into a failure on screen: the game
      // is already starting.
      const failure = value instanceof CommandError ? value : null;
      if (failure) toast.error(failure.message, { description: failureDetail(failure) });
    }
  },

  clearHistory: async () => {
    try {
      const log = await api.libraryClear();
      set({ history: log.entries });
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      toast.error(failure?.message ?? "The history could not be cleared.", {
        description: failureDetail(failure),
      });
    }
  },
}));
