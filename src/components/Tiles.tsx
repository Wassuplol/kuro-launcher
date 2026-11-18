import { useCallback, useEffect, useState } from "react";

import { Section } from "./Panel";
import { api, CommandError } from "../lib/ipc";
import type { AccountView, InstalledClient, LaunchTile, Launched } from "../lib/types";

/**
 * Tiles: a destination that remembers the choices around it.
 *
 * Every field here already exists on this page or on the Flags page. A tile only
 * remembers the combination, so a launch that usually means "this server, as this
 * account, with these flags, on this build" becomes one click instead of four.
 *
 * A tile names its flag profile rather than copying it, so editing the flags behind a
 * tile changes what it launches with. That is the point of naming them.
 */

const AFTER: { value: LaunchTile["after"]; label: string }[] = [
  { value: "stay", label: "Stay here" },
  { value: "minimize", label: "Minimise KURO" },
];

type Draft = {
  id: string;
  name: string;
  target: string;
  account: string;
  flags: string;
  client: string;
  after: LaunchTile["after"];
  used: number;
};

const field =
  "ui mt-1 h-8 w-full rounded-control border border-white/10 bg-raised px-2.5 text-[12px] text-body placeholder:text-muted focus:border-gold/60 focus:outline-none";

/** What a tile points at, said in the shortest honest way. */
function where(target: string): string {
  const text = target.trim();
  if (/^\d+$/.test(text)) return `place ${text}`;
  const place = text.match(/games\/(\d+)/) ?? text.match(/[?&]placeId=(\d+)/);
  if (place) return `place ${place[1]}`;
  if (text.startsWith("roblox://")) return "a deep link";
  return text.length > 36 ? `${text.slice(0, 36)}…` : text;
}

export function Tiles({
  seed,
  accounts,
  onLaunched,
}: {
  /** What the page's own destination field holds, used to start a new tile. */
  seed: string;
  accounts: AccountView[];
  onLaunched: (launched: Launched, accountId: number | null) => void;
}) {
  const [tiles, setTiles] = useState<LaunchTile[] | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [lists, setLists] = useState<{ flags: string[]; builds: InstalledClient[] } | null>(null);

  const load = useCallback(async () => {
    try {
      setTiles(await api.tilesList());
    } catch (value) {
      setProblem(value instanceof CommandError ? value.message : String(value));
      setTiles([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // The two lists the editor offers are only needed once it opens, and both cost a
  // request. Loaded together so the form never draws half full.
  const open = async (tile?: LaunchTile) => {
    setProblem(null);
    setNote(null);
    setDraft({
      id: tile?.id ?? "",
      name: tile?.name ?? "",
      target: tile?.target ?? seed,
      account: tile?.account ? String(tile.account) : "",
      flags: tile?.flags ?? "",
      client: tile?.client ?? "",
      after: tile?.after ?? "stay",
      used: tile?.used ?? 0,
    });

    if (lists) return;
    const [view, supply] = await Promise.all([
      api.flagsView().catch(() => null),
      api.supplySnapshot().catch(() => null),
    ]);
    setLists({
      flags: view?.store.profiles.map((profile) => profile.name) ?? [],
      builds: (supply?.installed ?? []).filter(
        (client) => client.real && client.kind === "player",
      ),
    });
  };

  const save = async () => {
    if (!draft) return;
    setProblem(null);
    try {
      const next = await api.tileSave({
        id: draft.id,
        name: draft.name,
        target: draft.target,
        account: draft.account ? Number(draft.account) : null,
        flags: draft.flags || null,
        client: draft.client || null,
        after: draft.after,
        at: 0,
        used: draft.used,
      });
      setTiles(next);
      setDraft(null);
    } catch (value) {
      setProblem(value instanceof CommandError ? value.message : String(value));
    }
  };

  const drop = async () => {
    if (!draft?.id) return;
    setProblem(null);
    try {
      setTiles(await api.tileDelete(draft.id));
      setDraft(null);
    } catch (value) {
      setProblem(value instanceof CommandError ? value.message : String(value));
    }
  };

  const play = async (tile: LaunchTile) => {
    setBusy(tile.id);
    setProblem(null);
    setNote(null);
    try {
      const launched = await api.tileLaunch(tile.id);
      setNote(launched.reused ? launched.detail : `${tile.name}: ${launched.detail}`);
      onLaunched(launched, tile.account);
      setTiles(await api.tilesList());
    } catch (value) {
      setProblem(value instanceof CommandError ? value.message : String(value));
    } finally {
      setBusy(null);
    }
  };

  const accountName = (id: number | null) =>
    id ? (accounts.find((entry) => entry.userId === id)?.display ?? "an account that is gone") : "the default account";

  const summary = (tile: LaunchTile) => {
    const parts = [where(tile.target), accountName(tile.account)];
    parts.push(tile.flags ? `flags: ${tile.flags}` : "flags: the chain");
    if (tile.client) parts.push(tile.client);
    if (tile.after === "minimize") parts.push("minimises KURO");
    return parts.join(" · ");
  };

  return (
    <Section
      title="Tiles"
      aside={
        <button
          type="button"
          onClick={() => void open()}
          className="ui -mt-1 h-6 rounded-control px-2 text-[11px] text-muted hover:bg-white/5 hover:text-body"
        >
          New tile
        </button>
      }
    >
      {draft ? (
        <div className="border-b border-white/6 px-3.5 py-3">
          <div className="grid grid-cols-2 gap-2.5">
            <div>
              <label htmlFor="tile-name" className="text-[11px] text-muted">
                Name
              </label>
              <input
                id="tile-name"
                value={draft.name}
                onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                placeholder="Bedwars with the crew"
                autoComplete="off"
                spellCheck={false}
                className={field}
              />
            </div>
            <div>
              <label htmlFor="tile-target" className="text-[11px] text-muted">
                Destination
              </label>
              <input
                id="tile-target"
                value={draft.target}
                onChange={(event) => setDraft({ ...draft, target: event.target.value })}
                placeholder="A game link, a place id or an invite"
                autoComplete="off"
                spellCheck={false}
                className={`${field} font-mono`}
              />
            </div>
            <div>
              <label htmlFor="tile-account" className="text-[11px] text-muted">
                Account
              </label>
              <select
                id="tile-account"
                value={draft.account}
                onChange={(event) => setDraft({ ...draft, account: event.target.value })}
                className={field}
              >
                <option value="">The default account</option>
                {accounts.map((entry) => (
                  <option key={entry.userId} value={entry.userId}>
                    {entry.display}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label htmlFor="tile-flags" className="text-[11px] text-muted">
                Flag profile
              </label>
              <select
                id="tile-flags"
                value={draft.flags}
                onChange={(event) => setDraft({ ...draft, flags: event.target.value })}
                className={field}
              >
                <option value="">The account, game and global chain</option>
                {(lists?.flags ?? []).map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label htmlFor="tile-build" className="text-[11px] text-muted">
                Build
              </label>
              <select
                id="tile-build"
                value={draft.client}
                onChange={(event) => setDraft({ ...draft, client: event.target.value })}
                className={field}
              >
                <option value="">The pinned build, then the live one</option>
                {(lists?.builds ?? []).map((client) => (
                  <option key={client.folder} value={client.folder}>
                    {client.version ?? client.folder}
                    {client.live ? " (live)" : ""}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label htmlFor="tile-after" className="text-[11px] text-muted">
                Once the game starts
              </label>
              <select
                id="tile-after"
                value={draft.after}
                onChange={(event) =>
                  setDraft({ ...draft, after: event.target.value as LaunchTile["after"] })
                }
                className={field}
              >
                {AFTER.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="mt-2.5 flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => void save()}
              className="ui h-7 rounded-control bg-gold px-2.5 text-[12px] font-semibold text-page hover:bg-[#f0cd7c]"
            >
              {draft.id ? "Save changes" : "Save tile"}
            </button>
            <button
              type="button"
              onClick={() => setDraft(null)}
              className="ui h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:text-body"
            >
              Cancel
            </button>
            {draft.id ? (
              <button
                type="button"
                onClick={() => void drop()}
                className="ui ml-auto h-7 rounded-control border border-white/10 px-2.5 text-[12px] text-muted hover:border-danger/40 hover:text-danger"
              >
                Delete this tile
              </button>
            ) : null}
          </div>

          <p className="mt-2 text-[11px] leading-[1.55] text-muted">
            A tile names its flag profile rather than copying it, so editing those flags later changes
            what this tile launches with.
          </p>
        </div>
      ) : null}

      {tiles === null ? (
        <p className="px-3.5 py-4 text-[12px] text-muted">Reading your tiles.</p>
      ) : tiles.length === 0 && !draft ? (
        <p className="px-3.5 py-4 text-[12px] text-muted">
          No tiles yet. Fill in a destination below and save it here, and the next launch is one click.
        </p>
      ) : (
        tiles.map((tile) => (
          <div
            key={tile.id}
            className="flex items-center justify-between gap-3 border-b border-white/6 px-3.5 py-2.5 last:border-b-0"
          >
            <div className="min-w-0">
              <div className="flex items-center gap-1.5">
                <span className="truncate text-[13px] text-body">{tile.name}</span>
                {tile.used > 0 ? (
                  <span className="num shrink-0 text-[10px] text-muted">{tile.used} launched</span>
                ) : null}
              </div>
              <div className="mt-0.5 truncate text-[11px] text-muted">{summary(tile)}</div>
            </div>

            <div className="flex shrink-0 items-center gap-1">
              <button
                type="button"
                onClick={() => void play(tile)}
                disabled={busy !== null}
                className="ui h-7 rounded-control bg-gold px-2.5 text-[12px] font-semibold text-page hover:bg-[#f0cd7c] disabled:opacity-40"
              >
                {busy === tile.id ? "Starting" : "Play"}
              </button>
              <button
                type="button"
                onClick={() => void open(tile)}
                className="ui h-7 rounded-control border border-white/10 px-2 text-[12px] text-muted hover:text-body"
              >
                Edit
              </button>
            </div>
          </div>
        ))
      )}

      {note ? (
        <p className="border-t border-white/6 px-3.5 py-2 text-[11px] leading-[1.55] text-body">{note}</p>
      ) : null}
      {problem ? (
        <p className="border-t border-white/6 px-3.5 py-2 text-[11px] leading-[1.55] text-danger">
          {problem}
        </p>
      ) : null}
    </Section>
  );
}
