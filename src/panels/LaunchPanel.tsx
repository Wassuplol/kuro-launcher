import { useEffect, useRef, useState } from "react";

import { Badge, Row, Section } from "../components/Panel";
import { Tiles } from "../components/Tiles";
import { Toggle } from "../components/Toggle";
import { api, CommandError, failureDetail } from "../lib/ipc";
import { copyText, inviteLink } from "../lib/invite";
import type { Launched, LaunchReading } from "../lib/types";
import { useAccounts } from "../store/accounts";
import { useDiscovery } from "../store/discovery";
import { useLibrary } from "../store/library";
import { useShell } from "../store/shell";

/**
 * One destination, one account, one Play.
 *
 * The screen reads the destination before it starts anything, because a launch
 * that turns out to be the wrong server costs a game loading screen. The mode is
 * shown as words, not as a request name, and the account row says which session
 * the ticket will be minted from.
 */
export function LaunchBody() {
  const settings = useShell((state) => state.settings);
  const toggleMulti = useShell((state) => state.setMultiInstance);

  const accounts = useAccounts((state) => state.accounts);
  const load = useAccounts((state) => state.load);

  const [target, setTarget] = useState("");
  const [reading, setReading] = useState<LaunchReading | null>(null);
  const [parseProblem, setParseProblem] = useState<{ message: string; detail?: string } | null>(
    null,
  );
  const [launchProblem, setLaunchProblem] = useState<{ message: string; detail?: string } | null>(
    null,
  );
  const [working, setWorking] = useState(false);
  const [done, setDone] = useState<Launched | null>(null);
  const [account, setAccount] = useState<number | null>(null);
  const [running, setRunning] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void load();
    setDone(null);
    setLaunchProblem(null);

    // A tile on Home or in Search hands its place here rather than spawning a
    // client. The read below picks it up on the same pass, so the page arrives
    // already knowing what it would run.
    const pending = useShell.getState().pending;
    const pendingAccount = useShell.getState().pendingAccount;
    if (pending) {
      setTarget(pending);
      if (pendingAccount != null) setAccount(pendingAccount);
      useShell.getState().clearPending();
    }
    // The field is the point of this page, so it takes focus. Unless something
    // else has opened in the meantime: being pulled out of the palette by a
    // timer is the kind of small theft that makes an app feel broken.
    const timer = window.setTimeout(() => {
      if (!useShell.getState().paletteOpen) field.current?.focus();
    }, 120);
    return () => window.clearTimeout(timer);
  }, [load]);

  // Whether a game is already running decides what Play will do, so it is read
  // rather than guessed at from a setting.
  useEffect(() => {
    void api
      .launchState()
      .then((state) => setRunning(state.playerRunning))
      .catch(() => setRunning(false));
  }, [settings?.launch.multiInstance]);

  const list = accounts ?? [];
  const chosen = list.find((entry) => entry.userId === account) ?? list.find((entry) => entry.isDefault) ?? list[0];

  // Reading is a pure parse on the Rust side, and it also says which client
  // would run it, so it is debounced rather than fired per keystroke.
  useEffect(() => {
    const text = target.trim();
    if (!text) {
      setReading(null);
      setParseProblem(null);
      return;
    }
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          setReading(await api.launchRead(text));
          setParseProblem(null);
        } catch (value) {
          const failure = value instanceof CommandError ? value : null;
          setReading(null);
          setParseProblem({
            message: failure?.message ?? "That destination could not be read.",
            detail: failureDetail(failure),
          });
        }
      })();
    }, 220);
    return () => window.clearTimeout(timer);
  }, [target]);

  // The library's own record of what this machine ran, written here because the page
  // is the only place that knows which game a place id belonged to. A lost log line
  // never becomes a failed launch: the client is already starting.
  const record = (launched: Launched, accountId: number | null) => {
    if (!launched.place) return;
    const detail = useDiscovery.getState().detail;
    const known = detail && detail.place === launched.place ? detail : null;
    void useLibrary.getState().record({
      place: launched.place,
      universe: known?.universe ?? null,
      name: known?.name ?? null,
      mode: launched.mode,
      account: accountId,
    });
  };

  // A party link is the launch, written down: the place, and the account that would
  // open it. The exact server comes from a receipt, because a fresh launch does not
  // know its job id until the client has joined one.
  const copyInvite = async (place: number | null) => {
    if (place == null) return;
    if (await copyText(inviteLink(place, null))) {
      setCopied("invite");
      window.setTimeout(() => setCopied((current) => (current === "invite" ? null : current)), 2000);
    }
  };

  const copyParty = async (
    key: string,
    place: number | null,
    server: string | null,
    account: number | null,
  ) => {
    const url = await api.linkFor(place, server, account).catch(() => null);
    if (!url) return;
    if (await copyText(url)) {
      setCopied(key);
      window.setTimeout(() => setCopied((current) => (current === key ? null : current)), 2000);
    } else {
      // A clipboard that refuses is worth saying out loud, with the link in reach.
      setLaunchProblem({ message: `The clipboard refused the copy. The link is ${url}` });
    }
  };

  const play = async () => {
    const text = target.trim();
    if (!text || working) return;
    setWorking(true);
    setLaunchProblem(null);
    setDone(null);
    try {
      const launched = await api.launchStart(text, chosen?.userId ?? null);
      setDone(launched);
      record(launched, chosen?.userId ?? null);
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      setLaunchProblem({
        message: failure?.message ?? "The launch did not start.",
        detail: failureDetail(failure),
      });
    } finally {
      setWorking(false);
    }
  };

  const ready = Boolean(reading) && Boolean(chosen) && !working;

  return (
    <>
      <Tiles seed={target} accounts={list} onLaunched={record} />

      <Section title="Where to">
        <div className="px-3.5 py-3">
          <label htmlFor="target" className="text-[13px] text-body">
            A game link or a place id
          </label>
          <p className="mt-1 text-[11px] leading-[1.55] text-muted">
            Paste a link from the browser, an invite, or a deep link from Discord. A private
            server and one exact server both work, and the row below says which it found.
          </p>

          <input
            id="target"
            ref={field}
            value={target}
            onChange={(event) => setTarget(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void play();
            }}
            placeholder="https://www.roblox.com/games/…"
            autoComplete="off"
            spellCheck={false}
            className="ui mt-2.5 h-8 w-full rounded-control border border-white/10 bg-raised px-2.5 font-mono text-[12px] text-body placeholder:font-sans placeholder:text-muted focus:border-gold/60 focus:outline-none"
          />

          <div className="mt-2 min-h-[18px] text-[11px] leading-[1.55]">
            {reading ? (
              <span className="text-muted">
                <Badge tone="gold">{reading.modeLabel}</Badge>
                <span className="ml-2">{reading.summary}</span>
              </span>
            ) : parseProblem ? (
              <span className="text-danger">
                {parseProblem.message}
                {parseProblem.detail ? (
                  <span className="ml-1.5 text-muted">({parseProblem.detail})</span>
                ) : null}
              </span>
            ) : (
              <span className="text-muted">Nothing read yet.</span>
            )}
          </div>
        </div>
      </Section>

      <Section
        title="As who"
        aside={
          <button
            type="button"
            onClick={() => useShell.getState().go("accounts")}
            className="ui -mt-1 h-6 rounded-control px-2 text-[11px] text-muted hover:bg-white/5 hover:text-body"
          >
            {list.length > 0 ? "Manage" : "Add an account"}
          </button>
        }
      >
        {list.length === 0 ? (
          <Row
            label="No account yet"
            hint="A launch needs a session to mint a ticket from. Add one and come back."
          />
        ) : (
          list.map((entry) => (
            <button
              key={entry.userId}
              type="button"
              onClick={() => setAccount(entry.userId)}
              className="flex w-full items-center justify-between gap-3 border-b border-white/6 px-3.5 py-2.5 text-left last:border-b-0 hover:bg-white/4"
            >
              <span className="min-w-0">
                <span className="flex items-center gap-1.5">
                  <span className="truncate text-[13px] text-body">{entry.display}</span>
                  {chosen?.userId === entry.userId ? <Badge tone="gold">launching</Badge> : null}
                  {entry.health === "expired" ? <Badge tone="danger">expired</Badge> : null}
                </span>
                <span className="mt-0.5 block text-[11px] text-muted">
                  <span className="num">@{entry.name}</span>
                </span>
              </span>
              <span
                aria-hidden
                className={
                  chosen?.userId === entry.userId
                    ? "h-2 w-2 shrink-0 rounded-full bg-gold"
                    : "h-2 w-2 shrink-0 rounded-full border border-white/25"
                }
              />
            </button>
          ))
        )}
      </Section>

      <Section title="Start">
        <div className="px-3.5 py-3">
          {/* With no account there is nothing to launch as, so the primary action
              is adding one rather than a disabled Play beside a small link. */}
          {list.length === 0 ? (
            <button
              type="button"
              onClick={() => useShell.getState().go("accounts")}
              className="ui h-9 w-full rounded-control bg-gold text-[13px] font-semibold text-page hover:bg-[#f0cd7c]"
            >
              Add an account
            </button>
          ) : (
            <button
              type="button"
              onClick={() => void play()}
              disabled={!ready}
              className="ui h-9 w-full rounded-control bg-gold text-[13px] font-semibold text-page hover:bg-[#f0cd7c] disabled:opacity-40"
            >
              {working ? "Starting" : reading?.mode === "follow" ? "Play and follow" : "Play"}
            </button>
          )}

          {done ? (
            <div className="mt-2 flex items-start justify-between gap-2">
              <p className="text-[11px] leading-[1.55] text-body">
                {done.detail}
                <span className="ml-1 text-muted">
                  {done.client}
                  {done.version ? ` · ${done.version}` : ""}
                </span>
              </p>
              <button
                type="button"
                onClick={() => void copyParty("launch", done.place, null, chosen?.userId ?? null)}
                title="A kuro:// link that opens this game as this account"
                className="ui h-6 shrink-0 rounded-control border border-white/10 px-2 text-[11px] text-muted hover:border-white/20 hover:text-body"
              >
                {copied === "launch" ? "Copied" : "Copy party link"}
              </button>
              <button
                type="button"
                onClick={() => void copyInvite(done.place)}
                title="A short link anyone can open: this game, no launcher needed"
                className="ui h-6 shrink-0 rounded-control border border-white/10 px-2 text-[11px] text-muted hover:border-white/20 hover:text-body"
              >
                {copied === "invite" ? "Copied" : "Copy invite"}
              </button>
            </div>
          ) : (
            <p className="mt-2 text-[11px] leading-[1.55] text-muted">
              {chosen
                ? `Mints a ticket for ${chosen.display} and starts the client in place of the browser.`
                : "Add an account to launch."}
            </p>
          )}

          {launchProblem ? (
            <p className="mt-1.5 text-[11px] leading-[1.55] text-danger">
              {launchProblem.message}
              {launchProblem.detail ? (
                <span className="ml-1.5 text-muted">({launchProblem.detail})</span>
              ) : null}
            </p>
          ) : null}
        </div>
      </Section>

      <Section title="More than one game">
        <Row
          label="Allow a second client"
          hint="Roblox focuses the window it already has instead of starting another. This takes that lock first, so a second one can start beside it."
        >
          <Toggle
            label="Allow more than one game at a time"
            checked={settings?.launch.multiInstance ?? false}
            onCheckedChange={(checked) => void toggleMulti(checked)}
          />
        </Row>
        <div className="px-3.5 py-2.5 text-[11px] leading-[1.55] text-muted">
          Off by default. It changes what every Roblox program on this machine does, not only
          this one, so it stays a switch rather than a default.
        </div>
      </Section>

      {running && !settings?.launch.multiInstance ? (
        <p className="mt-4 text-[11px] leading-[1.55] text-muted">
          A game is running right now, so Play will say so instead of opening a second window.
        </p>
      ) : null}
    </>
  );
}
