import { Play } from "lucide-react";

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
 * The body opens the game page, the way roblox.com does. The pill in the corner
 * hands the place straight to the Launch page, so a launch is still one click for
 * anyone who already knows what they want, and neither action spawns a client.
 */
export function GameTile({ game, className }: { game: GameCard; className?: string }) {
  const prefill = useShell((state) => state.prefill);
  const openGame = useShell((state) => state.openGame);
  const name = game.name.trim();

  return (
    <div className={cn("group relative", className)}>
      <button
        type="button"
        onClick={() => openGame(game.universe, game.place)}
        title={name ? `Open ${name}` : "Open the game"}
        className="block w-full text-left"
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
        <div className="mt-1.5 flex items-baseline gap-1.5">
          <span className="min-w-0 truncate text-[12px] leading-[1.35] text-body group-hover:text-gold">
            {name || "Reading the name"}
          </span>
          {/* Paid access, said on the tile: meeting a paywall in the Roblox client
              because the launcher did not mention it is the thing this prevents. */}
          {game.price ? (
            <span
              className="num shrink-0 rounded-[3px] bg-gold/15 px-1 text-[10px] leading-[1.5] text-gold"
              title={`Paid access: ${game.price} Robux`}
            >
              R$ {game.price}
            </span>
          ) : null}
        </div>
        <div className="truncate text-[11px] leading-[1.4] text-muted">
          {game.players > 0 ? `${game.players.toLocaleString()} playing` : "nobody playing"}
        </div>
      </button>

      {/* Hover and focus only: a grid of permanent play buttons would compete with
          the tiles it is drawn on, and this is one click for people who want it. */}
      <button
        type="button"
        onClick={() => prefill(String(game.place))}
        title={name ? `Prepare a launch of ${name}` : "Prepare a launch"}
        className="ui absolute top-1.5 right-1.5 flex h-7 w-7 items-center justify-center rounded-control bg-page/85 text-body opacity-0 ring-1 ring-white/12 backdrop-blur-[2px] transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:bg-gold hover:text-page"
      >
        <Play size={13} strokeWidth={2.2} />
      </button>
    </div>
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
