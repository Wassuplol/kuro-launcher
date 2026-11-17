import { getCurrentWindow } from "@tauri-apps/api/window";
import * as Tooltip from "@radix-ui/react-tooltip";
import { useHotkeys } from "react-hotkeys-hook";
import { Home, Library, Search, Settings, Users } from "lucide-react";
import { motion } from "motion/react";
import type { ComponentType } from "react";

import { RailItem } from "../components/RailItem";
import { WindowControls } from "../components/WindowControls";
import { cn } from "../lib/cn";
import { ENTER } from "../lib/motion";
import { useReducedMotion } from "../lib/useReducedMotion";
import { PAGE_LABEL, type Page } from "./router";
import { useShell } from "../store/shell";

import { FriendsScreen } from "../screens/Friends";
import { HomeScreen } from "../screens/Home";
import { LibraryScreen } from "../screens/Library";
import { SearchScreen } from "../screens/Search";

const win = getCurrentWindow();

const RAIL: { page: Page; icon: typeof Home; shortcut: string }[] = [
  { page: "home", icon: Home, shortcut: "Ctrl 1" },
  { page: "search", icon: Search, shortcut: "Ctrl 2" },
  { page: "library", icon: Library, shortcut: "Ctrl 3" },
  { page: "friends", icon: Users, shortcut: "Ctrl 4" },
];

const SCREENS: Record<Page, ComponentType> = {
  home: HomeScreen,
  search: SearchScreen,
  library: LibraryScreen,
  friends: FriendsScreen,
};

function TitleBar() {
  const page = useShell((state) => state.page);
  const openPalette = useShell((state) => state.openPalette);

  return (
    <header
      data-tauri-drag-region
      onDoubleClick={() => void win.toggleMaximize()}
      className="col-span-2 flex h-11 items-center gap-3 border-b border-white/8 bg-page pl-4 select-none"
    >
      <span data-tauri-drag-region className="text-[13px] font-medium tracking-[0.14em] text-body">
        KURO
      </span>
      <span aria-hidden data-tauri-drag-region className="h-3 w-px bg-white/12" />
      <span data-tauri-drag-region className="text-[12px] text-muted">
        {PAGE_LABEL[page]}
      </span>

      <div className="ml-auto flex h-full items-stretch">
        <button
          type="button"
          onClick={openPalette}
          className="ui my-auto mr-3 flex h-6 items-center gap-1.5 rounded-control border border-white/8 px-2 text-[11px] text-muted hover:border-white/16 hover:text-body"
        >
          Commands
          <span className="num text-muted">Ctrl K</span>
        </button>
        <WindowControls />
      </div>
    </header>
  );
}

function Rail() {
  const page = useShell((state) => state.page);
  const go = useShell((state) => state.go);
  const openSettings = useShell((state) => state.openSettings);
  const settingsOpen = useShell((state) => state.settingsOpen);

  return (
    <nav
      aria-label="Pages"
      className="row-start-2 flex flex-col items-center gap-1 border-r border-white/8 pt-3 pb-3"
    >
      <Tooltip.Provider delayDuration={400}>
        {RAIL.map((item) => (
          <RailItem
            key={item.page}
            label={PAGE_LABEL[item.page]}
            shortcut={item.shortcut}
            icon={item.icon}
            active={page === item.page}
            onClick={() => go(item.page)}
          />
        ))}

        <div className="mt-auto">
          <RailItem
            label="Settings"
            icon={Settings}
            active={settingsOpen}
            onClick={openSettings}
          />
        </div>
      </Tooltip.Provider>
    </nav>
  );
}

function PageOutlet() {
  const page = useShell((state) => state.page);
  const reduced = useReducedMotion();
  const Screen = SCREENS[page];

  return (
    <motion.div
      key={page}
      initial={reduced ? false : { opacity: 0, y: 4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={ENTER}
      className="h-full"
    >
      <Screen />
    </motion.div>
  );
}

export function Shell() {
  const go = useShell((state) => state.go);
  const openPalette = useShell((state) => state.openPalette);
  const closePalette = useShell((state) => state.closePalette);
  const closeSettings = useShell((state) => state.closeSettings);

  useHotkeys("ctrl+1", () => go("home"), { preventDefault: true });
  useHotkeys("ctrl+2", () => go("search"), { preventDefault: true });
  useHotkeys("ctrl+3", () => go("library"), { preventDefault: true });
  useHotkeys("ctrl+4", () => go("friends"), { preventDefault: true });
  // The hotkey library matches physical key codes, so the search key is
  // "slash", not "/". Layout independent, which is what a launcher wants.
  useHotkeys("slash", () => go("search"), { preventDefault: true });
  useHotkeys(
    "ctrl+k",
    () => {
      if (useShell.getState().paletteOpen) closePalette();
      else openPalette();
    },
    { preventDefault: true, enableOnFormTags: true },
  );

  // Escape closes one layer per press, never two.
  useHotkeys(
    "escape",
    () => {
      const state = useShell.getState();
      if (state.paletteOpen) closePalette();
      else if (state.settingsOpen) closeSettings();
      else if (state.versionsOpen) useShell.getState().closeVersions();
    },
    { enableOnFormTags: true },
  );

  return (
    <div
      className={cn(
        "grid h-full w-full grid-cols-[60px_1fr] grid-rows-[44px_1fr]",
        "overflow-hidden bg-page",
      )}
    >
      <TitleBar />
      <Rail />
      <main className="relative min-w-0 overflow-hidden">
        <PageOutlet />
      </main>
    </div>
  );
}
