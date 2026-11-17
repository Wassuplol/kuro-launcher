import { getCurrentWindow } from "@tauri-apps/api/window";
import { Command } from "cmdk";
import {
  ArrowDown,
  ArrowUp,
  CornerDownLeft,
  FolderOpen,
  Home,
  Library,
  RotateCcw,
  Search,
  Settings,
  SlidersHorizontal,
  Users,
} from "lucide-react";
import { motion } from "motion/react";

import { PAGE_LABEL, PAGES } from "../app/router";
import { ENTER } from "../lib/motion";
import { useShell } from "../store/shell";

const ICONS = { home: Home, search: Search, library: Library, friends: Users };

const win = getCurrentWindow();

export function CommandPalette() {
  const open = useShell((state) => state.paletteOpen);
  const close = useShell((state) => state.closePalette);
  const go = useShell((state) => state.go);
  const openSettings = useShell((state) => state.openSettings);
  const openOnboarding = useShell((state) => state.openOnboarding);
  const openDataRoot = useShell((state) => state.openDataRoot);
  const settings = useShell((state) => state.settings);
  const update = useShell((state) => state.update);

  if (!open) return null;

  const instant = settings?.intro.instantStart ?? false;

  return (
    <div className="fixed inset-0 z-50">
      <div
        className="absolute inset-0 bg-page/45 backdrop-blur-[4px]"
        onClick={close}
        aria-hidden
      />

      <motion.div
        initial={{ opacity: 0, y: -6 }}
        animate={{ opacity: 1, y: 0 }}
        transition={ENTER}
        className="relative mx-auto mt-[14vh] w-[560px] max-w-[92vw] overflow-hidden rounded-modal border border-white/10 bg-panel shadow-2xl"
      >
        <Command label="Commands">
          <div className="flex items-center gap-2.5 border-b border-white/8 px-3.5">
            <Search size={15} strokeWidth={1.6} className="text-muted" />
            <Command.Input
              autoFocus
              placeholder="Type a command"
              className="h-11 w-full bg-transparent text-[13px] text-body placeholder:text-muted focus:outline-none"
            />
          </div>

          <Command.List className="max-h-[420px] overflow-y-auto p-1.5">
            <Command.Empty>Nothing matches that.</Command.Empty>

            <Command.Group heading="Go">
              {PAGES.map((page, index) => {
                const Icon = ICONS[page];
                return (
                  <Command.Item
                    key={page}
                    value={`go ${PAGE_LABEL[page]}`}
                    onSelect={() => go(page)}
                  >
                    <Icon size={15} strokeWidth={1.6} className="cmd-icon text-muted" />
                    <span>Go to {PAGE_LABEL[page]}</span>
                    <span className="num ml-auto text-[11px] text-muted">Ctrl {index + 1}</span>
                  </Command.Item>
                );
              })}
            </Command.Group>

            <Command.Group heading="App">
              <Command.Item value="open settings" onSelect={openSettings}>
                <Settings size={15} strokeWidth={1.6} className="cmd-icon text-muted" />
                <span>Open settings</span>
              </Command.Item>

              <Command.Item
                value="open data folder"
                onSelect={() => {
                  close();
                  void openDataRoot();
                }}
              >
                <FolderOpen size={15} strokeWidth={1.6} className="cmd-icon text-muted" />
                <span>Open the data folder</span>
              </Command.Item>

              <Command.Item
                value="run the first run questions again"
                onSelect={() => {
                  close();
                  openOnboarding();
                }}
              >
                <RotateCcw size={15} strokeWidth={1.6} className="cmd-icon text-muted" />
                <span>Run the first-run questions again</span>
              </Command.Item>

              <Command.Item
                value="turn the intro on or off"
                onSelect={() => {
                  void update((draft) => {
                    draft.intro.instantStart = !draft.intro.instantStart;
                  });
                  close();
                }}
              >
                <SlidersHorizontal size={15} strokeWidth={1.6} className="cmd-icon text-muted" />
                <span>{instant ? "Show the intro on start" : "Skip the intro on start"}</span>
              </Command.Item>

              <Command.Item
                value="quit"
                onSelect={() => {
                  void win.close();
                }}
              >
                <CornerDownLeft size={15} strokeWidth={1.6} className="cmd-icon text-muted" />
                <span>Quit Kuro</span>
              </Command.Item>
            </Command.Group>
          </Command.List>

          <footer className="flex items-center gap-4 border-t border-white/8 px-3.5 py-2 text-[11px] text-muted">
            <span className="flex items-center gap-1">
              <ArrowUp size={11} strokeWidth={1.6} />
              <ArrowDown size={11} strokeWidth={1.6} />
              move
            </span>
            <span className="flex items-center gap-1">
              <CornerDownLeft size={11} strokeWidth={1.6} />
              run
            </span>
            <span className="ml-auto">Esc closes</span>
          </footer>
        </Command>
      </motion.div>
    </div>
  );
}
