import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { create } from "zustand";

import { api, CommandError, detailOf, failureDetail } from "../lib/ipc";
import type {
  ClientKind,
  InstallMethod,
  InstallProgress,
  PrunePlan,
  SupplySnapshot,
  VerifyReport,
} from "../lib/types";

/** The event the install worker reports on. */
const PROGRESS_EVENT = "supply:progress";

let listening = false;

/**
 * The install worker reports whether or not the drawer is on screen, so the
 * listener is registered once for the life of the process. Dropping events while
 * the drawer is closed would leave a reopened drawer showing a stale bar.
 */
function ensureListening(set: (partial: Partial<VersionsState>) => void): void {
  if (listening) return;
  listening = true;
  void listen<InstallProgress>(PROGRESS_EVENT, (event) => {
    set({ progress: event.payload });
  });
}

function announce(value: unknown, fallback: string): void {
  const failure = value instanceof CommandError ? value : null;
  toast.error(failure?.message ?? fallback, { description: failureDetail(failure) });
}

type VersionsState = {
  snapshot: SupplySnapshot | null;
  loading: boolean;
  refreshing: boolean;
  /** The install in flight, if any. One at a time, because they share a disk. */
  installing: { kind: ClientKind; upload: string } | null;
  progress: InstallProgress | null;
  plan: PrunePlan | null;
  checks: Record<string, VerifyReport>;

  load: () => Promise<void>;
  refresh: () => Promise<void>;
  /** Reads the versions folder and the live version for the opening, quietly. */
  prepareInstalled: () => Promise<void>;
  prepareLive: () => Promise<void>;
  install: (kind: ClientKind, upload: string, method: InstallMethod) => Promise<void>;
  cancel: () => Promise<void>;
  pin: (folder: string | null) => Promise<void>;
  check: (folder: string) => Promise<void>;
  repair: (folder: string) => Promise<void>;
  remove: (folder: string) => Promise<void>;
  reviewPrune: () => Promise<void>;
  prune: () => Promise<void>;
  dismissPlan: () => void;
};

export const useVersions = create<VersionsState>()((set, get) => ({
  snapshot: null,
  loading: false,
  refreshing: false,
  installing: null,
  progress: null,
  plan: null,
  checks: {},

  prepareInstalled: async () => {
    ensureListening(set);
    try {
      const snapshot = await api.supplySnapshot();
      set({ snapshot });
    } catch {
      // The opening is not the place to argue with a machine. The versions
      // screen re-reads on open and says what is wrong there.
    }
  },

  prepareLive: async () => {
    ensureListening(set);
    try {
      const snapshot = await api.supplyRefresh();
      set({ snapshot });
    } catch {
      // Same: the Check button is the place to retry, with the reason shown.
    }
  },

  load: async () => {
    ensureListening(set);

    set({ loading: true });
    try {
      const snapshot = await api.supplySnapshot();
      set({ snapshot, loading: false });
    } catch (value) {
      set({ loading: false });
      announce(value, "The installed versions could not be read.");
    }
  },

  refresh: async () => {
    set({ refreshing: true });
    try {
      const snapshot = await api.supplyRefresh();
      set({ snapshot, refreshing: false });
    } catch (value) {
      set({ refreshing: false });
      announce(value, "Roblox's version service could not be reached.");
    }
  },

  install: async (kind, upload, method) => {
    set({ installing: { kind, upload }, progress: null });
    try {
      const snapshot = await api.supplyInstall(kind, upload, method);
      set({ snapshot, installing: null, progress: null });
      toast.success("Installed. Roblox will use it from the next launch.");
    } catch (value) {
      set({ installing: null, progress: null });
      announce(value, "The install did not finish.");
    }
  },

  cancel: async () => {
    try {
      await api.supplyCancel();
    } catch (value) {
      announce(value, "The install could not be stopped.");
    }
  },

  pin: async (folder) => {
    try {
      await api.supplyPin(folder);
      // A reviewed prune plan is a statement about the pins at the time it was
      // read. Changing a pin makes it stale, so it is dropped rather than left
      // on screen describing a decision that is no longer true.
      set({ plan: null });
      await get().load();
    } catch (value) {
      announce(value, "The pin did not save.");
    }
  },

  check: async (folder) => {
    try {
      const report = await api.supplyVerify(folder);
      set((state) => ({ checks: { ...state.checks, [folder]: report } }));
    } catch (value) {
      announce(value, "The check did not finish.");
    }
  },

  repair: async (folder) => {
    set({ installing: { kind: "player", upload: folder }, progress: null });
    try {
      const report = await api.supplyRepair(folder);
      set((state) => ({
        installing: null,
        progress: null,
        checks: { ...state.checks, [folder]: report },
      }));
      await get().load();
      toast.success("Repaired. Every file matches the hashes again.");
    } catch (value) {
      set({ installing: null, progress: null });
      announce(value, "The repair did not finish.");
    }
  },

  remove: async (folder) => {
    try {
      const snapshot = await api.supplyRemove(folder);
      set({ snapshot });
      toast.success("Removed. Roblox downloads it again if it needs it.");
    } catch (value) {
      announce(value, "That version was not removed.");
    }
  },

  reviewPrune: async () => {
    try {
      const plan = await api.supplyPrunePlan();
      set({ plan });
    } catch (value) {
      announce(value, "The versions folder could not be read.");
    }
  },

  prune: async () => {
    try {
      const result = await api.supplyPrune();
      set({ plan: null });
      await get().load();

      if (result.removed.length > 0) {
        toast.success(
          `Removed ${result.removed.length} ${
            result.removed.length === 1 ? "version" : "versions"
          }.`,
        );
      }
      for (const failure of result.failed) {
        toast.error(failure.message, { description: detailOf(failure.code) });
      }
    } catch (value) {
      announce(value, "The prune did not finish.");
    }
  },

  dismissPlan: () => set({ plan: null }),
}));
