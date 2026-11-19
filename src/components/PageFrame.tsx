import type { ReactNode } from "react";

/**
 * The chrome for a page that lives in the main area.
 *
 * Drawers are for things you open while looking at something else: settings,
 * the palette. A tool with its own job gets this instead, so the rail can take
 * you to it and the window keeps the whole width for the work.
 */
export function PageFrame({
  title,
  subtitle,
  aside,
  wide = false,
  children,
}: {
  title: string;
  subtitle?: string;
  aside?: ReactNode;
  /** A page of tiles takes the whole window; a page of forms stays a readable column. */
  wide?: boolean;
  children: ReactNode;
}) {
  return (
    <div className="flex h-full flex-col">
      <header className="shrink-0 border-b border-white/8 px-8 pt-6 pb-5">
        <div className="flex items-start justify-between gap-6">
          <div className="min-w-0">
            <h1 className="text-[20px] font-semibold tracking-[-0.015em] text-body">{title}</h1>
            {subtitle ? (
              <p className="mt-1 max-w-[64ch] text-[12.5px] leading-[1.55] text-muted">
                {subtitle}
              </p>
            ) : null}
          </div>
          {aside ? <div className="shrink-0">{aside}</div> : null}
        </div>
      </header>

      {/* Anchor off: a feed that fills in after mount would otherwise drag the
          viewport down as thumbnails arrive, which reads as the page scrolling
          itself. */}
      <div className="min-h-0 flex-1 overflow-y-auto px-8 pb-14 [overflow-anchor:none]">
        <div className={wide ? "w-full" : "mx-auto w-full max-w-[820px]"}>{children}</div>
      </div>
    </div>
  );
}
