import { useEffect, useRef, useState } from "react";
import { Search as SearchIcon } from "lucide-react";

import { GameTile, TileGrid } from "../components/GameTile";
import { PageFrame } from "../components/PageFrame";
import { cn } from "../lib/cn";
import { api, CommandError, detailOf } from "../lib/ipc";
import type { DiscoverySearch } from "../lib/types";

/**
 * Search: games or people, one query at a time.
 *
 * Roblox allows four searches a minute, so this page does not search as you type
 * and does not search on mount. It spends one call on a deliberate submit, keeps
 * the answer for three minutes, and says how much of the minute is left so a
 * refusal reads as a budget rather than a broken box.
 */
export function SearchScreen() {
  const [term, setTerm] = useState("");
  const [people, setPeople] = useState(false);
  const [result, setResult] = useState<DiscoverySearch | null>(null);
  const [problem, setProblem] = useState<{ message: string; detail?: string } | null>(null);
  const [working, setWorking] = useState(false);
  const [budget, setBudget] = useState<[string, number, number][]>([]);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      if (!document.querySelector("[role=dialog]")) field.current?.focus();
    }, 120);
    return () => window.clearTimeout(timer);
  }, []);

  const run = async (nextTerm: string, nextPeople: boolean) => {
    const trimmed = nextTerm.trim();
    if (trimmed.length < 2) {
      setProblem({ message: "Type at least two characters." });
      return;
    }

    setWorking(true);
    setProblem(null);
    try {
      const answer = await api.discoverySearch(trimmed, nextPeople);
      setResult(answer);
      setBudget(await api.discoveryBudget().catch(() => []));
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      setResult(null);
      setProblem({
        message: failure?.message ?? "The search could not be sent.",
        detail: failure ? detailOf(failure.code, failure.action) : undefined,
      });
    } finally {
      setWorking(false);
    }
  };

  const searchLine = budget.find(([name]) => name === "search");
  const games = result?.games ?? [];
  const found = result?.people ?? [];

  return (
    <PageFrame
      wide
      title="Search"
      subtitle="Games and people, from Roblox's own index. A search is a request, not a keystroke: four of them fit in a minute."
    >
      <div className="mt-5 rounded-card border border-white/8 bg-page/50 p-3.5">
        <div className="flex flex-wrap items-center gap-2">
          <div className="relative min-w-0 flex-1">
            <SearchIcon
              size={15}
              strokeWidth={1.6}
              className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-muted"
            />
            <input
              ref={field}
              value={term}
              onChange={(event) => setTerm(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void run(term, people);
              }}
              placeholder={people ? "A username or display name" : "A game, a creator, a genre"}
              autoComplete="off"
              spellCheck={false}
              aria-label={people ? "Search people" : "Search games"}
              className="ui h-9 w-full rounded-control border border-white/10 bg-raised pr-2.5 pl-8 text-[12.5px] text-body placeholder:text-muted focus:border-gold/60 focus:outline-none"
            />
          </div>

          <div className="flex items-center gap-1.5">
            {([
              ["games", false],
              ["people", true],
            ] as const).map(([label, value]) => (
              <button
                key={label}
                type="button"
                onClick={() => setPeople(value)}
                aria-pressed={people === value}
                className={cn(
                  "ui h-8 rounded-control border px-2.5 text-[11.5px]",
                  people === value
                    ? "border-gold/50 bg-gold/10 text-gold"
                    : "border-white/10 text-muted hover:text-body",
                )}
              >
                {label}
              </button>
            ))}

            <button
              type="button"
              onClick={() => void run(term, people)}
              disabled={working}
              className="ui h-8 rounded-control bg-gold px-3 text-[11.5px] font-semibold text-page hover:bg-[#f0cd7c] disabled:opacity-40"
            >
              {working ? "Searching" : "Search"}
            </button>
          </div>
        </div>

        <div className="mt-2 text-[11px] text-muted">
          {searchLine
            ? `${searchLine[1]} of ${searchLine[2]} searches used this minute. An answer is kept for three.`
            : "An answer is kept for three minutes, so a repeat costs nothing."}
        </div>
      </div>

      {problem ? (
        <div className="mt-4 rounded-card border border-danger/35 bg-danger-fill/10 px-4 py-3">
          <div className="text-[12.5px] text-danger">{problem.message}</div>
          {problem.detail ? <div className="mt-1 text-[11.5px] text-muted">{problem.detail}</div> : null}
        </div>
      ) : null}

      {result && !people && games.length > 0 ? (
        <section className="mt-6">
          <div className="flex items-baseline justify-between gap-3">
            <h2 className="text-[13px] font-semibold text-body">Games</h2>
            <span className="text-[11px] text-muted">
              {games.length} {games.length === 1 ? "result" : "results"}
              {result.cached ? " from the cache" : ""}
            </span>
          </div>
          <div className="mt-3">
            <TileGrid>
              {games.map((game) => (
                <GameTile key={game.universe} game={game} />
              ))}
            </TileGrid>
          </div>
        </section>
      ) : null}

      {result && people ? (
        <section className="mt-6">
          <div className="flex items-baseline justify-between gap-3">
            <h2 className="text-[13px] font-semibold text-body">People</h2>
            <span className="text-[11px] text-muted">
              {found.length} {found.length === 1 ? "result" : "results"}
              {result.cached ? " from the cache" : ""}
            </span>
          </div>
          <div className="mt-3 divide-y divide-white/6 overflow-hidden rounded-card border border-white/8 bg-page/50">
            {found.map((person) => (
              <button
                key={person.user}
                type="button"
                onClick={() =>
                  void api.openSite(`https://www.roblox.com/users/${person.user}/profile`)
                }
                className="flex w-full items-center gap-3 px-3.5 py-2.5 text-left hover:bg-white/4"
              >
                <span className="h-8 w-8 shrink-0 overflow-hidden rounded-full bg-raised ring-1 ring-white/8">
                  {person.headshot ? (
                    <img src={person.headshot} alt="" className="h-full w-full object-cover" />
                  ) : null}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[12.5px] text-body">
                    {person.display || person.name}
                    {person.verified ? <span className="ml-1.5 text-[10px] text-gold">verified</span> : null}
                  </span>
                  <span className="block truncate text-[11px] text-muted">@{person.name}</span>
                </span>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {result && !people && games.length === 0 ? (
        <div className="mt-5 rounded-card border border-white/8 bg-panel px-4 py-3 text-[12.5px] text-muted">
          Nothing came back for that. Roblox's index is the limit here, not this box.
        </div>
      ) : null}

      {!result && !problem ? (
        <div className="mt-6 rounded-card border border-white/8 bg-panel px-4 py-6 text-center">
          <div className="text-[13px] text-body">Search Roblox</div>
          <div className="mt-1 text-[11.5px] text-muted">
            Games go straight to a prepared launch. People open their profile in a browser.
          </div>
        </div>
      ) : null}
    </PageFrame>
  );
}
