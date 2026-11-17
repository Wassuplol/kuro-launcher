import { useEffect, useRef, useState } from "react";
import { Toaster } from "sonner";

import { CommandPalette } from "../panels/CommandPalette";
import { Intro } from "../panels/Intro";
import { Onboarding } from "../panels/Onboarding";
import { SettingsPanel } from "../panels/SettingsPanel";
import { useShell } from "../store/shell";
import { Shell } from "./Shell";

// The intro plays once per process, whatever the settings say.
let introPlayed = false;

export function App() {
  const ready = useShell((state) => state.ready);
  const settings = useShell((state) => state.settings);
  const onboardingOpen = useShell((state) => state.onboardingOpen);
  const boot = useShell((state) => state.boot);
  const openOnboarding = useShell((state) => state.openOnboarding);
  const closeOnboarding = useShell((state) => state.closeOnboarding);

  const [intro, setIntro] = useState(false);
  const decided = useRef(false);

  useEffect(() => {
    void boot();
  }, [boot]);

  useEffect(() => {
    if (!ready || decided.current) return;
    decided.current = true;

    if (settings?.intro.instantStart === true || introPlayed) {
      if (settings && !settings.identity.onboarded) openOnboarding();
      return;
    }

    introPlayed = true;
    setIntro(true);
  }, [ready, settings, openOnboarding]);

  return (
    <>
      {ready ? <Shell /> : null}

      {intro ? (
        <Intro
          onDone={() => {
            setIntro(false);
            if (settings && !settings.identity.onboarded) openOnboarding();
          }}
        />
      ) : null}

      {onboardingOpen ? <Onboarding onDone={closeOnboarding} /> : null}

      {ready ? <CommandPalette /> : null}
      {ready ? <SettingsPanel /> : null}

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
    </>
  );
}
