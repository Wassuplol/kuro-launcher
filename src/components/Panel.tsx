import { X } from "lucide-react";
import { motion } from "motion/react";
import type { ReactNode } from "react";

import { cn } from "../lib/cn";
import { SETTLE } from "../lib/motion";

/**
 * The one drawer shape in the app. Settings and Versions are the same surface
 * with different contents, so they share the frame rather than each growing
 * their own header, width and close button.
 */
export function PanelFrame({
  title,
  label,
  onClose,
  children,
}: {
  title: string;
  /** Accessible name, when the visible title is not enough on its own. */
  label?: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <div className="fixed inset-0 z-40">
      <div className="absolute inset-0 bg-page/70" onClick={onClose} aria-hidden />

      <motion.aside
        role="dialog"
        aria-label={label ?? title}
        initial={{ x: 24, opacity: 0 }}
        animate={{ x: 0, opacity: 1 }}
        transition={SETTLE}
        className="absolute top-0 right-0 flex h-full w-[540px] max-w-[92vw] flex-col border-l border-white/8 bg-panel"
      >
        <header className="flex h-11 shrink-0 items-center justify-between border-b border-white/8 pr-2 pl-5">
          <h1 className="text-[13px] font-semibold text-body">{title}</h1>
          <button
            type="button"
            onClick={onClose}
            aria-label={`Close ${title.toLowerCase()}`}
            className="ui flex h-8 w-8 items-center justify-center rounded-control text-muted hover:bg-white/6 hover:text-body"
          >
            <X size={15} strokeWidth={1.6} />
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-12">{children}</div>
      </motion.aside>
    </div>
  );
}

export function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="mt-7 first:mt-5">
      <div className="flex items-baseline justify-between gap-3">
        <h2 className="label">{title}</h2>
        {aside}
      </div>
      <div className="surface mt-2 overflow-hidden">{children}</div>
    </section>
  );
}

export function Row({
  label,
  hint,
  children,
  className,
}: {
  label: ReactNode;
  hint?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex min-h-[46px] items-center justify-between gap-4 border-b border-white/6 px-3.5 py-2.5 last:border-b-0",
        className,
      )}
    >
      <div className="min-w-0">
        <div className="text-[13px] text-body">{label}</div>
        {hint ? <div className="mt-0.5 text-[11px] leading-[1.5] text-muted">{hint}</div> : null}
      </div>
      {children}
    </div>
  );
}

/** A short state word on a row. Colour carries meaning, never decoration. */
export function Badge({
  children,
  tone = "muted",
}: {
  children: ReactNode;
  tone?: "gold" | "success" | "muted" | "danger";
}) {
  const tones = {
    gold: "border-gold/40 bg-gold/10 text-gold",
    success: "border-success/35 bg-success/10 text-success",
    muted: "border-white/12 text-muted",
    danger: "border-danger/35 bg-danger-fill/10 text-danger",
  } as const;

  return (
    <span
      className={cn(
        "ui shrink-0 rounded-[4px] border px-1.5 py-px text-[10px] font-semibold tracking-[0.04em] uppercase",
        tones[tone],
      )}
    >
      {children}
    </span>
  );
}
