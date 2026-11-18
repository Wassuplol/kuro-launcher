import { cn } from "../lib/cn";
import type { GameCard } from "../lib/types";
import { useShell } from "../store/shell";

/**
 * One game in a grid: the icon, the name, who is playing.
 *
 * Three facts, one click. Roblox's own tile is the same three, which is the
 * density bar this row is measured against; a bigger card with more chrome would
 * read as a landing page rather than a shelf.
 *
 * Clicking hands the place to the Launch page rather than spawning a client. The
 * launcher's whole point is that a launch is a decision with a preflight, and a
 * grid is where people mis-click.
 */
export function GameTile({ game, className }: { game: GameCard; className?: string }) {
  const prefill = useShell((state) => state.prefill);
  const name = game.name.trim();

  return (
    <button
      type="button"
      onClick={() => prefill(String(game.place))}
      title={name ? `Prepare a launch of ${name}` : "Prepare a launch"}
      className={cn("group block text-left", className)}
    >
      <div className="relative aspect-square overflow-hidden rounded-[6px] bg-raised ring-1 ring-white/8">
        {game.icon ? (
          <img
            src={game.icon}
            alt=""
            loading="lazy"
            decoding="async"
            className="h-full w-full object-cover"
          />
        ) : (
          <div className="h-full w-full" aria-hidden />
        )}
      </div>
      <div className="mt-1.5 truncate text-[12px] leading-[1.35] text-body group-hover:text-gold">
        {name || "Reading the name"}
      </div>
      <div className="truncate text-[11px] leading-[1.4] text-muted">
        {game.players > 0 ? `${game.players.toLocaleString()} playing` : "nobody playing"}
      </div>
    </button>
  );
}

/** A grid of tiles that never leaves a hole: fixed minimum, fluid above it. */
export function TileGrid({ children }: { children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(148px,1fr))] gap-x-3 gap-y-4">
      {children}
    </div>
  );
}
