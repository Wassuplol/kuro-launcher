import { useEffect, useMemo, useState } from "react";

import { Badge, PanelFrame, Row, Section } from "../components/Panel";
import { Toggle } from "../components/Toggle";
import { cn } from "../lib/cn";
import { ago } from "../lib/format";
import type { FlagKind } from "../lib/types";
import { useAccounts } from "../store/accounts";
import { useFlags } from "../store/flags";
import { useShell } from "../store/shell";

/**
 * Flags: the catalogue, the profile being edited, where it applies, and what is
 * on disk right now.
 *
 * The screen never claims a flag is active. It shows what a profile says, what
 * Roblox's own default is, and whether a lease is live, because those are three
 * different facts and only the last one is in force.
 */

const KIND_LABEL: Record<FlagKind, string> = {
  flag: "on or off",
  int: "whole number",
  number: "number",
  string: "text",
  log: "log level",
  unknown: "unknown",
};

function defaultValue(kind: FlagKind, served: string | null): unknown | undefined {
  switch (kind) {
    case "flag":
      return served ? served.toLowerCase() !== "false" : true;
    case "int":
    case "number": {
      const parsed = served ? Number(served.split(";")[0]) : NaN;
      return Number.isFinite(parsed) ? parsed : 0;
    }
    case "string":
    case "log":
      return served ?? "";
    default:
      return undefined;
  }
}

export function FlagsPanel() {
  const open = useShell((state) => state.flagsOpen);
  const close = useShell((state) => state.closeFlags);
  const view = useFlags((state) => state.view);
  const load = useFlags((state) => state.load);
  const accounts = useAccounts((state) => state.accounts);

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  if (!open) return null;

  return (
    <PanelFrame title="Flags" onClose={close}>
      {view?.recovered ? (
        <div className="mt-4 rounded-card border border-white/10 bg-raised px-3.5 py-2.5 text-[11px] leading-[1.55] text-muted">
          Your client's own settings were put back on this start. The last session ended before
          it could do that itself, so the file it had written was still in place.
        </div>
      ) : null}

      <Catalog />
      <Profiles />
      <Scopes account={accounts ?? []} />

      {/* The primary action stays on screen while the rest scrolls: a drawer
          taller than the window must not hide the one button that writes. */}
      <div className="sticky bottom-0 z-10 -mx-5 mt-2 bg-panel px-5 pb-3">
        <Write />
      </div>
      <div className="h-2" aria-hidden />
    </PanelFrame>
  );
}

function Catalog() {
  const view = useFlags((state) => state.view);
  const results = useFlags((state) => state.results);
  const query = useFlags((state) => state.query);
  const search = useFlags((state) => state.search);
  const refresh = useFlags((state) => state.refreshCatalog);
  const working = useFlags((state) => state.working);
  const editing = useFlags((state) => state.editing);
  const setFlag = useFlags((state) => state.setFlag);
  const [busy, setBusy] = useState(false);

  const total = view?.catalog ? Object.keys(view.catalog.values).length : 0;
  const fetched = view?.catalog?.fetched ?? 0;

  const add = async (key: string, kind: FlagKind, served: string | null) => {
    const value = defaultValue(kind, served);
    if (value === undefined) return;
    setBusy(true);
    await setFlag(key, value);
    setBusy(false);
  };

  return (
    <Section
      title="Roblox's list"
      aside={
        <button
          type="button"
          onClick={() => void refresh()}
          disabled={working}
          className="ui -mt-1 h-6 rounded-control px-2 text-[11px] text-muted hover:bg-white/5 hover:text-body disabled:opacity-50"
        >
          {working ? "Fetching" : total > 0 ? "Fetch again" : "Fetch"}
        </button>
      }
    >
      <div className="px-3.5 py-2.5">
        <p className="text-[11px] leading-[1.55] text-muted">
          {total > 0
            ? `${total.toLocaleString()} keys, read from Roblox ${ago(fetched)}. A key's default here is the value the client itself is given.`
            : "Not fetched yet. Roblox serves its own list, which is the only source with real defaults."}
        </p>

        <input
          value={query}
          onChange={(event) => void search(event.target.value)}
          placeholder="Search a key, or type one Roblox does not serve"
          autoComplete="off"
          spellCheck={false}
          className="ui mt-2.5 h-8 w-full rounded-control border border-white/10 bg-raised px-2.5 font-mono text-[12px] text-body placeholder:font-sans placeholder:text-muted focus:border-gold/60 focus:outline-none"
        />
      </div>

      {query.trim() ? (
        results.length === 0 ? (
          <Row label="Nothing matches" hint="Try part of a name, like TargetFps." />
        ) : (
          results.map((hit) => {
            const already = editing
              ? Boolean(
                  view?.store.profiles.find((profile) => profile.name === editing)?.values?.[
                    hit.key
                  ],
                )
              : false;
            return (
              <div
                key={hit.key}
                className="flex min-h-[40px] items-center justify-between gap-3 border-b border-white/6 px-3.5 py-2 last:border-b-0"
              >
                <div className="min-w-0">
                  <div className="truncate font-mono text-[11.5px] text-body">{hit.key}</div>
                  <div className="mt-0.5 text-[11px] text-muted">
                    {KIND_LABEL[hit.kind]}
                    {hit.default === null ? (
                      <span> · Roblox serves no default for this one</span>
                    ) : (
                      <span className="num"> · default {hit.default}</span>
                    )}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => void add(hit.key, hit.kind, hit.default)}
                  disabled={already || busy || hit.kind === "unknown" || !editing}
                  className="ui h-6 shrink-0 rounded-control border border-white/10 px-2 text-[11px] text-muted hover:text-body disabled:opacity-40"
                >
                  {already ? "added" : "add"}
                </button>
              </div>
            );
          })
        )
      ) : null}
    </Section>
  );
}

function Profiles() {
  const view = useFlags((state) => state.view);
  const editing = useFlags((state) => state.editing);
  const choose = useFlags((state) => state.choose);
  const create = useFlags((state) => state.createProfile);
  const remove = useFlags((state) => state.deleteProfile);
  const setFlag = useFlags((state) => state.setFlag);
  const exportProfile = useFlags((state) => state.exportProfile);
  const importFile = useFlags((state) => state.importFile);
  const working = useFlags((state) => state.working);
  const [confirming, setConfirming] = useState(false);

  const profiles = view?.store.profiles ?? [];
  const profile = profiles.find((entry) => entry.name === editing) ?? null;

  const rows = useMemo(() => {
    if (!profile) return [];
    return Object.entries(profile.values).sort(([a], [b]) => a.localeCompare(b));
  }, [profile]);

  if (!view) return <Section title="Profiles">{<Row label="Reading your profiles" />}</Section>;

  return (
    <Section
      title="Profiles"
      aside={
        <button
          type="button"
          onClick={() => void create()}
          className="ui -mt-1 h-6 rounded-control px-2 text-[11px] text-muted hover:bg-white/5 hover:text-body"
        >
          New profile
        </button>
      }
    >
      {profiles.length === 0 ? (
        <Row
          label="No profile yet"
          hint="A profile is a named set of flags. Make one, then add keys from the list above."
        />
      ) : (
        <div className="flex flex-wrap gap-1.5 border-b border-white/6 px-3.5 py-2.5">
          {profiles.map((entry) => (
            <button
              key={entry.name}
              type="button"
              onClick={() => choose(entry.name)}
              className={cn(
                "ui h-6 rounded-control border px-2 text-[11px]",
                entry.name === editing
                  ? "border-gold/50 bg-gold/10 text-gold"
                  : "border-white/10 text-muted hover:text-body",
              )}
            >
              {entry.name}
            </button>
          ))}
        </div>
      )}

      {profile && rows.length === 0 ? (
        <Row label="This profile is empty" hint="Search above and add a key to it." />
      ) : null}

      {profile && rows.length > 0
        ? rows.map(([key, value]) => (
            <FlagLine
              key={key}
              name={key}
              value={value}
              onChange={(next) => void setFlag(key, next)}
              onRemove={() => void setFlag(key, null)}
              disabled={working}
            />
          ))
        : null}

      {profile ? (
        <div className="flex flex-wrap items-center gap-1.5 border-t border-white/6 px-3.5 py-2.5">
          <button
            type="button"
            onClick={() => void exportProfile(profile.name)}
            className="ui h-6 rounded-control border border-white/10 px-2 text-[11px] text-muted hover:text-body"
          >
            Export
          </button>
          {confirming ? (
            <>
              <span className="text-[11px] text-muted">Delete {profile.name}?</span>
              <button
                type="button"
                onClick={() => {
                  setConfirming(false);
                  void remove(profile.name);
                }}
                className="ui h-6 rounded-control bg-danger-fill px-2 text-[11px] font-semibold text-white"
              >
                Delete
              </button>
              <button
                type="button"
                onClick={() => setConfirming(false)}
                className="ui h-6 px-1.5 text-[11px] text-muted hover:text-body"
              >
                Keep
              </button>
            </>
          ) : (
            <button
              type="button"
              onClick={() => setConfirming(true)}
              className="ui h-6 rounded-control px-2 text-[11px] text-muted hover:bg-danger-fill/15 hover:text-danger"
            >
              Delete
            </button>
          )}
          <span className="text-[11px] text-muted">
            {plural(Object.keys(profile.values).length, "flag")} in {profile.name}
          </span>
        </div>
      ) : null}

      {view.exports.length > 0 ? (
        <div className="border-t border-white/6 px-3.5 py-2.5">
          <div className="text-[11px] text-muted">Files in the exports folder</div>
          <div className="mt-1.5 flex flex-wrap gap-1.5">
            {view.exports.map((file) => (
              <button
                key={file.path}
                type="button"
                onClick={() => void importFile(file.path)}
                className="ui h-6 rounded-control border border-white/10 px-2 font-mono text-[11px] text-muted hover:text-body"
                title={`Import ${file.path}`}
              >
                {file.name} <span className="num opacity-70">{file.keys}</span>
              </button>
            ))}
          </div>
        </div>
      ) : null}
    </Section>
  );
}

function FlagLine({
  name,
  value,
  onChange,
  onRemove,
  disabled,
}: {
  name: string;
  value: unknown;
  onChange: (next: unknown) => void;
  onRemove: () => void;
  disabled: boolean;
}) {
  const kind = useMemo<FlagKind>(() => guessKind(name), [name]);
  const [text, setText] = useState(String(value));

  // The store is the source of truth: when it changes under us, the field
  // follows. When the field changes, a short pause commits it, so a value is
  // saved by typing alone and nobody has to find Enter on a partly broken
  // keyboard.
  useEffect(() => {
    setText(String(value));
  }, [value]);

  const parsed = (raw: string): unknown | undefined => {
    if (kind === "int") {
      const number = Number.parseInt(raw, 10);
      return Number.isFinite(number) ? number : undefined;
    }
    if (kind === "number") {
      const number = Number.parseFloat(raw);
      return Number.isFinite(number) ? number : undefined;
    }
    if (kind === "string" || kind === "log") return raw;
    return undefined;
  };

  useEffect(() => {
    if (String(value) === text) return;
    const next = parsed(text);
    if (next === undefined) return;
    const timer = window.setTimeout(() => onChange(next), 500);
    return () => window.clearTimeout(timer);
    // `parsed` closes over `kind`, which is derived from the name.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  const commitNow = () => {
    const next = parsed(text);
    if (next !== undefined) onChange(next);
  };

  return (
    <div className="flex min-h-[42px] items-center justify-between gap-3 border-b border-white/6 px-3.5 py-2 last:border-b-0">
      <div className="min-w-0">
        <div className="truncate font-mono text-[11.5px] text-body">{name}</div>
        <div className="mt-0.5 text-[11px] text-muted">{KIND_LABEL[kind]}</div>
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        {kind === "flag" ? (
          <Toggle
            label={name}
            checked={value === true || value === "true" || value === "True"}
            onCheckedChange={(next) => onChange(next)}
            disabled={disabled}
          />
        ) : (
          <input
            value={text}
            onChange={(event) => setText(event.target.value)}
            onBlur={commitNow}
            onKeyDown={(event) => {
              if (event.key === "Enter") commitNow();
            }}
            inputMode={kind === "int" || kind === "number" ? "numeric" : "text"}
            spellCheck={false}
            className="ui h-7 w-28 rounded-control border border-white/10 bg-raised px-2 text-right font-mono text-[11.5px] text-body focus:border-gold/60 focus:outline-none"
          />
        )}

        <button
          type="button"
          onClick={onRemove}
          disabled={disabled}
          aria-label={`Remove ${name}`}
          title="Remove"
          className="ui flex h-7 w-7 items-center justify-center rounded-control text-muted hover:bg-danger-fill/15 hover:text-danger disabled:opacity-40"
        >
          ×
        </button>
      </div>
    </div>
  );
}

/** "1 flag", "4 flags". A count followed by a bare plural reads as a bug. */
function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

function guessKind(key: string): FlagKind {
  const rest =
    key.replace(/^(DF|SF|F|D|S|I)/, "") ?? "";
  if (rest.startsWith("Flag")) return "flag";
  if (rest.startsWith("Int")) return "int";
  if (rest.startsWith("String")) return "string";
  if (rest.startsWith("Log")) return "log";
  if (rest.startsWith("Float") || rest.startsWith("Double")) return "number";
  return "unknown";
}

function Scopes({ account }: { account: { userId: number; display: string }[] }) {
  const view = useFlags((state) => state.view);
  const setScope = useFlags((state) => state.setScope);
  const [place, setPlace] = useState("");

  if (!view) return null;
  const profiles = view.store.profiles;

  return (
    <Section title="Where it applies">
      <Row label="Any launch" hint="The last word for every flag no narrower scope sets.">
        <select
          value={view.store.scopes.global ?? ""}
          onChange={(event) => void setScope("global", null, event.target.value || null)}
          className="ui h-7 rounded-control border border-white/10 bg-raised px-1.5 text-[11.5px] text-body"
        >
          <option value="">None</option>
          {profiles.map((profile) => (
            <option key={profile.name} value={profile.name}>
              {profile.name}
            </option>
          ))}
        </select>
      </Row>

      {account.map((entry) => (
        <Row key={entry.userId} label={entry.display} hint="Beats the game and the global profile.">
          <select
            value={view.store.scopes.accounts[String(entry.userId)] ?? ""}
            onChange={(event) =>
              void setScope("account", entry.userId, event.target.value || null)
            }
            className="ui h-7 rounded-control border border-white/10 bg-raised px-1.5 text-[11.5px] text-body"
          >
            <option value="">None</option>
            {profiles.map((profile) => (
              <option key={profile.name} value={profile.name}>
                {profile.name}
              </option>
            ))}
          </select>
        </Row>
      ))}

      <Row
        label="One game"
        hint="A place id, for flags that only make sense there. Beats the global profile."
      >
        <div className="flex items-center gap-1.5">
          <input
            value={place}
            onChange={(event) => setPlace(event.target.value.replace(/[^0-9]/g, ""))}
            placeholder="place id"
            inputMode="numeric"
            aria-label="Place id"
            className="ui h-7 w-24 rounded-control border border-white/16 bg-raised px-2 text-right font-mono text-[11.5px] text-body placeholder:font-sans placeholder:text-muted focus:border-gold/60 focus:outline-none"
          />
          <select
            disabled={!place}
            value={view.store.scopes.games[place] ?? ""}
            onChange={(event) => void setScope("game", Number(place), event.target.value || null)}
            className="ui h-7 rounded-control border border-white/10 bg-raised px-1.5 text-[11.5px] text-body disabled:opacity-40"
          >
            <option value="">None</option>
            {profiles.map((profile) => (
              <option key={profile.name} value={profile.name}>
                {profile.name}
              </option>
            ))}
          </select>
        </div>
      </Row>

      {Object.keys(view.store.scopes.games).length > 0 ? (
        <div className="px-3.5 py-2.5 text-[11px] leading-[1.55] text-muted">
          Games already scoped: {Object.keys(view.store.scopes.games).join(", ")}
        </div>
      ) : null}
    </Section>
  );
}

function Write() {
  const view = useFlags((state) => state.view);
  const place = useFlags((state) => state.place);
  const apply = useFlags((state) => state.apply);
  const revert = useFlags((state) => state.revert);
  const working = useFlags((state) => state.working);
  const accounts = useAccounts((state) => state.accounts);

  if (!view) return null;

  const target = view.target;
  const lease = view.lease;
  const account = accounts?.[0]?.userId ?? null;
  const keys = view.store.profiles.reduce(
    (total, profile) => total + Object.keys(profile.values).length,
    0,
  );

  return (
    <Section
      title="Write"
      aside={
        lease ? (
          <Badge tone="gold">{lease.keys} live</Badge>
        ) : (
          <Badge tone={target?.exists ? "muted" : "muted"}>
            {target?.exists ? "file present" : "no file"}
          </Badge>
        )
      }
    >
      <div className="px-3.5 py-2.5">
        <div className="flex flex-wrap items-center gap-1.5">
          {(["user", "version"] as const).map((choice) => (
            <button
              key={choice}
              type="button"
              onClick={() => void place(choice)}
              disabled={working}
              aria-pressed={view.placement === choice}
              className={cn(
                "ui h-6 rounded-control border px-2 text-[11px]",
                view.placement === choice
                  ? "border-gold/50 bg-gold/10 text-gold"
                  : "border-white/10 text-muted hover:text-body",
              )}
            >
              {choice === "user" ? "This Windows user" : "This client build"}
            </button>
          ))}
        </div>

        <p className="mt-2 text-[11px] leading-[1.55] text-muted">
          {target
            ? `${target.note} ${target.exists ? `${Object.keys(target.current).length} keys are in it now.` : "The file does not exist yet."}`
            : "No place to write yet."}
        </p>
        {target ? (
          <p className="mt-1 break-all font-mono text-[10.5px] leading-[1.5] text-muted/80">
            {target.path}
          </p>
        ) : null}
      </div>

      <div className="flex flex-wrap items-center gap-1.5 border-t border-white/6 px-3.5 py-2.5">
        <button
          type="button"
          onClick={() => void apply(account, null)}
          disabled={working || keys === 0 || view.playerRunning}
          className="ui h-7 rounded-control bg-gold px-2.5 text-[11.5px] font-semibold text-page hover:bg-[#f0cd7c] disabled:opacity-40"
        >
          {working ? "Writing" : "Write now"}
        </button>

        {lease ? (
          <button
            type="button"
            onClick={() => void revert()}
            disabled={working}
            className="ui h-7 rounded-control border border-white/10 px-2.5 text-[11.5px] text-muted hover:text-body disabled:opacity-40"
          >
            Put it back
          </button>
        ) : null}

        <span className="text-[11px] text-muted">
          {view.playerRunning
            ? "A client is running, so a write would not reach it."
            : keys === 0
              ? "No flags to write yet."
              : `${plural(keys, "flag")} across ${plural(view.store.profiles.length, "profile")}.`}
        </span>
      </div>

      {lease ? (
        <div className="border-t border-white/6 px-3.5 py-2.5 text-[11px] leading-[1.55] text-muted">
          Applied {ago(lease.at)}. A launch puts the previous settings back when its client
          exits, and this app does the same when it closes.
        </div>
      ) : null}
    </Section>
  );
}
