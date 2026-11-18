import { create } from "zustand";

import { api, CommandError, detailOf } from "../lib/ipc";
import type { Roster } from "../lib/types";

/**
 * The friend list.
 *
 * Two budgets meet here and they are far apart: the list allows twenty calls a
 * minute and is cached on the Rust side for two, presence allows sixty and is asked
 * every time. That is why Refresh is cheap for presence and expensive for the list,
 * and why this store never polls: nothing here is on a timer.
 */
type SocialState = {
  roster: Roster | null;
  loading: boolean;
  problem: { message: string; detail?: string } | null;
  load: (force?: boolean) => Promise<void>;
};

export const useSocial = create<SocialState>()((set, get) => ({
  roster: null,
  loading: false,
  problem: null,

  load: async (force = false) => {
    if (get().loading) return;
    if (get().roster && !force) return;

    set({ loading: true });
    try {
      set({ roster: await api.friendsRoster(), loading: false, problem: null });
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      set({
        loading: false,
        problem: {
          message: failure?.message ?? "The friend list could not be read.",
          detail: failure ? detailOf(failure.code, failure.action) : undefined,
        },
      });
    }
  },
}));
