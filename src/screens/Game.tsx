import { useEffect, useState } from "react";
import { ArrowLeft, ExternalLink, Play } from "lucide-react";

import { PageFrame } from "../components/PageFrame";
import { cn } from "../lib/cn";
import { ago } from "../lib/format";
import { api } from "../lib/ipc";
import { useDiscovery } from "../store/discovery";
import { useShell } from "../store/shell";

/**
 * One game: what it is, then the servers it is actually running.
 *
 * The server list is the most expensive call on Roblox's whole surface at three a
 * minute, so it is behind a tab and never fetched on mount. A server row hands the
 * launch page a `roblox://…&gameInstanceId=` string, which is the only way to land
 * in the exact server anybody picked.
 */
export function GameScreen() {
  const target = useShell((state) => state.game);
  const go = useShell((state) => state.go);
  const prefill = useShell((state) => state.prefill);

  const detail = useDiscovery((state) => state.detail);
  const loading = useDiscovery((state) => state.detailLoading);
  const problem = useDiscovery((state) => state.detailProblem);
  const loadGame = useDiscovery((state) => state.loadGame);

  const servers = useDiscovery((state) => state.servers);
  const serversLoading = useDiscovery((state) => state.serversLoading);
  const serversProblem = useDiscovery((state) => state.serversProblem);
  const loadServers = useDiscovery((state) => state.loadServers);

  const [tab, setTab] = useState<"about" | "servers">("about");

  useEffect(() => {
    if (target) void loadGame(target.universe);
  }, [target, loadGame]);

  useEffect(() => {
    setTab("about");
  }, [target]);

  if (!target) {
    return (
      <PageFrame title="Game" subtitle="Nothing is open.">
        <div className="mt-5 rounded-card border border-white/8 bg-panel px-4 py-6 text-center">
          <div className="text-[13px] text-body">Pick a game first</div>
          <div className="mt-1 text-[11.5px] text-muted">
            A game page is opened from a tile on Home or in Search.
          </div>
          <button
            type="button"
            onClick={() => go("home")}
            className="ui mt-3 h-8 rounded-control bg-gold px-3 text-[11.5px] font-semibold text-page hover:bg-[#f0cd7c]"
          >
            Back to Home
          </button>
        </div>
      </PageFrame>
    );
  }

  const name = detail?.name ?? "Reading the game";
  const total = detail ? detail.upVotes + detail.downVotes : 0;
  const ratio = detail && total > 0 ? Math.round((detail.upVotes / total) * 100) : null;

  return (
    <PageFrame
      wide
      title={name}
      subtitle={detail ? `by ${detail.creator || "an unknown creator"}` : "Reading it from Roblox."}
      aside={
        <button
          type="button"
          onClick={() => go("home")}
          className="ui flex h-8 items-center gap-1.5 rounded-control border border-white/10 px-2.5 text-[11.5px] text-muted hover:text-body"
        >
          <ArrowLeft size={13} strokeWidth={1.6} />
          Home
        </button>
      }
    >
      {problem ? (
        <div className="mt-5 rounded-card border border-danger/35 bg-danger-fill/10 px-4 py-3">
          <div className="text-[12.5px] text-danger">{problem.message}</div>
          {problem.detail ? <div className="mt-1 text-[11.5px] text-muted">{problem.detail}</div> : null}
        </div>
      ) : null}

      <div className="mt-5 flex flex-wrap items-start gap-5">
        <div className="h-[168px] w-[168px] shrink-0 overflow-hidden rounded-card bg-raised ring-1 ring-white/8">
          {detail?.icon ? (
            <img src={detail.icon} alt="" className="h-full w-full object-cover" />
          ) : null}
        </div>

        <div className="min-w-[260px] flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => prefill(String(target.place))}
              className="ui flex h-9 items-center gap-1.5 rounded-control bg-gold px-3.5 text-[12.5px] font-semibold text-page hover:bg-[#f0cd7c]"
            >
              <Play size={14} strokeWidth={2} />
              Prepare a launch
            </button>
            <button
              type="button"
              onClick={() =>
                void api.openSite(`https://www.roblox.com/games/${target.place}`)
              }
              className="ui flex h-9 items-center gap-1.5 rounded-control border border-white/10 px-3 text-[12px] text-muted hover:text-body"
            >
              <ExternalLink size={13} strokeWidth={1.6} />
              Open on roblox.com
            </button>
          </div>

          <dl className="mt-4 grid grid-cols-2 gap-x-6 gap-y-2 text-[12px] sm:grid-cols-4">
            <Fact label="Playing" value={detail ? detail.players.toLocaleString() : "…"} />
            {/* A paid game says so before the Play button, not after the client opens. */}
            <Fact
              label="Access"
              value={
                detail == null
                  ? "…"
                  : detail.price
                    ? `${detail.price} Robux to play`
                    : "Free"
              }
            />
            <Fact label="Visits" value={detail ? detail.visits.toLocaleString() : "…"} />
            <Fact
              label="Rating"
              value={ratio === null ? "no votes" : `${ratio}% from ${total.toLocaleString()}`}
            />
            <Fact label="Max players" value={detail ? String(detail.maxPlayers) : "…"} />
          </dl>
        </div>
      </div>

      <div className="mt-6 flex items-center gap-1.5">
        {([
          ["about", "About"],
          ["servers", "Servers"],
        ] as const).map(([value, label]) => (
          <button
            key={value}
            type="button"
            onClick={() => {
              setTab(value);
              if (value === "servers") {
                void loadServers(detail?.place ?? target.place);
              }
            }}
            aria-pressed={tab === value}
            className={cn(
              "ui h-7 rounded-control border px-2.5 text-[11.5px]",
              tab === value
                ? "border-gold/50 bg-gold/10 text-gold"
                : "border-white/10 text-muted hover:text-body",
            )}
          >
            {label}
          </button>
        ))}
        {loading ? <span className="ml-2 text-[11px] text-muted">Reading</span> : null}
      </div>

      {tab === "about" ? (
        <section className="mt-3 rounded-card border border-white/8 bg-page/50 px-4 py-3.5">
          <p className="whitespace-pre-line text-[12.5px] leading-[1.6] text-body/90">
            {detail?.description?.trim() || "This game has no description."}
          </p>
          <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1 border-t border-white/6 pt-2.5 text-[11px] text-muted">
            {detail?.genre ? <span>{detail.genre}</span> : null}
            {detail?.updated ? <span>updated {detail.updated.slice(0, 10)}</span> : null}
            {detail ? <span>{ago(detail.fetched)}</span> : null}
          </div>
        </section>
      ) : null}

      {tab === "servers" ? (
        <section className="mt-3 overflow-hidden rounded-card border border-white/8 bg-page/50">
          <div className="flex items-baseline justify-between gap-3 border-b border-white/6 px-3.5 py-2.5">
            <span className="text-[11px] text-muted">
              Public servers, lowest ping first. Roblox allows three of these a minute, so this
              is asked for once and kept for two.
            </span>
            <button
              type="button"
              onClick={() => void loadServers(detail?.place ?? target.place)}
              disabled={serversLoading}
              className="ui h-6 rounded-control border border-white/10 px-2 text-[11px] text-muted hover:text-body disabled:opacity-40"
            >
              {serversLoading ? "Reading" : "Ask again"}
            </button>
          </div>

          {serversProblem ? (
            <div className="px-3.5 py-3 text-[12px] text-danger">{serversProblem.message}</div>
          ) : null}

          {servers?.list.map((server) => (
            <div
              key={server.id}
              className="flex items-center justify-between gap-3 border-b border-white/6 px-3.5 py-2.5 last:border-b-0"
            >
              <div className="min-w-0">
                <div className="num text-[12.5px] text-body">
                  {server.playing}/{server.maxPlayers} players
                </div>
                <div className="text-[11px] text-muted">
                  {server.ping > 0 ? `${server.ping} ms` : "ping unknown"}
                  {server.fps > 0 ? ` · ${Math.round(server.fps)} fps` : ""}
                </div>
              </div>
              <button
                type="button"
                onClick={() =>
                  prefill(
                    `roblox://placeId=${detail?.place ?? target.place}&gameInstanceId=${server.id}`,
                  )
                }
                className="ui h-7 shrink-0 rounded-control border border-gold/40 bg-gold/10 px-2.5 text-[11.5px] text-gold hover:bg-gold/15"
              >
                Play here
              </button>
            </div>
          ))}

          {servers && servers.list.length === 0 && !serversProblem ? (
            <div className="px-3.5 py-3 text-[12px] text-muted">
              Roblox lists no public servers for this place right now. A game can hide them.
            </div>
          ) : null}

          {!servers && !serversProblem ? (
            <div className="px-3.5 py-3 text-[12px] text-muted">Reading the list.</div>
          ) : null}
        </section>
      ) : null}
    </PageFrame>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-[10.5px] tracking-[0.06em] text-muted uppercase">{label}</dt>
      <dd className="num mt-0.5 text-[13px] text-body">{value}</dd>
    </div>
  );
}
