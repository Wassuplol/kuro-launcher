import { useEffect, useState } from "react";

import { api } from "../lib/ipc";

/**
 * The account's Robux, in the title bar.
 *
 * It is read once when the app opens and again when it is clicked, because a balance
 * that is polled is a private request nobody asked for. An expired session shows a dash
 * and says why in the tooltip: a zero and an unknown are different numbers, and the
 * launcher has no business inventing the first.
 */
export function RobuxPill() {
  const [robux, setRobux] = useState<number | null>(null);
  const [why, setWhy] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const read = async () => {
    setBusy(true);
    try {
      const balance = await api.robux();
      setRobux(balance.robux);
      setWhy(null);
    } catch (error) {
      setRobux(null);
      setWhy(error instanceof Error ? error.message : "The balance could not be read.");
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void read();
  }, []);

  const label = robux === null ? "-" : robux.toLocaleString();

  return (
    <button
      type="button"
      onClick={() => void read()}
      title={
        why ??
        (robux === null ? "Robux, reading" : `Robux on this account. Click to read again.`)
      }
      className="ui my-auto mr-2 flex h-6 items-center gap-1.5 rounded-control border border-white/8 px-2 text-[11px] text-muted hover:border-white/16 hover:text-body"
    >
      <span className={busy ? "num text-muted" : "num text-gold"}>R$</span>
      <span className="num text-body">{label}</span>
    </button>
  );
}
