import { toast } from "sonner";
import { create } from "zustand";

import { pageFromHash, reflect, type Page } from "../app/router";
import { api, CommandError, detailOf, failureDetail } from "../lib/ipc";
import type { AppInfo, Settings } from "../lib/types";

/**
 * The shell owns navigation and the two things that really are overlays: the
 * command palette and settings. Everything else is a page, so it is a place in
 * the window rather than a sheet over it.
 */
type ShellState = {
  ready: boolean;
  info: AppInfo | null;
  settings: Settings | null;
  bootFailed: boolean;
  page: Page;
  /** A place a grid sent to the Launch page, consumed there once. */
  pending: string | null;
  /** The account a link named, applied by the Launch page once. */
  pendingAccount: number | null;
  /** A link that should start without a second press, because it already was one. */
  pendingAuto: boolean;
  /** The game the detail page is showing, put there by the tile that opened it. */
  game: { universe: number; place: number } | null;
  paletteOpen: boolean;
  settingsOpen: boolean;
  onboardingOpen: boolean;
  /** The assistant dock is a column beside the page, not an overlay. */
  assistantOpen: boolean;

  boot: () => Promise<void>;
  go: (page: Page) => void;
  prefill: (target: string, account?: number | null, auto?: boolean) => void;
  clearAuto: () => void;
  /** Reads a `kuro://` link the backend is holding, and opens the Launch page on it. */
  readLink: () => Promise<void>;
  clearPending: () => void;
  openGame: (universe: number, place: number) => void;
  openPalette: () => void;
  closePalette: () => void;
  openSettings: () => void;
  toggleAssistant: (open?: boolean) => void;
  closeSettings: () => void;
  openOnboarding: () => void;
  closeOnboarding: () => void;
  setMultiInstance: (enabled: boolean) => Promise<void>;
  openDataRoot: () => Promise<void>;
  resetSettings: () => Promise<void>;
  update: (mutate: (draft: Settings) => void) => Promise<void>;
};

export const useShell = create<ShellState>()((set, get) => ({
  ready: false,
  info: null,
  settings: null,
  bootFailed: false,
  page: pageFromHash(window.location.hash),
  pending: null,
  pendingAccount: null,
  pendingAuto: false,
  game: null,
  paletteOpen: false,
  settingsOpen: false,
  onboardingOpen: false,
  assistantOpen: false,

  boot: async () => {
    try {
      const [info, settings] = await Promise.all([api.appInfo(), api.loadSettings()]);
      set({ info, settings, ready: true, bootFailed: Boolean(info.settingsError) });

      if (info.settingsError) {
        toast.error(info.settingsError.message, {
          description: detailOf(info.settingsError.code, info.settingsError.action),
        });
      }
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      toast.error(failure?.message ?? "The app could not read its own settings.", {
        description: failureDetail(failure),
      });
      set({ ready: true, bootFailed: true });
    }
  },

  // Going anywhere closes the overlays. It does not close a page, because a
  // page is what you are looking at, not something on top of it.
  go: (page) => {
    reflect(page);
    set({ page, paletteOpen: false, settingsOpen: false });
  },

  prefill: (target, account = null, auto = false) => {
    reflect("launch");
    set({
      pending: target,
      pendingAccount: account,
      pendingAuto: auto,
      page: "launch",
      paletteOpen: false,
      settingsOpen: false,
    });
  },

  readLink: async () => {
    const party = await api.linkTake().catch(() => null);
    if (party) {
      // A job id without a place is not a destination: the launch path parses a place and
      // an instance together, and nothing else here guesses.
      const target =
        party.place && party.server
          ? `roblox://placeId=${party.place}&gameInstanceId=${party.server}`
          : party.place
            ? String(party.place)
            : null;
      if (target) {
        get().prefill(target, party.account);
        return;
      }
    }

    // A Roblox link. It is a Play press that happened on roblox.com, so the default answer
    // is to act on it rather than to ask again; the switch in Settings decides otherwise.
    const raw = await api.nativeTake().catch(() => null);
    if (!raw) return;
    const auto = get().settings?.handover.auto !== false;
    get().prefill(raw, null, auto);
  },

  clearPending: () => set({ pending: null, pendingAccount: null }),
  clearAuto: () => set({ pendingAuto: false }),

  openGame: (universe, place) => {
    reflect("game");
    set({ game: { universe, place }, page: "game", paletteOpen: false, settingsOpen: false });
  },

  openPalette: () => set({ paletteOpen: true }),
  closePalette: () => set({ paletteOpen: false }),
  openSettings: () => set({ settingsOpen: true }),
  toggleAssistant: (open) =>
    set((state) => ({ assistantOpen: open === undefined ? !state.assistantOpen : open })),
  closeSettings: () => set({ settingsOpen: false }),
  openOnboarding: () => set({ onboardingOpen: true, settingsOpen: false, paletteOpen: false }),
  closeOnboarding: () => set({ onboardingOpen: false }),

  // The switch is not only a stored value: turning it on takes a named mutex,
  // which happens in Rust and can fail. Settings follow the result rather than
  // leading it, so the toggle cannot show a state the machine is not in.
  setMultiInstance: async (enabled) => {
    const current = get().settings;
    try {
      const answer = await api.launchMultiInstance(enabled);
      set({ settings: answer.settings });
      if (enabled && !answer.created) {
        toast.message("Another Roblox program already holds that lock.", {
          description: "The switch is on. It takes effect from that program's next start.",
        });
      }
    } catch (value) {
      set({ settings: current });
      const failure = value instanceof CommandError ? value : null;
      toast.error(failure?.message ?? "The multi-instance switch did not change.", {
        description: failureDetail(failure),
      });
    }
  },

  openDataRoot: async () => {
    try {
      await api.openDataRoot();
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      toast.error(failure?.message ?? "The data folder could not open.", {
        description: failureDetail(failure),
      });
    }
  },

  resetSettings: async () => {
    try {
      const fresh = await api.resetSettings();
      set({ settings: fresh, bootFailed: false });
      toast.success("Settings reset. The defaults are back.");
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      toast.error(failure?.message ?? "Settings could not be reset.", {
        description: failureDetail(failure),
      });
    }
  },

  update: async (mutate) => {
    const current = get().settings;
    if (!current) return;

    const next: Settings = structuredClone(current);
    mutate(next);
    set({ settings: next });

    try {
      const stored = await api.saveSettings(next);
      set({ settings: stored });
    } catch (value) {
      set({ settings: current });
      const failure = value instanceof CommandError ? value : null;
      toast.error(failure?.message ?? "Your settings did not save.", {
        description: failureDetail(failure),
      });
    }
  },
}));
