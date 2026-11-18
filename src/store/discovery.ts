import { toast } from "sonner";
import { create } from "zustand";

import { api, CommandError, detailOf } from "../lib/ipc";
import type { DiscoveryFeed } from "../lib/types";

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
};

export const useDiscovery = create<DiscoveryState>()((set, get) => ({
  feed: null,
  loading: false,
  problem: null,

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
}));
