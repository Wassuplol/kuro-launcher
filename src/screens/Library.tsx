import { useEffect, useMemo, useRef, useState } from "react";
import { RefreshCw, Search } from "lucide-react";

import { GameTile, TileGrid } from "../components/GameTile";
import { PageFrame } from "../components/PageFrame";
import { ago } from "../lib/format";
import { useAccounts } from "../store/accounts";
import { useLibrary } from "../store/library";
import { useShell } from "../store/shell";

/**
 * Library: the games you starred on Roblox, and the ones you launched from here.
 *
 * Two shelves with two different sources, said plainly on the page. Favourites are
 * Roblox's own list for the selected account. Recently played is this app's record
 * of what it launched, because Roblox publishes no anonymous play history and a
 * shelf that claims more than it knows is worse than a short one.
 *
 * The filter is local on purpose: it narrows the rows already in hand, costs no
 * request, and says how many it kept ("12 of 50"). A field that spent the
 * four-a-minute search budget from inside the library would be a different feature.
 */
export function LibraryScreen() {
  const accounts = useAccounts((state) => state.accounts);
  const loadAccounts = useAccounts((state) => state.load);

  const favourites = useLibrary((state) => state.favourites);
  const next = useLibrary((state) => state.next);
  const note = useLibrary((state) => state.note);
  const fetched = useLibrary((state) => state.fetched);
  const cached = useLibrary((state) => state.cached);
  const loading = useLibrary((state) => state.loading);
  const problem = useLibrary((state) => state.problem);
  const load = useLibrary((state) => state.load);
  const more = useLibrary((state) => state.more);

  const history = useLibrary((state) => state.history);
  const historyLoaded = useLibrary((state) => state.historyLoaded);
  const loadHistory = useLibrary((state) => state.loadHistory);
  const clearHistory = useLibrary((state) => state.clearHistory);

  const go = useShell((state) => state.go);
  const prefill = useShell((state) => state.prefill);

  const [query, setQuery] = useState("");
  const [confirming, setConfirming] = useState(false);
  const confirmTimer = useRef<number | null>(null);

  useEffect(() => {
    void loadAccounts();
    void loadHistory();
  }, [loadAccounts, loadHistory]);

  const chosen = accounts?.find((account) => account.isDefault) ?? accounts?.[0] ?? null;

  useEffect(() => {
    void load(chosen?.userId ?? null);
  }, [chosen?.userId, load]);

  useEffect(() => {
    return () => {
      if (confirmTimer.current !== null) window.clearTimeout(confirmTimer.current);
    };
  }, []);

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return favourites;
    return favourites.filter(
      (game) =>
        game.name.toLowerCase().includes(needle) ||
        game.creator.toLowerCase().includes(needle),
    );
  }, [favourites, query]);

  const header = chosen ? `${chosen.display}'s favourites` : "Favourites";

  return (
    <PageFrame
      wide
      title="Library"
      subtitle="The games you starred on Roblox, and the ones you launched from here. A tile prepares a launch; nothing starts until you say so."
      aside={
        chosen ? (
          <button
            type="button"
            onClick={() => void load(chosen.userId, true)}
            disabled={loading}
            className="ui flex h-8 items-center gap-1.5 rounded-control border border-white/10 px-2.5 text-[11.5px] text-muted hover:text-body disabled:opacity-40"
          >
            <RefreshCw size={13} strokeWidth={1.6} className={loading ? "animate-spin" : undefined} />
            {loading ? "Reading" : "Refresh"}
          </button>
        ) : null
      }
    >
      {!accounts ? <p className="mt-5 text-[12.5px] text-muted">Reading your accounts.</p> : null}

      {accounts && accounts.length === 0 ? (
        <div className="mt-5 rounded-card border border-white/8 bg-panel px-4 py-4">
          <div className="text-[13px] text-body">Favourites come from an account</div>
          <p className="mt-1 max-w-[62ch] text-[12px] leading-[1.6] text-muted">
            Add one on the Accounts page and the games you have starred appear here. Nothing
            is read until an account exists, because there is no public list to guess at.
          </p>
          <button
            type="button"
            onClick={() => go("accounts")}
            className="ui mt-3 flex h-8 items-center rounded-control bg-gold px-3 text-[12px] font-medium text-page"
          >
            Open Accounts
          </button>
        </div>
      ) : null}

      {problem ? (
        <div className="mt-5 rounded-card border border-danger/35 bg-danger-fill/10 px-4 py-3">
          <div className="text-[12.5px] text-danger">{problem.message}</div>
          {problem.detail ? (
            <div className="mt-1 text-[11.5px] text-muted">{problem.detail}</div>
          ) : null}
        </div>
      ) : null}

      {chosen ? (
        <section className="mt-6">
          <div className="flex flex-wrap items-baseline justify-between gap-3">
            <div className="flex items-baseline gap-2">
              <h2 className="text-[13px] font-semibold text-body">{header}</h2>
              <span className="text-[11px] text-muted">
                {query.trim()
                  ? `${shown.length} of ${favourites.length}`
                  : `${favourites.length} loaded`}
              </span>
            </div>

            {favourites.length > 8 ? (
              <div className="relative">
                <Search
                  size={12}
                  strokeWidth={1.8}
                  className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted"
                  aria-hidden
                />
                <input
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Filter by name or creator"
                  aria-label="Filter your favourites"
                  className="h-8 w-[220px] rounded-control border border-white/10 bg-panel pr-2.5 pl-7 text-[12px] text-body outline-none placeholder:text-muted/70 focus:border-gold/40"
                />
              </div>
            ) : null}
          </div>

          {loading && favourites.length === 0 ? <ShelfSkeleton /> : null}

          {!loading && favourites.length === 0 && !problem ? (
            <p className="mt-3 rounded-card border border-white/8 bg-panel px-4 py-3 text-[12.5px] leading-[1.6] text-muted">
              Roblox answered and this account has not starred anything yet. A star on
              roblox.com lands here on the next read.
            </p>
          ) : null}

          {shown.length > 0 ? (
            <div className="mt-3">
              <TileGrid>
                {shown.map((game) => (
                  <GameTile key={game.universe} game={game} />
                ))}
              </TileGrid>
            </div>
          ) : null}

          {favourites.length > 0 && shown.length === 0 ? (
            <p className="mt-3 text-[12.5px] text-muted">
              Nothing in this shelf matches "{query.trim()}".
            </p>
          ) : null}

          {next ? (
            <button
              type="button"
              onClick={() => void more()}
              disabled={loading}
              className="ui mt-4 flex h-8 items-center rounded-control border border-white/10 px-3 text-[11.5px] text-muted hover:text-body disabled:opacity-40"
            >
              {loading ? "Reading" : "Load more"}
            </button>
          ) : null}

          {note ? <p className="mt-3 text-[11.5px] text-muted">{note}</p> : null}

          {fetched ? (
            <div className="mt-3 text-[11px] text-muted">
              Read {ago(fetched)}
              {cached ? " from the cache" : ""}. Favourites are one call a page, so a page is
              kept for a couple of minutes.
            </div>
          ) : null}
        </section>
      ) : null}

      <section className="mt-9 border-t border-white/8 pt-6">
        <div className="flex items-baseline justify-between gap-3">
          <div className="flex items-baseline gap-2">
            <h2 className="text-[13px] font-semibold text-body">Recently played</h2>
            <span className="text-[11px] text-muted">
              {history.length === 0
                ? "nothing yet"
                : history.length === 1
                  ? "1 launch"
                  : `${history.length} launches`}
            </span>
          </div>

          {history.length > 0 ? (
            <button
              type="button"
              onClick={() => {
                if (confirming) {
                  setConfirming(false);
                  if (confirmTimer.current !== null) window.clearTimeout(confirmTimer.current);
                  void clearHistory();
                  return;
                }
                setConfirming(true);
                confirmTimer.current = window.setTimeout(() => setConfirming(false), 4000);
              }}
              className={`ui rounded-control px-2 py-1 text-[11.5px] ${
                confirming ? "bg-danger-fill/15 text-danger" : "text-muted hover:text-body"
              }`}
            >
              {confirming ? "Clear this list?" : "Clear"}
            </button>
          ) : null}
        </div>

        {historyLoaded && history.length === 0 ? (
          <p className="mt-3 text-[12.5px] leading-[1.6] text-muted">
            A launch you start from this app lands here, with the game it was and when. It is
            this machine's own record: Roblox does not publish a play history to read.
          </p>
        ) : null}

        {history.length > 0 ? (
          <ul className="mt-2">
            {history.map((entry) => (
              <li
                key={`${entry.account ?? 0}-${entry.universe ?? entry.place}-${entry.at}`}
                className="flex items-center gap-3 border-b border-white/6 py-2.5 last:border-b-0"
              >
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[12.5px] text-body">
                    {entry.name?.trim() || `Place ${entry.place}`}
                  </div>
                  <div className="text-[11px] text-muted">
                    {modeWords(entry.mode)} · {ago(entry.at)}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => prefill(String(entry.place))}
                  className="ui shrink-0 rounded-control border border-white/10 px-2 py-1 text-[11.5px] text-muted hover:border-gold/40 hover:text-gold"
                >
                  Prepare
                </button>
              </li>
            ))}
          </ul>
        ) : null}
      </section>
    </PageFrame>
  );
}

function modeWords(mode: string): string {
  switch (mode) {
    case "normal":
      return "any server";
    case "private":
      return "a private server";
    case "server":
      return "one server";
    case "friend":
      return "a friend";
    default:
      return mode;
  }
}

function ShelfSkeleton() {
  return (
    <div
      className="mt-3 grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] gap-x-3 gap-y-4"
      aria-hidden
    >
      {Array.from({ length: 12 }).map((_, index) => (
        <div key={index}>
          <div className="aspect-square rounded-[6px] bg-white/5" />
          <div className="mt-2 h-2.5 w-4/5 rounded bg-white/5" />
          <div className="mt-1.5 h-2 w-2/5 rounded bg-white/5" />
        </div>
      ))}
    </div>
  );
}
