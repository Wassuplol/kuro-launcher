import { useEffect, useState } from "react";

import { PanelFrame, Row, Section } from "../components/Panel";
import { Toggle } from "../components/Toggle";
import { cn } from "../lib/cn";
import type { Intent } from "../lib/types";
import { useShell } from "../store/shell";

const GENRES = [
  "Obby",
  "Simulator",
  "Tycoon",
  "Fighting",
  "RPG",
  "Horror",
  "Racing",
  "Building",
];

const INTENTS: { id: Intent; label: string }[] = [
  { id: "launch_faster", label: "Launch faster" },
  { id: "manage_accounts", label: "Manage accounts" },
  { id: "tune_performance", label: "Tune performance" },
  { id: "building_something", label: "Building something" },
];

export function SettingsPanel() {
  const open = useShell((state) => state.settingsOpen);
  const close = useShell((state) => state.closeSettings);
  const openOnboarding = useShell((state) => state.openOnboarding);
  const settings = useShell((state) => state.settings);
  const info = useShell((state) => state.info);
  const update = useShell((state) => state.update);
  const resetSettings = useShell((state) => state.resetSettings);
  const openVersions = useShell((state) => state.openVersions);
  const bootFailed = useShell((state) => state.bootFailed);

  const [name, setName] = useState(settings?.identity.name ?? "");
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    setName(settings?.identity.name ?? "");
  }, [settings?.identity.name]);

  if (!open) return null;

  const commitName = () => {
    if (!settings) return;
    const next = name.trim();
    if (next === settings.identity.name) return;
    void update((draft) => {
      draft.identity.name = next;
    });
  };

  const setIntent = (intent: Intent) => {
    void update((draft) => {
      draft.identity.intent = intent;
      draft.advanced = intent === "building_something";
    });
  };

  const forget = () => {
    void update((draft) => {
      draft.identity.name = "";
      draft.identity.genres = [];
      draft.identity.intent = null;
      draft.identity.onboarded = false;
    });
    setConfirming(false);
    openOnboarding();
  };

  return (
    <PanelFrame title="Settings" onClose={close}>
      {bootFailed ? (
            <div className="mt-4 rounded-card border border-danger/30 bg-danger-fill/10 p-3.5">
              <div className="text-[12px] font-semibold text-danger">
                Your settings file could not be read
              </div>
              <p className="mt-1 text-[12px] leading-[1.6] text-muted">
                The app started with defaults. Resetting writes a clean file; the old answers
                are gone either way.
              </p>
              <button
                type="button"
                onClick={() => void resetSettings()}
                className="ui mt-2.5 h-7 rounded-control border border-danger/40 px-2.5 text-[12px] text-danger hover:bg-danger-fill/20"
              >
                Reset settings
              </button>
            </div>
          ) : null}

          {settings ? (
            <>
              <Section title="You">
            <Row label="Name" hint="Shown in the greeting on Home.">
              <input
                value={name}
                maxLength={24}
                onChange={(event) => setName(event.target.value)}
                onBlur={commitName}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.currentTarget.blur();
                  }
                }}
                placeholder="you"
                aria-label="Your name"
                className="ui h-8 w-[160px] rounded-control border border-white/10 bg-raised px-2.5 text-[13px] text-body placeholder:text-muted focus:border-gold/60 focus:outline-none"
              />
            </Row>

            <div className="border-b border-white/6 px-3.5 py-3">
              <div className="text-[13px] text-body">What you play</div>
              <div className="mt-2 flex flex-wrap gap-1.5">
                {GENRES.map((genre) => {
                  const on = settings.identity.genres.includes(genre);
                  return (
                    <button
                      key={genre}
                      type="button"
                      aria-pressed={on}
                      onClick={() =>
                        void update((draft) => {
                          draft.identity.genres = on
                            ? draft.identity.genres.filter((item) => item !== genre)
                            : [...draft.identity.genres, genre];
                        })
                      }
                      className={cn(
                        "ui h-7 rounded-control border px-2.5 text-[12px]",
                        on
                          ? "border-gold/50 bg-gold/10 text-gold"
                          : "border-white/10 text-muted hover:border-white/20 hover:text-body",
                      )}
                    >
                      {genre}
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="border-b border-white/6 px-3.5 py-3">
              <div className="text-[13px] text-body">Why you are here</div>
              <div className="mt-2 flex flex-wrap gap-1.5" role="radiogroup" aria-label="Why you are here">
                {INTENTS.map((option) => {
                  const on = settings.identity.intent === option.id;
                  return (
                    <button
                      key={option.id}
                      type="button"
                      role="radio"
                      aria-checked={on}
                      onClick={() => setIntent(option.id)}
                      className={cn(
                        "ui h-7 rounded-control border px-2.5 text-[12px]",
                        on
                          ? "border-gold/50 bg-gold/10 text-gold"
                          : "border-white/10 text-muted hover:border-white/20 hover:text-body",
                      )}
                    >
                      {option.label}
                    </button>
                  );
                })}
              </div>
            </div>

            <Row label="Run the questions again" hint="The three first-run questions, nothing lost.">
              <button
                type="button"
                onClick={openOnboarding}
                className="ui h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-body hover:border-white/20 hover:bg-white/5"
              >
                Start
              </button>
            </Row>

            <Row
              label="Forget my answers"
              hint={
                confirming
                  ? "This clears your name, genres and reason. It cannot be undone."
                  : "Clears your answers and asks the three questions again."
              }
            >
              {confirming ? (
                <div className="flex items-center gap-1.5">
                  <button
                    type="button"
                    onClick={() => setConfirming(false)}
                    className="ui h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:text-body"
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    onClick={forget}
                    className="ui h-7 rounded-control bg-danger-fill px-2.5 text-[12px] font-semibold text-white hover:bg-[#b8373b]"
                  >
                    Forget
                  </button>
                </div>
              ) : (
                <button
                  type="button"
                  onClick={() => setConfirming(true)}
                  className="ui h-7 rounded-control px-2.5 text-[12px] text-danger hover:bg-danger-fill/15"
                >
                  Forget
                </button>
              )}
            </Row>
          </Section>

          <Section title="Startup">
            <Row label="Skip the intro" hint="Starts straight into the app.">
              <Toggle
                label="Skip the intro"
                checked={settings.intro.instantStart}
                onCheckedChange={(checked) =>
                  void update((draft) => {
                    draft.intro.instantStart = checked;
                  })
                }
              />
            </Row>
          </Section>

          <Section title="Roblox">
            <Row
              label="Versions"
              hint="What is installed, what Roblox is serving, and how to install or repair it."
            >
              <button
                type="button"
                onClick={openVersions}
                className="ui h-7 shrink-0 rounded-control border border-white/10 px-2.5 text-[12px] text-body hover:border-white/20 hover:bg-white/5"
              >
                Open
              </button>
            </Row>
          </Section>

          <Section title="Advanced">
            <Row label="Advanced mode" hint="Shows the data folder and build details.">
              <Toggle
                label="Advanced mode"
                checked={settings.advanced}
                onCheckedChange={(checked) =>
                  void update((draft) => {
                    draft.advanced = checked;
                  })
                }
              />
            </Row>

            {settings.advanced ? (
              <>
                <Row label="Data folder" hint={info?.dataRoot ?? ""}>
                  <OpenDataFolder />
                </Row>
                <Row label="Build" hint={info?.identifier ?? ""}>
                  <span className="num text-[12px] text-muted">{info?.version ?? ""}</span>
                </Row>
              </>
            ) : null}
          </Section>

          <Section title="About">
            <Row label={info?.name ?? "Mem's Kuro Launcher"} hint="Unofficial third-party launcher.">
              <span className="num text-[12px] text-muted">{info?.version ?? ""}</span>
            </Row>
            <Row label="Licence" hint="Source is open under the MIT licence." />
            <div className="px-3.5 py-3 text-[12px] leading-[1.6] text-muted">
              It never modifies the Roblox client, never touches Hyperion, and never sends a
              session anywhere except Roblox.
            </div>
          </Section>
            </>
          ) : (
        <div className="mt-5 rounded-card border border-white/8 bg-page/50 p-4">
              <h2 className="text-[13px] font-semibold text-body">
                Your settings could not be read
              </h2>
              <p className="mt-1.5 text-[12px] leading-[1.6] text-muted">
                The file is on disk but unreadable. Resetting writes the defaults back and asks
                the three questions again.
              </p>
              <button
                type="button"
                onClick={() => void resetSettings()}
                className="ui mt-3 h-8 rounded-control bg-gold px-3 text-[12px] font-semibold text-page hover:bg-[#f0cd7c]"
              >
                Reset settings
              </button>
            </div>
          )}
    </PanelFrame>
  );
}

function OpenDataFolder() {
  const openDataRoot = useShell((state) => state.openDataRoot);
  return (
    <button
      type="button"
      onClick={() => void openDataRoot()}
      className="ui h-7 shrink-0 rounded-control border border-white/10 px-2.5 text-[12px] text-body hover:border-white/20 hover:bg-white/5"
    >
      Open folder
    </button>
  );
}
