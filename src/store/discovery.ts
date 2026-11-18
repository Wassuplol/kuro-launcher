import { toast } from "sonner";
import { create } from "zustand";

import { api, CommandError, detailOf } from "../lib/ipc";
import type { DiscoveryFeed, GameDetail, GameServers } from "../lib/types";

/**
 * The home feed.
 *
 * One load per app run unless the person asks again: the feed is 250 KB and a
 * hydration call per fifty ids, and the answer only changes every few minutes.
 * A failure is kept as words on the page, because a launcher that silently shows
 * an empty shelf is indistinguishable from one with nothing to show.
 */
type DiscoveryState = {
  feed: DiscoveryFeed | null;
  loading: boolean;
  problem: { message: string; detail?: string } | null;
  load: (force?: boolean) => Promise<void>;

  detail: GameDetail | null;
  detailLoading: boolean;
  detailProblem: { message: string; detail?: string } | null;
  loadGame: (universe: number) => Promise<void>;

  servers: GameServers | null;
  serversLoading: boolean;
  serversProblem: { message: string; detail?: string } | null;
  loadServers: (place: number) => Promise<void>;
};

export const useDiscovery = create<DiscoveryState>()((set, get) => ({
  feed: null,
  loading: false,
  problem: null,
  detail: null,
  detailLoading: false,
  detailProblem: null,
  servers: null,
  serversLoading: false,
  serversProblem: null,

  load: async (force = false) => {
    if (get().loading) return;
    if (get().feed && !force) return;

    set({ loading: true });
    try {
      const feed = await api.discoveryHome();
      set({ feed, loading: false, problem: null });
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      const problem = {
        message: failure?.message ?? "The home feed could not be read.",
        detail: failure ? detailOf(failure.code, failure.action) : undefined,
      };
      set({ loading: false, problem });
      if (failure) toast.error(failure.message, { description: detailOf(failure.code, failure.action) });
    }
  },

  /** One game. Cheap enough to fetch again when the id changes, never on a timer. */
  loadGame: async (universe) => {
    if (get().detail?.universe === universe) return;
    set({ detailLoading: true, detailProblem: null, detail: null, servers: null });
    try {
      set({ detail: await api.discoveryGame(universe), detailLoading: false });
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      set({
        detailLoading: false,
        detailProblem: {
          message: failure?.message ?? "That game could not be read.",
          detail: failure ? detailOf(failure.code, failure.action) : undefined,
        },
      });
    }
  },

  /**
   * The server list. Three calls a minute, so this is only ever called from the
   * Servers tab, and an answer already in hand is not asked for again.
   */
  loadServers: async (place) => {
    if (get().servers?.place === place && !get().serversProblem) return;
    set({ serversLoading: true, serversProblem: null });
    try {
      set({ servers: await api.discoveryServers(place), serversLoading: false });
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      set({
        serversLoading: false,
        serversProblem: {
          message: failure?.message ?? "The server list could not be read.",
          detail: failure ? detailOf(failure.code, failure.action) : undefined,
        },
      });
    }
  },
}));
