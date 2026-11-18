import { Section } from "./Panel";
import { span, stamp } from "../lib/format";
import type { Ledger } from "../lib/types";

/**
 * The ledger: totals that outlive the log folder.
 *
 * Roblox keeps a handful of logs and deletes the rest, so a total read from that folder
 * shrinks on its own. This card reads the store that every scan folds into, and says
 * plainly when it started: a number without its coverage is a claim, not a record.
 */
export function LedgerCard({ ledger }: { ledger: Ledger }) {
  if (ledger.all.sessions === 0) return null;

  const cells = [
    { label: `time in game in ${ledger.year}`, value: span(ledger.thisYear.seconds) },
    {
      label: ledger.thisYear.sessions === 1 ? "session" : "sessions",
      value: String(ledger.thisYear.sessions),
    },
    {
      label: ledger.thisYear.places === 1 ? "place" : "places",
      value: String(ledger.thisYear.places),
    },
    {
      label: ledger.thisYear.days === 1 ? "day played" : "days played",
      value: String(ledger.thisYear.days),
    },
  ];

  const most = ledger.yearTop[0] ?? null;

  return (
    <Section title="The ledger">
      <div className="grid grid-cols-4 border-b border-white/8">
        {cells.map((cell, index) => (
          <div key={cell.label} className={index > 0 ? "border-l border-white/8 px-4 py-3" : "px-4 py-3"}>
            <div className="num text-[19px] leading-none text-body">{cell.value}</div>
            <div className="label mt-2">{cell.label}</div>
          </div>
        ))}
      </div>

      <dl className="px-3.5 py-2.5 text-[11.5px]">
        {ledger.longest ? (
          <Fact label="Longest session">
            <span className="num">{span(ledger.longest.seconds)}</span>
            <span className="text-muted">
              {" · "}
              {ledger.longest.name ?? (ledger.longest.place ? `Place ${ledger.longest.place}` : "no place")}
              {" · "}
              {stamp(ledger.longest.started)}
            </span>
          </Fact>
        ) : null}
        {ledger.busiest ? (
          <Fact label="Busiest day">
            <span className="num">{ledger.busiest.date}</span>
            <span className="num text-muted"> · {span(ledger.busiest.seconds)}</span>
          </Fact>
        ) : null}
        {most ? (
          <Fact label="Most played">
            <span>{most.name ?? `Place ${most.place}`}</span>
            <span className="num text-muted">
              {" · "}
              {span(most.seconds)} over {most.sessions} session{most.sessions === 1 ? "" : "s"}
            </span>
          </Fact>
        ) : null}
      </dl>

      <p className="border-t border-white/6 px-3.5 py-2 text-[11px] leading-[1.55] text-muted">
        {ledger.first
          ? `Counting since ${stamp(ledger.first)}: ${span(ledger.all.seconds)} over ${ledger.all.sessions} session${ledger.all.sessions === 1 ? "" : "s"} and ${ledger.all.places} place${ledger.all.places === 1 ? "" : "s"}. Roblox deletes old logs, so this is the only copy.`
          : "Nothing recorded yet."}
      </p>
    </Section>
  );
}

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-3 py-[3px]">
      <dt className="w-[104px] shrink-0 text-muted">{label}</dt>
      <dd className="min-w-0 flex-1 truncate text-body">{children}</dd>
    </div>
  );
}
