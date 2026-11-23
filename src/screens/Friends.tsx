import { useEffect, useMemo, useState } from "react";
import { BadgeCheck, RefreshCw, Search } from "lucide-react";

import { EmptyState } from "../components/EmptyState";
import { PageFrame } from "../components/PageFrame";
import { ago } from "../lib/format";
import { api } from "../lib/ipc";
import { useShell } from "../store/shell";
import { useSocial } from "../store/social";

/**
 * Friends: who is on, what they are in, and one click into the same server.
 *
 * The join string is the interesting part. Presence publishes the exact server id
 * (`gameId`) beside the place, so a friend in a game can be joined where they
 * actually are rather than in a fresh server of the same place. That is the whole
 * reason this page can do something the website's friend list cannot.
 *
 * Everything on screen is real presence, and the page says when it was read instead
 * of implying it is live: twenty calls a minute is the tightest budget in the app, so
 * the list is cached and only presence is refreshed.
 */
export function FriendsScreen() {
  const roster = useSocial((state) => state.roster);
  const loading = useSocial((state) => state.loading);
  const problem = useSocial((state) => state.problem);
  const load = useSocial((state) => state.load);

  const prefill = useShell((state) => state.prefill);
  const go = useShell((state) => state.go);

  const [query, setQuery] = useState("");

  useEffect(() => {
    void load();
  }, [load]);

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const friends = roster?.friends ?? [];
    if (!needle) return friends;
    return friends.filter(
      (friend) =>
        friend.display.toLowerCase().includes(needle) ||
        friend.name.toLowerCase().includes(needle) ||
        friend.presence.label.toLowerCase().includes(needle),
    );
  }, [roster, query]);

  const missingAccount = problem?.detail?.startsWith("session_expired") ?? false;

  return (
    <PageFrame
      title="Friends"
      subtitle="Who is on, what they are in, and the same server they are in. The list needs an account; presence does not."
      aside={
        roster ? (
          <button
            type="button"
            onClick={() => void load(true)}
            disabled={loading}
            className="ui flex h-8 items-center gap-1.5 rounded-control border border-white/10 px-2.5 text-[11.5px] text-muted hover:text-body disabled:opacity-40"
          >
            <RefreshCw size={13} strokeWidth={1.6} className={loading ? "animate-spin" : undefined} />
            {loading ? "Reading" : "Refresh"}
          </button>
        ) : null
      }
    >
      {missingAccount ? (
        <div className="mt-6">
          <EmptyState
            mirror
            title="Friends need an account"
            body="The friend list is per account and Roblox refuses it to anyone who is not signed in. Add an account and this page fills."
          />
          <div className="mt-4 flex justify-center">
            <button
              type="button"
              onClick={() => go("accounts")}
              className="ui flex h-8 items-center rounded-control bg-gold px-3 text-[12px] font-medium text-page"
            >
              Open Accounts
            </button>
          </div>
        </div>
      ) : null}

      {problem && !missingAccount ? (
        <div className="mt-5 rounded-card border border-danger/35 bg-danger-fill/10 px-4 py-3">
          <div className="text-[12.5px] text-danger">{problem.message}</div>
          {problem.detail ? (
            <div className="mt-1 text-[11.5px] text-muted">{problem.detail}</div>
          ) : null}
        </div>
      ) : null}

      {loading && !roster ? <RosterSkeleton /> : null}

      {roster ? (
        <>
          <div className="mt-6 flex flex-wrap items-baseline justify-between gap-3">
            <div className="flex items-baseline gap-2 text-[11.5px] text-muted">
              <span className={roster.inGame > 0 ? "text-success" : undefined}>
                {roster.inGame} in a game
              </span>
              <span aria-hidden>·</span>
              <span>{roster.online} online</span>
              <span aria-hidden>·</span>
              <span>{roster.offline} offline</span>
              {query.trim() ? (
                <>
                  <span aria-hidden>·</span>
                  <span>
                    {shown.length} of {roster.friends.length}
                  </span>
                </>
              ) : null}
            </div>

            {roster.friends.length > 8 ? (
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
                  placeholder="Filter by name or game"
                  aria-label="Filter your friends"
                  className="h-8 w-[230px] rounded-control border border-white/10 bg-panel pr-2.5 pl-7 text-[12px] text-body outline-none placeholder:text-muted/70 focus:border-gold/60"
                />
              </div>
            ) : null}
          </div>

          {roster.friends.length === 0 ? (
            <EmptyState
              mirror
              title="No friends on this account"
              body="The list came back empty. Friendships are made on roblox.com and arrive here on the next read."
            />
          ) : null}

          {shown.length > 0 ? (
            <ul className="mt-3">
              {shown.map((friend) => (
                <li
                  key={friend.user}
                  className="flex items-center gap-3 border-b border-white/6 py-2 last:border-b-0"
                >
                  <span className="relative h-7 w-7 shrink-0">
                    {friend.headshot ? (
                      <img
                        src={friend.headshot}
                        alt=""
                        loading="lazy"
                        decoding="async"
                        className="h-7 w-7 rounded-full object-cover ring-1 ring-white/10"
                      />
                    ) : (
                      <span aria-hidden className="block h-7 w-7 rounded-full bg-raised" />
                    )}
                    {/* In a game is a solid dot; online but not playing is the same
                        green at half strength, so the two states are not told apart by
                        reading the status line. */}
                    {friend.presence.kind === 2 || friend.presence.kind === 1 ? (
                      <span
                        aria-hidden
                        className={`absolute -right-0.5 -bottom-0.5 h-2 w-2 rounded-full ring-2 ring-page ${
                          friend.presence.kind === 2 ? "bg-success" : "bg-success/45"
                        }`}
                      />
                    ) : null}
                  </span>

                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      <button
                        type="button"
                        onClick={() => void api.openSite(`https://www.roblox.com/users/${friend.user}/profile`)}
                        className="truncate text-[12.5px] text-body hover:text-gold"
                        title={`Open ${friend.display} on roblox.com`}
                      >
                        {friend.display}
                      </button>
                      {friend.verified ? (
                        <BadgeCheck size={12} strokeWidth={1.8} className="shrink-0 text-gold" aria-label="verified" />
                      ) : null}
                      {friend.name ? (
                        <span className="truncate text-[11px] text-muted">@{friend.name}</span>
                      ) : null}
                    </div>
                    <div
                      className={`truncate text-[11px] ${
                        friend.presence.kind === 2 ? "text-body/80" : "text-muted"
                      }`}
                    >
                      {friend.presence.label}
                    </div>
                  </div>

                  {/* Only for someone in a game: a Join that cannot work is a dead
                      control, and this page does not have any. */}
                  {friend.presence.kind === 2 && friend.presence.place ? (
                    <button
                      type="button"
                      onClick={() =>
                        prefill(
                          `roblox://placeId=${friend.presence.place}${
                            friend.presence.server
                              ? `&gameInstanceId=${friend.presence.server}`
                              : ""
                          }`,
                        )
                      }
                      className="ui shrink-0 rounded-control border border-white/10 px-2.5 py-1 text-[11.5px] text-muted hover:border-gold/40 hover:text-gold"
                      title="Prepare a launch into their server"
                    >
                      Join
                    </button>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : null}

          {roster.friends.length > 0 && shown.length === 0 ? (
            <p className="mt-3 text-[12.5px] text-muted">
              Nobody on this list matches "{query.trim()}".
            </p>
          ) : null}

          {roster.note ? <p className="mt-3 text-[11.5px] text-muted">{roster.note}</p> : null}

          <div className="mt-4 text-[11px] text-muted">
            Read {ago(roster.fetched)}
            {roster.cached ? " (the list came from the cache; presence is always fresh)" : ""}. The
            friend list allows twenty calls a minute, which is the tightest budget in the app.
          </div>
        </>
      ) : null}
    </PageFrame>
  );
}

function RosterSkeleton() {
  return (
    <div className="mt-6" aria-hidden>
      {Array.from({ length: 10 }).map((_, index) => (
        <div key={index} className="flex items-center gap-3 border-b border-white/6 py-2">
          <div className="h-7 w-7 shrink-0 rounded-full bg-white/5" />
          <div className="flex-1">
            <div className="h-2.5 w-40 rounded bg-white/5" />
            <div className="mt-1.5 h-2 w-24 rounded bg-white/5" />
          </div>
        </div>
      ))}
    </div>
  );
}
