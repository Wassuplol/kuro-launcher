import { useEffect, useRef, type ReactNode } from "react";

import { cn } from "../lib/cn";

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
  const scroller = useRef<HTMLDivElement>(null);

  // A page opens at its top. WebView2 restores the scroll offset of a scroller it has
  // seen before, so a second visit could land halfway down a page whose heading is
  // fixed above content that had already been scrolled past.
  useEffect(() => {
    scroller.current?.scrollTo({ top: 0 });
  }, []);

  return (
    <div className="flex h-full flex-col">
      <header
        className={cn(
          "relative shrink-0 pt-7 pb-5 pl-8",
          // The page scroller reserves a scrollbar gutter, so the header has to
          // reserve the same one or the centred column lands five pixels off.
          wide ? "pr-8" : "pr-[42px]",
        )}
      >
        <div className={wide ? "" : "mx-auto w-full max-w-[820px]"}>
          <div className="flex items-start justify-between gap-6">
            <div className="min-w-0">
              <h1 className="text-[26px] leading-[1.1] font-semibold tracking-[-0.022em] text-body">
                {title}
              </h1>
              {subtitle ? (
                <p className="mt-2 max-w-[62ch] text-[12.5px] leading-[1.6] text-muted">
                  {subtitle}
                </p>
              ) : null}
            </div>
            {aside ? <div className="shrink-0 pt-1">{aside}</div> : null}
          </div>
        </div>
        {/* A rule that fades out instead of a border that runs edge to edge: the
            page then has no box around it, which is the whole point of the veil. */}
        <span
          aria-hidden
          className={cn(
            "absolute bottom-0 h-px bg-gradient-to-r from-white/10 via-white/5 to-transparent",
            wide ? "inset-x-8" : "left-1/2 w-full max-w-[820px] -translate-x-1/2",
          )}
        />
      </header>

      {/* Anchor off: a feed that fills in after mount would otherwise drag the
          viewport down as thumbnails arrive, which reads as the page scrolling
          itself. */}
      <div
        ref={scroller}
        className="min-h-0 flex-1 overflow-y-auto px-8 pb-16 [overflow-anchor:none] [scrollbar-gutter:stable]"
      >
        <div className={wide ? "w-full" : "mx-auto w-full max-w-[820px]"}>{children}</div>
      </div>
    </div>
  );
}
