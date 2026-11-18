import { useEffect } from "react";
import { RefreshCw } from "lucide-react";

import { GameTile, TileGrid } from "../components/GameTile";
import { PageFrame } from "../components/PageFrame";
import { ago } from "../lib/format";
import { useDiscovery } from "../store/discovery";

/**
 * Home: what Roblox is serving, read with the signed-in session.
 *
 * The feed arrives as ids, so every tile here was hydrated by a second call. That
 * is why the page loads in two beats rather than one, and why a failure says
 * which of the two failed instead of showing an empty shelf.
 */
export function HomeScreen() {
  const feed = useDiscovery((state) => state.feed);
  const loading = useDiscovery((state) => state.loading);
  const problem = useDiscovery((state) => state.problem);
  const load = useDiscovery((state) => state.load);

  useEffect(() => {
    void load();
  }, [load]);

  const sections = feed?.sections.filter((section) => section.games.length > 0) ?? [];

  return (
    <PageFrame
      wide
      title="Home"
      subtitle="What Roblox is serving right now, read with your session. Picking a game prepares a launch; nothing starts until you say so."
      aside={
        <button
          type="button"
          onClick={() => void load(true)}
          disabled={loading}
          className="ui flex h-8 items-center gap-1.5 rounded-control border border-white/10 px-2.5 text-[11.5px] text-muted hover:text-body disabled:opacity-40"
        >
          <RefreshCw size={13} strokeWidth={1.6} className={loading ? "animate-spin" : undefined} />
          {loading ? "Reading" : "Refresh"}
        </button>
      }
    >
      {problem ? (
        <div className="mt-5 rounded-card border border-danger/35 bg-danger-fill/10 px-4 py-3">
          <div className="text-[12.5px] text-danger">{problem.message}</div>
          {problem.detail ? (
            <div className="mt-1 text-[11.5px] text-muted">{problem.detail}</div>
          ) : null}
        </div>
      ) : null}

      {loading && !feed ? <FeedSkeleton /> : null}

      {sections.map((section) => (
        <section key={section.topic} className="mt-7 first:mt-5">
          <div className="flex items-baseline justify-between gap-3">
            <h2 className="text-[13px] font-semibold text-body">{section.topic}</h2>
            <span className="text-[11px] text-muted">
              {section.games.length} {section.games.length === 1 ? "game" : "games"}
            </span>
          </div>
          <div className="mt-3">
            <TileGrid>
              {section.games.map((game) => (
                <GameTile key={`${section.topic}-${game.universe}`} game={game} />
              ))}
            </TileGrid>
          </div>
        </section>
      ))}

      {feed && sections.length === 0 && !problem ? (
        <div className="mt-5 rounded-card border border-white/8 bg-panel px-4 py-3 text-[12.5px] text-muted">
          Roblox answered, and every section came back empty for this account.
        </div>
      ) : null}

      {feed ? (
        <div className="mt-8 border-t border-white/6 pt-3 text-[11px] text-muted">
          Read {ago(feed.fetched)}
          {feed.cached ? " from the cache" : ""}. The feed is four calls a minute, so it is kept for a
          couple of them.
        </div>
      ) : null}
    </PageFrame>
  );
}

function FeedSkeleton() {
  return (
    <div className="mt-7" aria-hidden>
      <div className="h-3 w-32 rounded bg-white/6" />
      <div className="mt-3 grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] gap-x-3 gap-y-4">
        {Array.from({ length: 14 }).map((_, index) => (
          <div key={index}>
            <div className="aspect-square rounded-[6px] bg-white/5" />
            <div className="mt-2 h-2.5 w-4/5 rounded bg-white/5" />
            <div className="mt-1.5 h-2 w-2/5 rounded bg-white/5" />
          </div>
        ))}
      </div>
    </div>
  );
}
