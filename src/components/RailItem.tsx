import * as Tooltip from "@radix-ui/react-tooltip";
import type { LucideIcon } from "lucide-react";
import { useEffect, useRef } from "react";

import { cn } from "../lib/cn";
import { revealIcon } from "../lib/revealIcon";
import { useReducedMotion } from "../lib/useReducedMotion";

type Props = {
  label: string;
  shortcut?: string;
  icon: LucideIcon;
  active?: boolean;
  onClick: () => void;
};

export function RailItem({ label, shortcut, icon: Icon, active, onClick }: Props) {
  const iconRef = useRef<SVGSVGElement>(null);
  const reduced = useReducedMotion();

  // anime.js draws the icon when its page becomes the live one.
  useEffect(() => {
    if (!active || reduced || !iconRef.current) return;
    revealIcon(iconRef.current);
  }, [active, reduced]);

  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-current={active ? "page" : undefined}
          onClick={onClick}
          className={cn(
            "ui relative flex h-11 w-11 items-center justify-center rounded-control",
            active ? "bg-white/5 text-gold" : "text-muted hover:bg-white/5 hover:text-body",
          )}
        >
          {active ? (
            <span
              aria-hidden
              className="absolute -left-2 top-1/2 h-4 w-[2px] -translate-y-1/2 rounded-r-full bg-gold"
            />
          ) : null}
          <Icon ref={iconRef} size={20} strokeWidth={1.6} />
        </button>
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content
          side="right"
          sideOffset={10}
          className="z-[70] flex items-center gap-2 rounded-control border border-white/10 bg-raised px-2.5 py-1.5 text-[12px] text-body shadow-lg"
        >
          {label}
          {shortcut ? (
            <span className="num text-[11px] text-muted">{shortcut}</span>
          ) : null}
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}
