import { useEffect, useRef, useState } from "react";
import { Toaster } from "sonner";

import { cn } from "../lib/cn";
import { api } from "../lib/ipc";
import { AccountsPanel } from "../panels/AccountsPanel";
import { CommandPalette } from "../panels/CommandPalette";
import { Onboarding } from "../panels/Onboarding";
import { Opening } from "../panels/Opening";
import { SettingsPanel } from "../panels/SettingsPanel";
import { VersionsPanel } from "../panels/VersionsPanel";
import { useShell } from "../store/shell";
import { Shell } from "./Shell";

// The opening plays once per process, whatever the settings say.
let played = false;

export function App() {
  const ready = useShell((state) => state.ready);
  const settings = useShell((state) => state.settings);
  const onboardingOpen = useShell((state) => state.onboardingOpen);
  const bootFailed = useShell((state) => state.bootFailed);
  const boot = useShell((state) => state.boot);
  const openOnboarding = useShell((state) => state.openOnboarding);
  const closeOnboarding = useShell((state) => state.closeOnboarding);

  /** The size the window is now, which the opening will grow back to. */
  const [target, setTarget] = useState<{ width: number; height: number } | null>(null);
  const [opening, setOpening] = useState(false);
  const [appearing, setAppearing] = useState(false);
  const decided = useRef(false);

  useEffect(() => {
    // The page reads the size the window-state plugin restored, remembers it as
    // the destination, then hands the window back at the opening scene's size.
    // A read during setup would get the config default instead, because the
    // plugin restores after the window is created.
    const width = Math.max(window.innerWidth, 920);
    const height = Math.max(window.innerHeight, 620);
    setTarget({ width, height });

    void api.windowOpen(520, 360).catch(() => undefined);
    void boot();
  }, [boot]);

  useEffect(() => {
    if (!ready || !target || decided.current) return;
    decided.current = true;

    const openQuestions = () => {
      if (settings && !settings.identity.onboarded && !bootFailed) openOnboarding();
    };

    // No opening: the window still has to be taken back to the size the person
    // left it at, so the settle runs either way.
    if (settings?.intro.instantStart === true || played) {
      void api.windowGrow(target.width, target.height, 0).catch(() => undefined);
      setAppearing(true);
      openQuestions();
      return;
    }

    played = true;
    setOpening(true);
  }, [ready, target, settings, bootFailed, openOnboarding]);

  const settle = () => {
    setOpening(false);
    setAppearing(true);
    if (settings && !settings.identity.onboarded && !bootFailed) openOnboarding();
  };

  return (
    <div className="relative h-full w-full overflow-hidden bg-page">
      {/*
        The real interface is laid out at its final size from the first frame,
        even while the window is still small. The window then opens around it, so
        what the growth reveals is a finished page rather than a reflow.
      */}
      {ready ? (
        <div
          className={cn(
            "transition-opacity",
            opening
              ? "pointer-events-none absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2"
              : "h-full w-full",
            appearing ? "opacity-100" : "opacity-0",
          )}
          style={
            opening && target
              ? {
                  width: target.width,
                  height: target.height,
                  transitionDuration: "560ms",
                  transitionDelay: "120ms",
                  transitionTimingFunction: "cubic-bezier(0.2,0.8,0.2,1)",
                }
              : { transitionDuration: "160ms" }
          }
        >
          <Shell />
        </div>
      ) : null}

      {opening && target ? <Opening target={target} onDone={settle} /> : null}

      {onboardingOpen ? <Onboarding onDone={closeOnboarding} /> : null}

      {ready ? <CommandPalette /> : null}
      {ready ? <SettingsPanel /> : null}
      {ready ? <VersionsPanel /> : null}
      {ready ? <AccountsPanel /> : null}

      <Toaster
        theme="dark"
        position="bottom-right"
        toastOptions={{
          classNames: {
            toast: "!bg-panel !border !border-white/10 !text-body !text-[13px]",
            description: "!text-muted",
          },
        }}
      />
    </div>
  );
}
