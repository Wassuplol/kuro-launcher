import { useEffect, useState } from "react";

import { api } from "../lib/ipc";
import type { StripState } from "../lib/types";

/**
 * The in-game strip.
 *
 * A window of its own, always on top, so it can be read while a game is in front of it.
 * Every number here comes from the client's own log: the place it joined, the server
 * that answered, and the clock the client keeps. Nothing reads the client process, and
 * nothing is sent to Roblox.
 *
 * The clock ticks locally between reads, because re-reading a log every second to move
 * a number by one would be the app spending a disk read on a digit.
 */

function clock(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;
  if (hours > 0) return `${hours}:${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
  return `${minutes}:${String(rest).padStart(2, "0")}`;
}

export function Strip() {
  const [state, setState] = useState<StripState | null>(null);
  const [seconds, setSeconds] = useState(0);

  useEffect(() => {
    let live = true;

    const poll = async () => {
      try {
        const next = await api.stripState();
        if (!live) return;
        setState(next);
        setSeconds(next.seconds);
      } catch {
        // The last reading stays on screen: a strip that blanks when one request
        // fails is worse than one that is a few seconds behind.
      }
    };

    void poll();
    const timer = window.setInterval(() => void poll(), 3000);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, []);

  const running = state?.running === true;
  const live = running && state?.place != null;

  useEffect(() => {
    if (!live) return;
    const timer = window.setInterval(() => setSeconds((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [live]);

  const title = state?.place != null ? (state.name ?? `Place ${state.place}`) : running ? "Client running" : "No session";
  const elapsed = live ? clock(seconds) : running ? "…" : "idle";
  // The join time is the only latency the log actually measures. A TCP probe against
  // the game port always timed out, because Roblox servers do not listen on it.
  const detail =
    state?.joinMs != null ? `joined in ${(state.joinMs / 1000).toFixed(1)} s` : (state?.note ?? null);

  return (
    <div
      data-tauri-drag-region
      className="flex h-full w-full items-stretch overflow-hidden rounded-[12px] border border-white/12 bg-panel"
    >
      <div data-tauri-drag-region className="flex min-w-0 flex-1 flex-col justify-center gap-1 px-3 py-2">
        <div data-tauri-drag-region className="flex items-baseline justify-between gap-3">
          <span className="truncate text-[12.5px] font-semibold text-body">{title}</span>
          <span className={live ? "num text-[15px] leading-none text-gold" : "num text-[15px] leading-none text-muted"}>
            {elapsed}
          </span>
        </div>

        <div data-tauri-drag-region className="flex items-center gap-2 text-[10.5px] leading-none text-muted">
          <span className={state?.joinMs != null ? "num shrink-0 text-body" : "truncate"}>
            {detail ?? (running ? "reading the log" : "this fills in while a game runs")}
          </span>
          {state?.endpoint ? <span className="num shrink-0">{state.endpoint}</span> : null}
          {running && state?.account ? (
            <span className="ml-auto shrink-0 truncate">{state.account}</span>
          ) : null}
        </div>
      </div>

      <button
        type="button"
        onClick={() => void api.stripClose()}
        aria-label="Close the strip"
        className="ui flex w-8 shrink-0 items-center justify-center border-l border-white/8 text-[13px] text-muted hover:bg-white/6 hover:text-body"
      >
        ×
      </button>
    </div>
  );
}
