import { cn } from "../lib/cn";

import restArt from "../assets/mascot/atrestaiko.webp";

type Props = {
  title: string;
  body: string;
  hint?: string;
  art?: boolean;
  mirror?: boolean;
  level?: 1 | 2;
  spread?: boolean;
};

/**
 * A page with nothing on it still has to say why. The mascot appears on empty
 * states only, once per screen, never beside a failure.
 */
export function EmptyState({
  title,
  body,
  hint,
  art = true,
  mirror = false,
  level = 1,
  spread = false,
}: Props) {
  const Heading = level === 1 ? "h1" : "h2";

  return (
    <div
      className={cn(
        "flex items-center gap-7",
        spread && "w-full justify-between gap-10",
        mirror && "flex-row-reverse",
      )}
    >
      <div className="max-w-[440px]">
        <Heading
          className={cn(
            "font-semibold tracking-[-0.01em] text-body",
            level === 1 ? "text-[20px]" : "text-[16px]",
          )}
        >
          {title}
        </Heading>
        <p className="mt-2 text-[13px] leading-[1.65] text-muted">{body}</p>
        {hint ? <p className="mt-4 text-[12px] leading-[1.6] text-muted">{hint}</p> : null}
      </div>
      {art ? (
        <img
          src={restArt}
          alt=""
          draggable={false}
          className="h-[208px] w-auto shrink-0 select-none opacity-95"
        />
      ) : null}
    </div>
  );
}
