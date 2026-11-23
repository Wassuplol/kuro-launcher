import { useEffect, useState } from "react";

import { PanelFrame, Row, Section } from "../components/Panel";
import { AssistantSection } from "./AssistantSection";
import { Toggle } from "../components/Toggle";
import { api } from "../lib/ipc";
import { cn } from "../lib/cn";
import type { HandoverState, Intent, LinkState, PresenceView } from "../lib/types";
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
  const go = useShell((state) => state.go);
  const bootFailed = useShell((state) => state.bootFailed);

  const [link, setLink] = useState<LinkState | null>(null);
  const [linkBusy, setLinkBusy] = useState(false);
  const [handover, setHandover] = useState<HandoverState | null>(null);
  const [handoverBusy, setHandoverBusy] = useState(false);
  const [presence, setPresence] = useState<PresenceView | null>(null);
  const [appId, setAppId] = useState("");
  const [presenceBusy, setPresenceBusy] = useState(false);

  useEffect(() => {
    void api
      .linkState()
      .then(setLink)
      .catch(() => setLink(null));

    void api
      .handoverState()
      .then(setHandover)
      .catch(() => setHandover(null));
  }, []);

  useEffect(() => {
    void api
      .presenceView()
      .then((view) => {
        setPresence(view);
        setAppId(view.appId);
      })
      .catch(() => setPresence(null));
  }, []);

  const savePresence = async (enabled: boolean, id: string) => {
    setPresenceBusy(true);
    try {
      const view = await api.presenceSave(enabled, id);
      setPresence(view);
      setAppId(view.appId);
    } catch (value) {
      // The row keeps its last known state; the message says what was wrong with the id.
      setPresence((current) =>
        current
          ? {
              ...current,
              status: {
                ...current.status,
                error: value instanceof Error ? value.message : String(value),
              },
            }
          : current,
      );
    } finally {
      setPresenceBusy(false);
    }
  };

  const presenceLine = (() => {
    if (!presence) return "Reading Discord's connection.";
    if (!presence.enabled) return "Off. Nothing is sent to Discord.";
    if (!presence.discordRunning) return "On, but Discord is not running right now.";
    if (presence.status.error) return presence.status.error;
    if (presence.status.connected && presence.status.line) return `Showing: ${presence.status.line}`;
    if (presence.status.connected) return "Connected, waiting for a session.";
    return "On. Looking for Discord.";
  })();

  // The registry is what Windows does and the setting is what this app re-states at the
  // next start, so both move together: a toggle that said on while Windows still ran
  // Roblox's launcher would be the exact lie this section exists to avoid.
  const toggleHandover = async (on: boolean) => {
    setHandoverBusy(true);
    try {
      const next = on ? await api.handoverTake() : await api.handoverGiveBack();
      setHandover(next);
      await update((draft) => {
        draft.handover.takeover = on;
      });
    } catch {
      // The row keeps saying what it said: a failed write must not look like a state.
    } finally {
      setHandoverBusy(false);
    }
  };

  const toggleLink = async () => {
    setLinkBusy(true);
    try {
      setLink(link?.ours ? await api.linkUnregister() : await api.linkRegister());
    } catch {
      // The row keeps saying what it said: a failed write must not look like a state.
    } finally {
      setLinkBusy(false);
    }
  };

  const [name, setName] = useState(settings?.identity.name ?? "");
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    setName(settings?.identity.name ?? "");
  }, [settings?.identity.name]);

  // Both schemes, because one scheme taken and the other not is a half-replaced machine.
  const handoverOurs = handover?.schemes.every((scheme) => scheme.ours) ?? false;

  const currentHandler =
    handover?.schemes.find((scheme) => scheme.scheme === "roblox-player")?.current ??
    handover?.schemes.find((scheme) => scheme.current)?.current ??
    null;

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
                    className="ui h-7 rounded-control bg-danger-fill px-2.5 text-[12px] font-semibold text-white hover:bg-danger-hover"
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
            <Row label="Accounts" hint="Sessions saved on this machine, and whether they still work.">
              <button
                type="button"
                onClick={() => go("accounts")}
                className="ui h-7 shrink-0 rounded-control border border-white/10 px-2.5 text-[12px] text-body hover:border-white/20 hover:bg-white/5"
              >
                Open
              </button>
            </Row>

            <Row
              label="Versions"
              hint="What is installed, what Roblox is serving, and how to install or repair it."
            >
              <button
                type="button"
                onClick={() => go("versions")}
                className="ui h-7 shrink-0 rounded-control border border-white/10 px-2.5 text-[12px] text-body hover:border-white/20 hover:bg-white/5"
              >
                Open
              </button>
            </Row>
          </Section>

          <Section title="Links">
            <Row
              label="kuro:// links"
              hint={
                link?.ours
                  ? "Registered for this build. A link carries a place, one exact server and an account, so a friend's link lands where they were."
                  : "Points the kuro:// scheme at this app, so a party link opens here. It is written in your own user account, and the same button removes it."
              }
            >
              <button
                type="button"
                onClick={() => void toggleLink()}
                disabled={linkBusy || link === null}
                className="ui h-7 shrink-0 rounded-control border border-white/10 px-2.5 text-[12px] text-body hover:border-white/20 hover:bg-white/5 disabled:opacity-50"
              >
                {linkBusy ? "Writing" : link?.ours ? "Remove" : "Register"}
              </button>
            </Row>

            {link && !link.ours && link.command ? (
              <p className="px-3.5 py-2 text-[11px] leading-[1.55] text-muted">
                Windows currently opens kuro:// with{" "}
                <span className="num">{link.command}</span>. Registering replaces that with
                this build.
              </p>
            ) : null}
          </Section>

          <Section title="Roblox's own links">
            <Row
              label="Open Roblox's launch links here"
              hint={
                handoverOurs
                  ? "Windows opens both of Roblox's game schemes with this build. Roblox's launcher is not removed: turning this off puts it back exactly as it was."
                  : "A Play press on roblox.com asks Windows for Roblox's own launcher. Turning this on points roblox:// and roblox-player: at this build, so that press lands here instead."
              }
            >
              <Toggle
                label="Open Roblox's launch links in this build"
                checked={handoverOurs}
                disabled={handoverBusy || handover === null}
                onCheckedChange={(checked) => void toggleHandover(checked)}
              />
            </Row>

            <Row
              label="Start them as they arrive"
              hint="A Play press on roblox.com is already a press, so the launch begins without another click. Off means the link fills the Launch page and waits for you."
            >
              <Toggle
                label="Start Roblox links as they arrive"
                checked={settings?.handover.auto ?? true}
                disabled={!handoverOurs}
                onCheckedChange={(checked) =>
                  void update((draft) => {
                    draft.handover.auto = checked;
                  })
                }
              />
            </Row>

            {handover && !handoverOurs && currentHandler ? (
              <p className="px-3.5 py-2 text-[11px] leading-[1.55] text-muted">
                Windows opens roblox-player: with <span className="num">{currentHandler}</span>
                . Taking it turns every game link from the site into a launch from here.
              </p>
            ) : null}

            {handover?.note ? (
              <p className="px-3.5 py-2 text-[11px] leading-[1.55] text-muted">{handover.note}</p>
            ) : null}
          </Section>

          <Section title="Discord">
            <Row
              label="Show what I am playing"
              hint="Presence on your Discord profile, read from the same log the Sessions page reads. Off unless you turn it on, and nothing goes anywhere but Discord on this machine."
            >
              <Toggle
                label="Show what I am playing in Discord"
                checked={presence?.enabled ?? false}
                onCheckedChange={(checked) => void savePresence(checked, appId)}
              />
            </Row>

            <div className="px-3.5 py-2.5">
              <label htmlFor="discord-app" className="text-[11px] text-muted">
                Application id
              </label>
              <p className="mt-0.5 text-[11px] leading-[1.55] text-muted">
                The number from discord.com/developers under General Information. It decides the
                name and the icon on the card, so it has to be an application you made.
              </p>
              <div className="mt-2 flex gap-1.5">
                <input
                  id="discord-app"
                  value={appId}
                  onChange={(event) => setAppId(event.target.value)}
                  placeholder="1234567890123456789"
                  autoComplete="off"
                  spellCheck={false}
                  inputMode="numeric"
                  className="ui h-8 min-w-0 flex-1 rounded-control border border-white/10 bg-raised px-2.5 font-mono text-[12px] text-body placeholder:text-muted focus:border-gold/60 focus:outline-none"
                />
                <button
                  type="button"
                  onClick={() => void savePresence(presence?.enabled ?? false, appId)}
                  disabled={presenceBusy}
                  className="ui h-8 shrink-0 rounded-control border border-white/10 px-2.5 text-[12px] text-body hover:border-white/20 hover:bg-white/5 disabled:opacity-50"
                >
                  {presenceBusy ? "Saving" : "Save"}
                </button>
              </div>
              <p className="mt-2 text-[11px] leading-[1.55] text-body">{presenceLine}</p>
            </div>
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

          <AssistantSection />

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
                className="ui mt-3 h-8 rounded-control bg-gold px-3 text-[12px] font-semibold text-page hover:bg-gold-hover"
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
