import { toast } from "sonner";
import { create } from "zustand";

import { pageFromHash, reflect, type Page } from "../app/router";
import { api, CommandError, detailOf, failureDetail } from "../lib/ipc";
import type { AppInfo, Settings } from "../lib/types";

type ShellState = {
  ready: boolean;
  info: AppInfo | null;
  settings: Settings | null;
  bootFailed: boolean;
  page: Page;
  paletteOpen: boolean;
  settingsOpen: boolean;
  versionsOpen: boolean;
  accountsOpen: boolean;
  launchOpen: boolean;
  flagsOpen: boolean;
  onboardingOpen: boolean;

  boot: () => Promise<void>;
  go: (page: Page) => void;
  openPalette: () => void;
  closePalette: () => void;
  openSettings: () => void;
  closeSettings: () => void;
  openVersions: () => void;
  closeVersions: () => void;
  openAccounts: () => void;
  closeAccounts: () => void;
  openLaunch: () => void;
  closeLaunch: () => void;
  openFlags: () => void;
  closeFlags: () => void;
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
  paletteOpen: false,
  settingsOpen: false,
  versionsOpen: false,
  accountsOpen: false,
  launchOpen: false,
  flagsOpen: false,
  onboardingOpen: false,

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

  go: (page) => {
    reflect(page);
    set({
      page,
      paletteOpen: false,
      settingsOpen: false,
      versionsOpen: false,
      accountsOpen: false,
      launchOpen: false,
    });
  },

  openPalette: () => set({ paletteOpen: true }),
  closePalette: () => set({ paletteOpen: false }),
  openSettings: () =>
    set({
      settingsOpen: true,
      versionsOpen: false,
      accountsOpen: false,
      launchOpen: false,
      flagsOpen: false,
    }),
  closeSettings: () => set({ settingsOpen: false }),
  openVersions: () =>
    set({
      versionsOpen: true,
      settingsOpen: false,
      paletteOpen: false,
      accountsOpen: false,
      launchOpen: false,
    }),
  closeVersions: () => set({ versionsOpen: false }),
  openAccounts: () =>
    set({
      accountsOpen: true,
      settingsOpen: false,
      paletteOpen: false,
      versionsOpen: false,
      launchOpen: false,
    }),
  closeAccounts: () => set({ accountsOpen: false }),
  openLaunch: () =>
    set({
      launchOpen: true,
      settingsOpen: false,
      paletteOpen: false,
      versionsOpen: false,
      accountsOpen: false,
    }),
  closeLaunch: () => set({ launchOpen: false }),
  openFlags: () =>
    set({
      flagsOpen: true,
      settingsOpen: false,
      paletteOpen: false,
      versionsOpen: false,
      accountsOpen: false,
      launchOpen: false,
    }),
  closeFlags: () => set({ flagsOpen: false }),
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
