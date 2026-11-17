import { toast } from "sonner";
import { create } from "zustand";

import { pageFromHash, reflect, type Page } from "../app/router";
import { api, CommandError } from "../lib/ipc";
import type { AppInfo, Settings } from "../lib/types";

type ShellState = {
  ready: boolean;
  info: AppInfo | null;
  settings: Settings | null;
  page: Page;
  paletteOpen: boolean;
  settingsOpen: boolean;
  onboardingOpen: boolean;

  boot: () => Promise<void>;
  go: (page: Page) => void;
  openPalette: () => void;
  closePalette: () => void;
  openSettings: () => void;
  closeSettings: () => void;
  openOnboarding: () => void;
  closeOnboarding: () => void;
  openDataRoot: () => Promise<void>;
  update: (mutate: (draft: Settings) => void) => Promise<void>;
};

export const useShell = create<ShellState>()((set, get) => ({
  ready: false,
  info: null,
  settings: null,
  page: pageFromHash(window.location.hash),
  paletteOpen: false,
  settingsOpen: false,
  onboardingOpen: false,

  boot: async () => {
    try {
      const [info, settings] = await Promise.all([api.appInfo(), api.loadSettings()]);
      set({ info, settings, ready: true });
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      toast.error(failure?.message ?? "The app could not read its own settings.");
      set({ ready: true });
    }
  },

  go: (page) => {
    reflect(page);
    set({ page, paletteOpen: false, settingsOpen: false });
  },

  openPalette: () => set({ paletteOpen: true }),
  closePalette: () => set({ paletteOpen: false }),
  openSettings: () => set({ settingsOpen: true }),
  closeSettings: () => set({ settingsOpen: false }),
  openOnboarding: () => set({ onboardingOpen: true, settingsOpen: false, paletteOpen: false }),
  closeOnboarding: () => set({ onboardingOpen: false }),

  openDataRoot: async () => {
    try {
      await api.openDataRoot();
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      toast.error(failure?.message ?? "The data folder could not open.");
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
        description: failure?.action,
      });
    }
  },
}));
