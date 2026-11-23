import { getCurrentWindow } from "@tauri-apps/api/window";
import * as Tooltip from "@radix-ui/react-tooltip";
import { useHotkeys } from "react-hotkeys-hook";
import {
  History,
  Home,
  Library,
  MessagesSquare,
  PackageCheck,
  Play,
  Search,
  Settings,
  SlidersHorizontal,
  UserRound,
  Users,
} from "lucide-react";
import { motion } from "motion/react";
import type { ComponentType } from "react";

import { AssistantPanel } from "../components/AssistantPanel";
import { RobuxPill } from "../components/RobuxPill";
import { RailItem } from "../components/RailItem";
import { WindowControls } from "../components/WindowControls";
import { cn } from "../lib/cn";
import { ENTER } from "../lib/motion";
import { useReducedMotion } from "../lib/useReducedMotion";
import { PAGE_LABEL, type Page } from "./router";
import { useShell } from "../store/shell";

import { AccountsScreen } from "../screens/Accounts";
import { FlagsScreen } from "../screens/Flags";
import { FriendsScreen } from "../screens/Friends";
import { GameScreen } from "../screens/Game";
import { HomeScreen } from "../screens/Home";
import { LaunchScreen } from "../screens/Launch";
import { LibraryScreen } from "../screens/Library";
import { SearchScreen } from "../screens/Search";
import { SessionsScreen } from "../screens/Sessions";
import { VersionsScreen } from "../screens/Versions";

const win = getCurrentWindow();

const RAIL: { page: Page; icon: typeof Home; shortcut: string }[] = [
  { page: "home", icon: Home, shortcut: "Ctrl 1" },
  { page: "search", icon: Search, shortcut: "Ctrl 2" },
  { page: "library", icon: Library, shortcut: "Ctrl 3" },
  { page: "sessions", icon: History, shortcut: "Ctrl 0" },
  { page: "friends", icon: Users, shortcut: "Ctrl 4" },
];

/**
 * The rail used to point at four pages and hide every working verb behind
 * Ctrl+K. These are the things that do something today, so they sit on the rail
 * beside the pages and keep their palette entries as the second door.
 */
const TOOLS: { page: Page; label: string; icon: typeof Home; shortcut: string }[] = [
  { page: "launch", label: "Launch a place", icon: Play, shortcut: "Ctrl 5" },
  { page: "accounts", label: "Accounts", icon: UserRound, shortcut: "Ctrl 6" },
  { page: "flags", label: "Flags", icon: SlidersHorizontal, shortcut: "Ctrl 7" },
  { page: "versions", label: "Roblox versions", icon: PackageCheck, shortcut: "Ctrl 8" },
];

const SCREENS: Record<Page, ComponentType> = {
  home: HomeScreen,
  search: SearchScreen,
  library: LibraryScreen,
  sessions: SessionsScreen,
  friends: FriendsScreen,
  launch: LaunchScreen,
  accounts: AccountsScreen,
  flags: FlagsScreen,
  versions: VersionsScreen,
  game: GameScreen,
};

function TitleBar() {
  const page = useShell((state) => state.page);
  const openPalette = useShell((state) => state.openPalette);

  // A frameless window has no frame to drag by, so dragging is asked for. The
  // capability that allows `startDragging` was not in the list, which is why the
  // window could not be moved at all: the call was refused and nothing said so.
  //
  // The three pixels of slack keep a double click a double click, because a click
  // that never moves never starts a drag and the maximise still reaches the header.
  const beginDrag = (event: React.MouseEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    if (target.closest("button, input, textarea, a")) return;

    const startX = event.clientX;
    const startY = event.clientY;

    const cleanup = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", cleanup);
    };

    const move = (cursor: MouseEvent) => {
      if (Math.abs(cursor.clientX - startX) < 3 && Math.abs(cursor.clientY - startY) < 3) {
        return;
      }
      cleanup();
      void (async () => {
        // Explorer restores a maximised window when you drag it, and a window that
        // will not move because it is maximised reads as broken.
        if (await win.isMaximized()) await win.unmaximize();
        await win.startDragging();
      })();
    };

    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", cleanup);
  };

  return (
    <header
      onMouseDown={beginDrag}
      onDoubleClick={() => void win.toggleMaximize()}
      className="col-span-full flex h-11 items-center gap-3 bg-gradient-to-b from-[#0d1017] to-page pl-4 select-none"
    >
      <span className="flex items-baseline gap-2">
        <span className="text-[12.5px] font-semibold tracking-[0.26em] text-body">KURO</span>
        <span aria-hidden className="mb-[3px] h-[5px] w-[5px] rounded-[1px] bg-gold/70" />
      </span>
      <span className="text-[12px] text-[var(--dim)]">{PAGE_LABEL[page]}</span>

      <div className="ml-auto flex h-full items-stretch">
        <RobuxPill />
        <button
          type="button"
          onClick={openPalette}
          className="ui my-auto mr-3 flex h-6 items-center gap-1.5 rounded-control px-2 text-[11px] text-[var(--dim)] hover:bg-white/5 hover:text-body"
        >
          Commands
          <span className="num text-[var(--dim)]">Ctrl K</span>
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
  const assistantOpen = useShell((state) => state.assistantOpen);
  const toggleAssistant = useShell((state) => state.toggleAssistant);

  return (
    <nav
      aria-label="Main"
      className="row-start-2 flex flex-col items-center gap-1 overflow-y-auto pt-3 pb-3"
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

        <span aria-hidden className="my-2 h-px w-5 shrink-0 bg-white/8" />

        {TOOLS.map((tool) => (
          <RailItem
            key={tool.page}
            label={tool.label}
            shortcut={tool.shortcut}
            icon={tool.icon}
            active={page === tool.page}
            onClick={() => go(tool.page)}
          />
        ))}

        <div className="mt-auto flex shrink-0 flex-col gap-1 pt-2">
          <RailItem
            label="Assistant"
            shortcut="Ctrl 9"
            icon={MessagesSquare}
            active={assistantOpen}
            onClick={() => toggleAssistant()}
          />
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
      initial={reduced ? false : { opacity: 0, y: 6 }}
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
  const assistantOpen = useShell((state) => state.assistantOpen);
  const toggleAssistant = useShell((state) => state.toggleAssistant);
  const openPalette = useShell((state) => state.openPalette);
  const closePalette = useShell((state) => state.closePalette);
  const closeSettings = useShell((state) => state.closeSettings);

  useHotkeys("ctrl+1", () => go("home"), { preventDefault: true });
  useHotkeys("ctrl+2", () => go("search"), { preventDefault: true });
  useHotkeys("ctrl+3", () => go("library"), { preventDefault: true });
  useHotkeys("ctrl+0", () => go("sessions"), { preventDefault: true });
  useHotkeys("ctrl+4", () => go("friends"), { preventDefault: true });
  useHotkeys("ctrl+5", () => go("launch"), { preventDefault: true });
  useHotkeys("ctrl+6", () => go("accounts"), { preventDefault: true });
  useHotkeys("ctrl+7", () => go("flags"), { preventDefault: true });
  useHotkeys("ctrl+8", () => go("versions"), { preventDefault: true });
  useHotkeys("ctrl+9", () => toggleAssistant(), { preventDefault: true, enableOnFormTags: true });
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
      // Only overlays close on Escape. A page is a place you are, not a layer
      // on top of one, so Escape never throws you out of it.
      if (state.paletteOpen) closePalette();
      else if (state.settingsOpen) closeSettings();
    },
    { enableOnFormTags: true },
  );

  return (
    <div
      className={cn(
        "grid h-full w-full grid-rows-[44px_1fr] overflow-hidden bg-page",
        assistantOpen ? "grid-cols-[64px_1fr_360px]" : "grid-cols-[64px_1fr]",
      )}
    >
      <TitleBar />
      <Rail />
      <main className="veil relative min-w-0 overflow-hidden">
        <PageOutlet />
      </main>
      {assistantOpen ? <AssistantPanel /> : null}
    </div>
  );
}
