import { ChevronRight, ExternalLink, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";

import { EmptyState } from "../components/EmptyState";
import { Badge, Section } from "../components/Panel";
import { cn } from "../lib/cn";
import { bytes, span, stamp } from "../lib/format";
import { api, CommandError, failureDetail } from "../lib/ipc";
import type { Receipt } from "../lib/types";

/**
 * Sessions: what this machine actually ran.
 *
 * The page is a reader, not a recorder. Roblox writes a log per launch and the
 * client is talkative, so the facts here are quoted from its own file: the join
 * line, the handshake, the adapter it picked, the shutdown stage. Nothing is
 * inferred from the website, and a log that stops early says so instead of being
 * dressed up as a crash.
 */

const OUTCOME: Record<Receipt["outcome"], { label: string; tone: "gold" | "success" | "muted" | "danger" }> = {
  left: { label: "Played", tone: "success" },
  ended: { label: "Ended", tone: "muted" },
  no_handshake: { label: "No server", tone: "gold" },
  never_joined: { label: "No join", tone: "muted" },
  crashed: { label: "Crash", tone: "danger" },
};

type Filter = "all" | "reached" | "missed";

type Totals = {
  count: number;
  seconds: number;
  reached: number;
  missed: number;
};

function summarize(receipts: Receipt[]): Totals {
  return receipts.reduce<Totals>(
    (sum, receipt) => {
      sum.count += 1;
      if (receipt.handshake) {
        sum.reached += 1;
        sum.seconds += receipt.seconds;
      } else {
        sum.missed += 1;
      }
      return sum;
    },
    { count: 0, seconds: 0, reached: 0, missed: 0 },
  );
}

export function SessionsBody() {
  const [receipts, setReceipts] = useState<Receipt[] | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState<Filter>("all");

  const load = useCallback(async () => {
    setBusy(true);
    setProblem(null);
    try {
      const list = await api.receiptsScan();
      setReceipts(list);

      // A place id is honest and unreadable. The names come from a cache that only
      // asks Roblox once per place, and the page is already painted when it does.
      const missing = Array.from(
        new Set(list.filter((receipt) => receipt.place && !receipt.name).map((receipt) => receipt.place as number)),
      );
      if (missing.length > 0) {
        const names = await api.receiptNames(missing);
        setReceipts((current) =>
          (current ?? []).map((receipt) => {
            const name = receipt.place ? names[String(receipt.place)] : undefined;
            return receipt.name || !name ? receipt : { ...receipt, name };
          }),
        );
      }
    } catch (error) {
      setProblem(error instanceof CommandError ? (failureDetail(error) ?? error.message) : String(error));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const totals = useMemo(() => summarize(receipts ?? []), [receipts]);

  const shown = useMemo(() => {
    const list = receipts ?? [];
    if (filter === "reached") return list.filter((receipt) => receipt.handshake);
    if (filter === "missed") return list.filter((receipt) => !receipt.handshake);
    return list;
  }, [receipts, filter]);

  if (problem) {
    return (
      <div className="mt-5 rounded-card border border-danger/30 bg-danger-fill/10 p-3.5">
        <p className="text-[12.5px] leading-[1.55] text-danger">{problem}</p>
        <button
          type="button"
          onClick={() => void load()}
          className="ui mt-2.5 h-7 rounded-control border border-danger/40 px-2.5 text-[12px] text-danger hover:bg-danger-fill/20"
        >
          Try again
        </button>
      </div>
    );
  }

  if (!receipts) {
    return (
      <p className="mt-5 text-[12.5px] text-muted">
        {busy ? "Reading this machine's client logs." : "Nothing read yet."}
      </p>
    );
  }

  if (receipts.length === 0) {
    return (
      <div className="mt-10">
        <EmptyState
          title="No client logs yet"
          body="Roblox writes a log every time the client starts. Launch something from here and this page fills with what actually happened."
        />
      </div>
    );
  }

  return (
    <>
      <Summary totals={totals} />
      <Machine receipts={receipts} />
      <Played receipts={receipts} />

      <Section
        title="Launches on this machine"
        aside={
          <div className="flex items-center gap-1">
            <Segmented value={filter} onChange={setFilter} totals={totals} />
            <button
              type="button"
              onClick={() => void load()}
              disabled={busy}
              aria-label="Read the logs again"
              className="ui -mt-1 flex h-6 items-center gap-1.5 rounded-control px-2 text-[11px] text-muted hover:bg-white/5 hover:text-body disabled:opacity-50"
            >
              <RefreshCw size={12} strokeWidth={1.8} className={cn(busy && "animate-spin")} />
              {busy ? "Reading" : "Refresh"}
            </button>
          </div>
        }
      >
        {shown.length === 0 ? (
          <p className="px-3.5 py-4 text-[12px] text-muted">
            {filter === "missed"
              ? "Every launch here reached a server."
              : filter === "reached"
                ? "No launch here reached a server."
                : "Nothing to show."}
          </p>
        ) : (
          shown.map((receipt) => <ReceiptRow key={receipt.id} receipt={receipt} />)
        )}
      </Section>

      <p className="mt-4 text-[11px] leading-[1.6] text-muted">
        Read from {totals.count} log{totals.count === 1 ? "" : "s"} in Roblox's own log folder. Roblox
        keeps the recent ones and deletes the rest, so an empty week is the folder, not the app.
      </p>
    </>
  );
}

/**
 * Which GPU the client actually ran on, said in one line under the totals.
 *
 * The client enumerates every adapter and then names the one it chose, so this is a
 * reading rather than a guess. When the chosen card is not the largest one on the
 * machine, that is worth saying plainly: Windows decides per program.
 */
function Machine({ receipts }: { receipts: Receipt[] }) {
  const newest = receipts.find((receipt) => receipt.adapters.length > 0);
  if (!newest) return null;

  const chosen = newest.adapters.find((adapter) => adapter.chosen) ?? null;
  const others = newest.adapters.filter((adapter) => !adapter.chosen && adapter.vramMb);
  const biggest = newest.adapters.reduce<Receipt["adapters"][number] | null>(
    (best, adapter) => ((adapter.vramMb ?? 0) > (best?.vramMb ?? 0) ? adapter : best),
    null,
  );
  const wrong = chosen && biggest && chosen.name !== biggest.name;

  return (
    <div className="mt-3">
      <p className="flex flex-wrap items-baseline gap-x-2 text-[11.5px] text-muted">
        <span className="label">Graphics</span>
        <span className="text-body">{chosen?.name ?? "not recorded"}</span>
        {chosen?.vramMb ? <span className="num">{Math.round(chosen.vramMb / 1024)} GB</span> : null}
        {newest.api ? <span className="num">{newest.api}</span> : null}
        {others.length > 0 ? (
          <span className="text-muted">
            · also present: {others.map((adapter) => adapter.name).join(", ")}
          </span>
        ) : null}
      </p>
      {wrong ? (
        <p className="mt-1 text-[11.5px] text-gold">
          Windows sent the client to {chosen?.name}, not the larger {biggest?.name}. That is a
          per-program Windows setting, not a Roblox flag.
        </p>
      ) : null}
    </div>
  );
}

/**
 * What the logs add up to: the games with the most time in them, and the last seven
 * days as bars. Both come from the receipts already in hand, so nothing here needs a
 * request and nothing is claimed beyond the window the logs still cover.
 */
function Played({ receipts }: { receipts: Receipt[] }) {
  const played = receipts.filter((receipt) => receipt.handshake && receipt.place);

  const top = useMemo(() => {
    const byPlace = new Map<number, { name: string; seconds: number; sessions: number }>();
    for (const receipt of played) {
      const place = receipt.place as number;
      const entry = byPlace.get(place) ?? {
        name: receipt.name ?? `Place ${place}`,
        seconds: 0,
        sessions: 0,
      };
      entry.seconds += receipt.seconds;
      entry.sessions += 1;
      byPlace.set(place, entry);
    }
    return Array.from(byPlace.values())
      .sort((a, b) => b.seconds - a.seconds)
      .slice(0, 3);
  }, [receipts]);

  const days = useMemo(() => {
    const midnight = new Date();
    midnight.setHours(0, 0, 0, 0);
    const buckets = Array.from({ length: 7 }, (_, index) => {
      const start = new Date(midnight);
      start.setDate(start.getDate() - (6 - index));
      return { start, seconds: 0 };
    });
    for (const receipt of played) {
      const at = new Date(receipt.started * 1000);
      const slot = buckets.findIndex((bucket) => {
        const next = new Date(bucket.start);
        next.setDate(next.getDate() + 1);
        return at >= bucket.start && at < next;
      });
      if (slot >= 0) {
        const bucket = buckets[slot];
        if (bucket) bucket.seconds += receipt.seconds;
      }
    }
    return buckets;
  }, [receipts]);

  if (top.length === 0) return null;

  const peak = Math.max(...days.map((day) => day.seconds), 1);
  const today = new Date().toDateString();

  return (
    <div className="mt-3 rounded-card border border-white/8 bg-page/50">
      <div className="grid grid-cols-[1fr_auto] gap-8 px-4 py-3">
        <div className="min-w-0">
          <div className="label">Most played</div>
          <div className="mt-2 space-y-1.5">
            {top.map((entry) => (
              <div key={entry.name} className="flex items-baseline gap-3 text-[11.5px]">
                <span className="truncate text-body">{entry.name}</span>
                <span className="num ml-auto shrink-0 text-[11px] text-muted">
                  {entry.sessions} session{entry.sessions === 1 ? "" : "s"}
                </span>
                <span className="num w-[52px] shrink-0 text-right text-body">{span(entry.seconds)}</span>
              </div>
            ))}
          </div>
        </div>

        <div className="shrink-0 border-l border-white/8 pl-7">
          <div className="label text-right">Last 7 days</div>
          {/* The baseline is drawn for every day, so a quiet week reads as a quiet
              week rather than as a chart that failed to render. */}
          <div className="mt-2 flex items-end gap-2 border-b border-white/10 pb-1">
            {days.map((day) => {
              const height = day.seconds > 0 ? Math.max(5, Math.round((day.seconds / peak) * 30)) : 3;
              const isToday = day.start.toDateString() === today;
              return (
                <span
                  key={day.start.toISOString()}
                  title={`${day.start.toLocaleDateString(undefined, { weekday: "long" })}: ${span(day.seconds)}`}
                  className={cn("w-[13px] rounded-[2px]", isToday ? "bg-white/55" : day.seconds > 0 ? "bg-white/28" : "bg-white/12")}
                  style={{ height }}
                />
              );
            })}
          </div>
          <div className="mt-1 flex gap-2">
            {days.map((day) => (
              <span
                key={day.start.toISOString()}
                className="num w-[13px] text-center text-[8.5px] leading-none text-muted"
              >
                {day.start.toLocaleDateString(undefined, { weekday: "short" })}
              </span>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function Summary({ totals }: { totals: Totals }) {
  const cells = [
    { label: "launches", value: String(totals.count) },
    { label: "time in game", value: span(totals.seconds) },
    { label: "reached a server", value: String(totals.reached) },
    { label: "did not", value: String(totals.missed) },
  ];

  return (
    <div className="mt-5 grid grid-cols-4 overflow-hidden rounded-card border border-white/8 bg-page/50">
      {cells.map((cell, index) => (
        <div key={cell.label} className={cn("px-4 py-3", index > 0 && "border-l border-white/8")}>
          <div className="num text-[19px] leading-none text-body">{cell.value}</div>
          <div className="label mt-2">{cell.label}</div>
        </div>
      ))}
    </div>
  );
}

function Segmented({
  value,
  onChange,
  totals,
}: {
  value: Filter;
  onChange: (next: Filter) => void;
  totals: Totals;
}) {
  const options: { id: Filter; label: string }[] = [
    { id: "all", label: `All ${totals.count}` },
    { id: "reached", label: `Reached ${totals.reached}` },
    { id: "missed", label: `Did not ${totals.missed}` },
  ];

  return (
    <div role="group" aria-label="Filter launches" className="mr-1 flex items-center gap-0.5 rounded-control border border-white/8 p-0.5">
      {options.map((option) => (
        <button
          key={option.id}
          type="button"
          aria-pressed={value === option.id}
          onClick={() => onChange(option.id)}
          className={cn(
            "ui h-5 rounded-[3px] px-2 text-[10.5px]",
            value === option.id ? "bg-white/8 text-body" : "text-muted hover:text-body",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function ReceiptRow({ receipt }: { receipt: Receipt }) {
  const [open, setOpen] = useState(false);
  const tone = OUTCOME[receipt.outcome];
  const title = receipt.name ?? (receipt.place ? `Place ${receipt.place}` : "No place asked for");

  return (
    <div className="border-b border-white/6 last:border-b-0">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
        className="ui flex w-full items-start gap-3 px-3.5 py-2.5 text-left hover:bg-white/[0.025]"
      >
        <ChevronRight
          size={13}
          strokeWidth={1.8}
          className={cn("mt-[3px] shrink-0 text-muted transition-transform duration-150", open && "rotate-90")}
        />

        <span className="grid min-w-0 flex-1 grid-cols-[1fr_auto] items-baseline gap-x-4 gap-y-0.5">
          <span className="flex min-w-0 items-baseline gap-2">
            <span className="truncate text-[13px] text-body">{title}</span>
            <Badge tone={tone.tone}>{tone.label}</Badge>
          </span>
          <span className="num shrink-0 text-[12px] text-body">
            {receipt.handshake ? span(receipt.seconds) : "no session"}
          </span>

          <span className="truncate text-[11.5px] leading-[1.5] text-muted">{receipt.verdict}</span>
          <span className="num shrink-0 text-[11px] text-muted">{stamp(receipt.started)}</span>
        </span>
      </button>

      {open ? <Details receipt={receipt} /> : null}
    </div>
  );
}

function Details({ receipt }: { receipt: Receipt }) {
  const open = async () => {
    try {
      await api.receiptsOpen(receipt.logPath);
    } catch {
      // The row stays open with the path visible; opening is a convenience.
    }
  };

  return (
    <div className="border-t border-white/6 bg-black/15 px-3.5 py-3 pl-[42px]">
      <dl className="text-[11.5px]">
        <Fact label="Started">{stamp(receipt.started)}</Fact>
        <Fact label="Client">{receipt.version}</Fact>
        {receipt.place ? (
          <Fact label="Place">
            <span className="num">{receipt.place}</span>
            {receipt.universe ? <span className="num text-muted"> · universe {receipt.universe}</span> : null}
          </Fact>
        ) : null}
        {receipt.server ? (
          <Fact label="Server">
            <span className="num">{receipt.server}</span>
            {receipt.job ? (
              <span className="ml-2 truncate text-muted" title={receipt.job}>
                {receipt.job}
              </span>
            ) : null}
          </Fact>
        ) : null}
        {receipt.joinSeconds !== null ? (
          <Fact label="Join took">
            <span className="num">{span(receipt.joinSeconds)}</span>
            <span className="text-muted"> from process start to the handshake</span>
          </Fact>
        ) : null}
        {receipt.adapter ? (
          <Fact label="Graphics">
            {receipt.adapter}
            {receipt.vramMb ? <span className="num text-muted"> · {Math.round(receipt.vramMb / 1024)} GB</span> : null}
            {receipt.api ? <span className="num text-muted"> · {receipt.api}</span> : null}
          </Fact>
        ) : null}
        <Fact label="Ended">
          {receipt.shutdownStage ? (
            <span className="text-muted">shut down at stage {receipt.shutdownStage}</span>
          ) : (
            <span className="text-muted">no shutdown line in this log</span>
          )}
        </Fact>
        <Fact label="Requests">
          {receipt.httpErrors === 0 ? (
            <span className="text-muted">none failed</span>
          ) : (
            <span>
              <span className="num">{receipt.httpErrors}</span> failed
              {receipt.lastHttp ? <span className="num text-muted"> · last {receipt.lastHttp}</span> : null}
            </span>
          )}
        </Fact>
        <Fact label="Log">
          <span className="num truncate text-muted">{receipt.id}.log</span>
          <span className="num text-muted">
            {" "}
            · {receipt.lines} lines · {bytes(receipt.bytes)}
          </span>
        </Fact>
      </dl>

      <div className="mt-2.5 flex items-center gap-3">
        <button
          type="button"
          onClick={() => void open()}
          className="ui flex h-6 items-center gap-1.5 rounded-control border border-white/10 px-2 text-[11px] text-muted hover:border-white/20 hover:text-body"
        >
          <ExternalLink size={11} strokeWidth={1.8} />
          Open the log
        </button>
        {receipt.action ? <span className="text-[11px] text-gold">{receipt.action}</span> : null}
      </div>
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex gap-3 py-[3px]">
      <dt className="w-[76px] shrink-0 text-muted">{label}</dt>
      <dd className="min-w-0 flex-1 truncate text-body">{children}</dd>
    </div>
  );
}
