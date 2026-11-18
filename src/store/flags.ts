import { toast } from "sonner";
import { create } from "zustand";

import { api, CommandError, failureDetail } from "../lib/ipc";
import type { FlagHit, FlagRow, FlagStore, Placement } from "../lib/types";

function announce(value: unknown, fallback: string): void {
  const failure = value instanceof CommandError ? value : null;
  toast.error(failure?.message ?? fallback, { description: failureDetail(failure) });
}

/**
 * The flags drawer holds one workspace: the profiles, the scope chain, the file
 * a write would touch, and the lease if one is live.
 *
 * The one rule that shapes it: what is on screen is what is on disk. Every save
 * sends the whole set, and the view is reloaded from Rust after each change
 * rather than patched locally, so a failed write cannot leave the page claiming
 * something the machine does not have.
 */
type FlagsState = {
  view: import("../lib/types").FlagsView | null;
  loading: boolean;
  working: boolean;
  editing: string | null;
  results: FlagHit[];
  query: string;

  load: () => Promise<void>;
  search: (query: string) => Promise<void>;
  choose: (name: string) => void;
  createProfile: () => Promise<void>;
  deleteProfile: (name: string) => Promise<void>;
  setFlag: (key: string, value: unknown | null) => Promise<void>;
  setScope: (slot: "global" | "account" | "game", id: number | null, profile: string | null) => Promise<void>;
  place: (placement: Placement) => Promise<void>;
  refreshCatalog: () => Promise<void>;
  apply: (account: number | null, place: number | null) => Promise<void>;
  revert: () => Promise<void>;
  exportProfile: (name: string) => Promise<void>;
  importFile: (path: string) => Promise<void>;
};

function nameFor(store: FlagStore): string {
  let index = store.profiles.length + 1;
  let name = `Profile ${index}`;
  while (store.profiles.some((profile) => profile.name === name)) {
    index += 1;
    name = `Profile ${index}`;
  }
  return name;
}

export const useFlags = create<FlagsState>()((set, get) => ({
  view: null,
  loading: false,
  working: false,
  editing: null,
  results: [],
  query: "",

  load: async () => {
    set({ loading: true });
    try {
      const view = await api.flagsView();
      const current = get().editing;
      const stillThere = view.store.profiles.some((profile) => profile.name === current);
      set({
        view,
        loading: false,
        editing: stillThere
          ? current
          : view.store.profiles[0]?.name ?? null,
      });
    } catch (value) {
      set({ loading: false });
      announce(value, "Your flag profiles could not be read.");
    }
  },

  search: async (query) => {
    set({ query });
    try {
      set({ results: await api.flagsSearch(query, 40) });
    } catch (value) {
      set({ results: [] });
      announce(value, "The flag list could not be searched.");
    }
  },

  choose: (name) => set({ editing: name }),

  createProfile: async () => {
    const view = get().view;
    if (!view) return;
    const name = nameFor(view.store);
    try {
      const store = await api.flagsSaveProfile(name, {});
      set({ view: { ...view, store }, editing: name });
    } catch (value) {
      announce(value, "That profile could not be created.");
    }
  },

  deleteProfile: async (name) => {
    const view = get().view;
    if (!view) return;
    try {
      const store = await api.flagsDeleteProfile(name);
      set({
        view: { ...view, store },
        editing: get().editing === name ? store.profiles[0]?.name ?? null : get().editing,
      });
      toast.success(`${name} deleted.`);
    } catch (value) {
      announce(value, "That profile could not be deleted.");
    }
  },

  setFlag: async (key, value) => {
    const view = get().view;
    const editing = get().editing;
    if (!view || !editing) return;
    const profile = view.store.profiles.find((entry) => entry.name === editing);
    if (!profile) return;

    const next = { ...profile.values };
    if (value === null) delete next[key];
    else next[key] = value;

    set({ working: true });
    try {
      const store = await api.flagsSaveProfile(editing, next);
      set({ view: { ...view, store }, working: false });
    } catch (value) {
      set({ working: false });
      announce(value, "That flag was not saved.");
    }
  },

  setScope: async (slot, id, profile) => {
    const view = get().view;
    if (!view) return;
    try {
      const store = await api.flagsSetScope(slot, id, profile);
      set({ view: { ...view, store } });
    } catch (value) {
      announce(value, "That scope was not changed.");
    }
  },

  place: async (placement) => {
    const view = get().view;
    if (!view) return;
    set({ working: true });
    try {
      await api.flagsPlace(placement);
      const fresh = await api.flagsView();
      set({ view: fresh, working: false });
    } catch (value) {
      set({ working: false });
      announce(value, "That placement was not saved.");
    }
  },

  refreshCatalog: async () => {
    set({ working: true });
    try {
      const catalog = await api.flagsRefresh();
      const view = get().view;
      set({
        working: false,
        view: view ? { ...view, catalog } : view,
      });
      toast.success(`${Object.keys(catalog.values).length} flags read from Roblox.`);
    } catch (value) {
      set({ working: false });
      announce(value, "Roblox's flag list could not be fetched.");
    }
  },

  apply: async (account, place) => {
    set({ working: true });
    try {
      const lease = await api.flagsApply(account, place);
      const view = get().view;
      set({ working: false, view: view ? { ...view, lease } : view });
      toast.success(`${lease.keys} flags written.`);
    } catch (value) {
      set({ working: false });
      announce(value, "The flags were not written.");
    }
  },

  revert: async () => {
    set({ working: true });
    try {
      const restored = await api.flagsRevert();
      const view = get().view;
      set({ working: false, view: view ? { ...view, lease: null } : view });
      toast.success(restored ? "The previous settings are back." : "There was nothing to put back.");
    } catch (value) {
      set({ working: false });
      announce(value, "The previous settings could not be restored.");
    }
  },

  exportProfile: async (name) => {
    try {
      const path = await api.flagsExport(name);
      toast.success("Exported.", { description: path });
      const view = get().view;
      if (view) set({ view: { ...view, exports: (await api.flagsView()).exports } });
    } catch (value) {
      announce(value, "That profile could not be exported.");
    }
  },

  importFile: async (path) => {
    try {
      const imported = await api.flagsImport(path);
      const store = await api.flagsSaveProfile(imported.name, imported.values);
      const view = get().view;
      set({ view: view ? { ...view, store } : view, editing: imported.name });
      toast.success(`${Object.keys(imported.values).length} flags imported.`, {
        description: imported.refused.length
          ? `${imported.refused.length} refused: ${imported.refused.slice(0, 3).join(", ")}`
          : undefined,
      });
      void get().load();
    } catch (value) {
      announce(value, "That file could not be imported.");
    }
  },
}));

/** The rows a launch would actually write, which is what the summary counts. */
export async function resolveRows(account: number | null, place: number | null): Promise<FlagRow[]> {
  return api.flagsResolve(account, place);
}
