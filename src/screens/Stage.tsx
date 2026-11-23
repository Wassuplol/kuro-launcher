import { Plus, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { EmptyState } from "../components/EmptyState";
import { PageFrame } from "../components/PageFrame";
import { cn } from "../lib/cn";
import { api, CommandError, detailOf } from "../lib/ipc";
import type { StagePad, StagePlace, StageView, Squad } from "../lib/types";
import { useReducedMotion } from "../lib/useReducedMotion";
import { useShell } from "../store/shell";
import { useSocial } from "../store/social";

/**
 * The stage: one pad per account on this machine.
 *
 * This is the surface a website cannot have and a bootstrapper does not want. A pad is
 * not a row: it is the account's own avatar, rendered by Roblox right now, standing on a
 * pool of light that says whether that account is free. Pressing the bar hands the same
 * destination to every pad at once, which needs the multi-instance lock and says so
 * before it needs it.
 *
 * What a pad is not: a friend. A friend cannot be started by this machine, so friends
 * live in the strip below, where the one honest action is opening their exact server.
 */

type Problem = { message: string; detail?: string };

/**
 * A destination row is 300 pixels wide, and a game name matters more there than how
 * long ago it ran. The long form belongs in the Library; here it is two characters.
 */
function shortAgo(unix: number, now: number = Date.now()): string {
  if (!unix) return "";
  const minutes = Math.round(now / 1000 - unix) / 60;
  if (minutes < 60) return `${Math.max(1, Math.round(minutes))}m`;
  if (minutes < 60 * 24) return `${Math.round(minutes / 60)}h`;
  return `${Math.round(minutes / (60 * 24))}d`;
}

const EMPTY = new Set<number>();

export function StageScreen() {
  const go = useShell((state) => state.go);
  const openGame = useShell((state) => state.openGame);
  const prefill = useShell((state) => state.prefill);
  const settings = useShell((state) => state.settings);
  const setMulti = useShell((state) => state.setMultiInstance);

  const roster = useSocial((state) => state.roster);
  const loadRoster = useSocial((state) => state.load);

  const [view, setView] = useState<StageView | null>(null);
  const [places, setPlaces] = useState<StagePlace[]>([]);
  const [destination, setDestination] = useState<StagePlace | null>(null);
  const [loading, setLoading] = useState(true);
  const [problem, setProblem] = useState<Problem | null>(null);

  /** Accounts left off the stage. A pad is on it unless somebody took it off. */
  const [off, setOff] = useState<Set<number>>(EMPTY);
  const seeded = useRef(false);

  const [running, setRunning] = useState(false);
  const [report, setReport] = useState<Squad | null>(null);
  const [refused, setRefused] = useState<string | null>(null);

  const multi = settings?.launch.multiInstance ?? false;
  const reduced = useReducedMotion();
  const pads = useRef<HTMLDivElement>(null);

  const read = async () => {
    setLoading(true);
    try {
      const [stage, known] = await Promise.all([api.stageView(), api.stagePlaces()]);
      setView(stage);
      setPlaces(known);
      setProblem(null);

      // The newest place is the destination nobody has to choose. An empty list is a
      // real answer: this machine has never launched anything.
      setDestination((current) => current ?? known[0] ?? null);

      // A pad with a dead session cannot start, so it begins off the stage rather than
      // as a light that lies. Seeded once: a refresh must not undo a choice.
      if (!seeded.current) {
        seeded.current = true;
        setOff(new Set(stage.pads.filter((pad) => pad.health === "expired").map((pad) => pad.user)));
      }
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      setProblem({
        message: failure?.message ?? "The stage could not be read.",
        detail: failure ? detailOf(failure.code, failure.action) : undefined,
      });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void read();
    void loadRoster();
    // Read once on arrival. Presence is not on a timer anywhere in this app.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const everyPad = view?.pads ?? [];
  const onStage = useMemo(
    () => everyPad.filter((pad) => !off.has(pad.user)),
    [everyPad, off],
  );

  const toggle = (pad: StagePad) => {
    setReport(null);
    setRefused(null);
    setOff((current) => {
      const next = new Set(current);
      if (next.has(pad.user)) next.delete(pad.user);
      else next.add(pad.user);
      return next;
    });
  };

  // The pointer moves the figures a few pixels and nothing else. Motion 3: it answers
  // the hand, it does not loop.
  const track = (event: React.PointerEvent<HTMLDivElement>) => {
    if (reduced || !pads.current) return;
    const box = event.currentTarget.getBoundingClientRect();
    const x = ((event.clientX - box.left) / box.width - 0.5) * 12;
    const y = ((event.clientY - box.top) / box.height - 0.5) * 5;
    pads.current.style.setProperty("--px", x.toFixed(2));
    pads.current.style.setProperty("--py", y.toFixed(2));
  };

  const rest = () => {
    if (!pads.current) return;
    pads.current.style.setProperty("--px", "0");
    pads.current.style.setProperty("--py", "0");
  };

  const target = destination ? `roblox://placeId=${destination.place}` : null;
  const needsMulti = onStage.length > 1 && !multi;

  async function launch() {
    if (!target || running) return;
    setRunning(true);
    setRefused(null);
    try {
      const squad = await api.stageLaunch(
        target,
        onStage.map((pad) => pad.user),
      );
      setReport(squad);
      if (squad.started === squad.attempted) {
        toast.success(
          squad.attempted === 1
            ? `${squad.pads[0]?.display || "The pad"} started.`
            : `All ${squad.attempted} pads started.`,
          { description: destination?.name },
        );
      } else {
        toast.message(`${squad.started} of ${squad.attempted} pads started.`, {
          description: "The rest say why under their names.",
        });
      }
    } catch (value) {
      const failure = value instanceof CommandError ? value : null;
      setRefused(failure ? `${failure.message} ${failure.action ?? ""}`.trim() : "The launch did not start.");
    } finally {
      setRunning(false);
    }
  }

  const out = (user: number) => report?.pads.find((pad) => pad.account === user) ?? null;
  const sessionKnown = view?.presence === true;
  // Who could actually play right now. Five rows is what fits beside the destination
  // panel; the rest are one press away on Friends rather than eight rows of scroll here.
  const reachable = (roster?.friends ?? []).filter(
    (friend) => friend.presence.kind === 1 || friend.presence.kind === 2,
  );
  const free = [...reachable]
    .sort((left, right) => right.presence.kind - left.presence.kind)
    .slice(0, 5);
  const moreFree = reachable.length - free.length;

  // The one floating surface on this page: the bar that hands every pad the same
  // destination. It sits in the frame's footer, so it cannot fall below the fold.
  const bar = (
    <div className="mx-auto w-full max-w-[1184px] px-8 pt-2 pb-4">
        <div className="flex items-center justify-between gap-3 rounded-card border border-white/10 px-3.5 py-2.5">
          <div className="min-w-0">
            <div className="truncate text-[12.5px] font-semibold text-body">
              {destination ? destination.name : "No destination yet"}
            </div>
            <div
              className={cn(
                "mt-0.5 truncate text-[11px]",
                refused ? "text-danger" : report && report.started < report.attempted ? "text-gold" : "text-muted",
              )}
            >
              {refused
                ? refused
                : report
                  ? report.attempted === 1
                    ? report.started === 1
                      ? "Started."
                      : "The pad did not start. It says why below."
                    : `${report.started} of ${report.attempted} pads started.`
                  : destination
                    ? onStage.length > 1
                      ? `${onStage.length} pads, one server, started together.`
                      : onStage.length === 1
                        ? "One pad on the stage. Add a second account to run two at once."
                        : "No pad is on the stage, so nothing would start."
                    : "Pick a game the way you normally would; its name stands here."}
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-2">
            {destination ? (
              <button
                type="button"
                onClick={() => openGame(destination.universe, destination.place)}
                className="ui flex h-9 items-center rounded-control border border-white/10 px-3 text-[12px] text-muted hover:bg-white/5 hover:text-body"
              >
                Game page
              </button>
            ) : null}

            {onStage.length === 0 ? (
              <button
                type="button"
                onClick={() => setOff(new Set())}
                className="ui flex h-9 items-center rounded-control bg-gold px-4 text-[12.5px] font-medium text-page hover:brightness-105"
              >
                Put a pad back
              </button>
            ) : !destination ? (
              <button
                type="button"
                onClick={() => go("home")}
                className="ui flex h-9 items-center rounded-control bg-gold px-4 text-[12.5px] font-medium text-page hover:brightness-105"
              >
                Choose a game
              </button>
            ) : needsMulti ? (
              <button
                type="button"
                onClick={() => void setMulti(true)}
                className="ui flex h-9 items-center rounded-control bg-gold px-4 text-[12.5px] font-medium text-page hover:brightness-105"
              >
                Allow more than one game
              </button>
            ) : (
              <button
                type="button"
                onClick={() => void launch()}
                disabled={running}
                className="ui flex h-9 items-center rounded-control bg-gold px-4 text-[12.5px] font-medium text-page hover:brightness-105 disabled:opacity-60"
              >
                {running
                  ? onStage.length > 1
                    ? "Starting pads"
                    : "Starting"
                  : onStage.length > 1
                    ? `Launch ${onStage.length} pads`
                    : "Launch"}
              </button>
            )}
          </div>
        </div>
      </div>
  );

  return (
    <PageFrame
      wide
      footer={everyPad.length > 0 ? bar : null}
      title="Stage"
      subtitle="Every account on this machine, standing on its own pad. A lit pad runs when you press Launch, and every lit pad lands in the same server."
      aside={
        <div className="flex items-center gap-1.5">
          <span className="flex h-6 items-center gap-1.5 rounded-control border border-white/10 px-2 text-[11px] text-muted">
            <span
              aria-hidden
              className={cn("h-1 w-1 rounded-[1px]", onStage.length > 0 ? "bg-success" : "bg-white/25")}
            />
            {onStage.length} {onStage.length === 1 ? "pad" : "pads"}
          </span>
          <span className="flex h-6 items-center gap-1.5 rounded-control border border-white/10 px-2 text-[11px] text-muted">
            <span
              aria-hidden
              className={cn("h-1 w-1 rounded-[1px]", sessionKnown ? "bg-gold" : "bg-white/25")}
            />
            {sessionKnown ? "presence live" : "no presence"}
          </span>
          <button
            type="button"
            onClick={() => {
              void read();
              void loadRoster(true);
            }}
            disabled={loading}
            aria-label="Read the stage again"
            className="ui flex h-6 w-6 items-center justify-center rounded-control text-[var(--dim)] hover:bg-white/5 hover:text-body disabled:opacity-40"
          >
            <RefreshCw size={12} strokeWidth={1.7} className={loading ? "animate-spin" : undefined} />
          </button>
        </div>
      }
    >
      {problem ? (
        <div className="mt-4 rounded-card border border-danger/35 bg-danger-fill/10 px-4 py-3">
          <div className="text-[12.5px] text-danger">{problem.message}</div>
          {problem.detail ? <div className="mt-1 text-[11.5px] text-muted">{problem.detail}</div> : null}
        </div>
      ) : null}

      {view?.note ? (
        <div className="mt-4 rounded-card border border-white/10 bg-panel/60 px-4 py-2.5 text-[12px] text-muted">
          {view.note}
        </div>
      ) : null}

      {loading && !view ? <StageSkeleton /> : null}

      {view && everyPad.length === 0 ? (
        <div className="mt-10">
          <EmptyState
            mirror
            title="The stage is empty"
            body="A pad is an account this machine can start. Add one and it stands here with the avatar Roblox renders for it, and the pad says whether it is free."
          />
          <div className="mt-5 flex justify-center">
            <button
              type="button"
              onClick={() => go("accounts")}
              className="ui flex h-9 items-center rounded-control bg-gold px-4 text-[12.5px] font-medium text-page hover:brightness-105"
            >
              Add an account
            </button>
          </div>
        </div>
      ) : null}

      {view && everyPad.length > 0 ? (
        <>
          {/* The lit plane. One pool, one floor, and the figures standing between them. */}
          <div
            onPointerMove={track}
            onPointerLeave={rest}
            className="relative mt-2 overflow-hidden"
          >
            {/* The floor.
                A radial mask centred on its own box put the bright part of the plane
                below the container's clip, so the grid rendered and nobody could see it:
                measured, the left half of the stage was pure #0B0D12 at every sample. The
                mask now fades straight up from the container's bottom edge, which is the
                edge the perspective compresses onto. */}
            <div
              aria-hidden
              className="pointer-events-none absolute inset-x-0 bottom-0 h-[240px] opacity-70"
              style={{
                backgroundImage:
                  "repeating-linear-gradient(90deg, rgba(255,255,255,0.095) 0 1px, transparent 1px 84px), repeating-linear-gradient(0deg, rgba(255,255,255,0.07) 0 1px, transparent 1px 64px)",
                transform: "perspective(620px) rotateX(62deg)",
                transformOrigin: "50% 100%",
                maskImage: "linear-gradient(to top, #000 0%, rgba(0,0,0,0.8) 46%, transparent 100%)",
                WebkitMaskImage: "linear-gradient(to top, #000 0%, rgba(0,0,0,0.8) 46%, transparent 100%)",
              }}
            />
            <div
              aria-hidden
              className="pointer-events-none absolute inset-x-0 bottom-10 mx-auto h-[220px] w-[min(720px,96%)]"
              style={{
                background:
                  "radial-gradient(ellipse at 50% 58%, rgba(231,193,105,0.11), rgba(231,193,105,0.03) 46%, transparent 72%)",
              }}
            />

            {/* The composition holds a width even when the window does not: two pads
                drifting apart on a 1440 pixel floor read as an empty room, not a stage. */}
            <div
              ref={pads}
              className="relative mx-auto flex w-full max-w-[1120px] flex-wrap items-end justify-center gap-x-[clamp(10px,3vw,44px)] gap-y-6 px-1 pt-2 pb-7"
            >
              {everyPad.map((pad) => (
                <Pad
                  key={pad.user}
                  pad={pad}
                  lit={!off.has(pad.user)}
                  outcome={out(pad.user)}
                  onToggle={() => toggle(pad)}
                />
              ))}
              <AddPad onPress={() => go("accounts")} />
            </div>
          </div>

          <div className="mx-auto grid w-full max-w-[1120px] grid-cols-[minmax(0,1fr)_320px] gap-4 max-[880px]:grid-cols-1">
            <div className="surface flex min-w-0 flex-col px-3 pt-2.5 pb-2">
              <div className="px-1 text-[10px] font-semibold tracking-[0.16em] text-[var(--dim)]">
                WHO ELSE IS ON
              </div>
              {free.length > 0 ? (
                <div className="mt-1 flex flex-col divide-y divide-white/[0.06]">
                  {free.map((friend) => (
                    <button
                      key={friend.user}
                      type="button"
                      onClick={() =>
                        friend.presence.place
                          ? prefill(
                              friend.presence.server
                                ? `roblox://placeId=${friend.presence.place}&gameInstanceId=${friend.presence.server}`
                                : String(friend.presence.place),
                            )
                          : undefined
                      }
                      title={friend.presence.label}
                      className="ui flex h-9 items-center gap-2.5 px-1 text-left hover:bg-white/5"
                    >
                      <span className="relative h-6 w-6 shrink-0 overflow-hidden rounded-full bg-raised">
                        {friend.headshot ? (
                          <img src={friend.headshot} alt="" className="h-full w-full object-cover" />
                        ) : null}
                      </span>
                      <span className="min-w-0 flex-1 truncate text-[12px] text-body">
                        {friend.display || friend.name}
                      </span>
                      <span
                        className={cn(
                          "shrink-0 truncate text-[11px]",
                          friend.presence.kind === 2 ? "text-gold" : "text-[var(--dim)]",
                        )}
                        style={{ maxWidth: "42%" }}
                      >
                        {friend.presence.kind === 2 ? friend.presence.label : "Free"}
                      </span>
                    </button>
                  ))}
                  {moreFree > 0 ? (
                    <button
                      type="button"
                      onClick={() => go("friends")}
                      className="ui mt-0.5 flex h-7 items-center self-start rounded-control px-2 text-[11px] text-[var(--dim)] hover:bg-white/5 hover:text-body"
                    >
                      {`${moreFree} more on Friends`}
                    </button>
                  ) : null}
                </div>
              ) : (
                <p className="mt-1 px-1 py-1 text-[11.5px] leading-[1.55] text-[var(--dim)]">
                  {roster
                    ? "Nobody on your list is free right now."
                    : "The friend list needs a session. Add an account and who is on appears here."}
                </p>
              )}
            </div>

            <div className="surface flex flex-col px-3 pt-2.5 pb-2">
              <div className="text-[10px] font-semibold tracking-[0.16em] text-[var(--dim)]">DESTINATION</div>
              {places.length > 0 ? (
                <div className="mt-1 flex flex-col">
                  {places.slice(0, 4).map((place) => {
                    const chosen = destination?.place === place.place;
                    return (
                      <button
                        key={place.place}
                        type="button"
                        aria-pressed={chosen}
                        onClick={() => {
                          setDestination(place);
                          setReport(null);
                        }}
                        title={place.name}
                        className={cn(
                          "ui flex h-8 items-center justify-between gap-2 rounded-control px-2 text-left text-[11.5px]",
                          chosen ? "bg-raised text-body" : "text-muted hover:bg-white/5 hover:text-body",
                        )}
                      >
                        <span className="min-w-0 truncate">{place.name}</span>
                        <span className="num shrink-0 text-[10.5px] text-[var(--dim)]">{shortAgo(place.at)}</span>
                      </button>
                    );
                  })}
                </div>
              ) : (
                <p className="mt-1 px-2 py-1 text-[11.5px] leading-[1.55] text-[var(--dim)]">
                  Nothing launched from this machine yet. A game you open once is named here for good.
                </p>
              )}

              <div className="mt-auto flex items-center justify-between border-t border-white/8 px-2 pt-2 pb-1 text-[11.5px]">
                <span className="text-muted">Place</span>
                <span className="num text-body">{destination ? destination.place : "none"}</span>
              </div>
              <div className="flex items-center justify-between px-2 pb-1 text-[11.5px]">
                <span className="text-muted">Accounts</span>
                <span className="num text-body">
                  {onStage.length} of {everyPad.length}
                </span>
              </div>
              <div className="flex items-center justify-between px-2 pb-1 text-[11.5px]">
                <span className="text-muted">More than one game</span>
                <button
                  type="button"
                  onClick={() => void setMulti(!multi)}
                  aria-pressed={multi}
                  className={cn(
                    "ui rounded-control px-1.5 py-0.5 text-[11px]",
                    multi ? "text-success hover:bg-white/5" : "text-[var(--dim)] hover:bg-white/5 hover:text-body",
                  )}
                >
                  {multi ? "allowed" : "off"}
                </button>
              </div>
            </div>
          </div>

        </>
      ) : null}
    </PageFrame>
  );
}

/** One account: its figure, its pad, and what the last press did with it. */
function Pad({
  pad,
  lit,
  outcome,
  onToggle,
}: {
  pad: StagePad;
  lit: boolean;
  outcome: { ok: boolean; detail: string } | null;
  onToggle: () => void;
}) {
  const live = pad.presence.kind === 2;
  const ready = pad.health !== "expired";
  const state = !lit ? "off the stage" : live ? "in game" : ready ? "ready" : "sign in again";

  return (
    <figure className="m-0 flex w-[clamp(136px,17.5vw,232px)] flex-col items-center">
      <figcaption className="mb-1.5 min-h-[34px] text-center">
        <span className="block text-[12.5px] font-semibold tracking-[0.01em] text-body">
          {pad.display || pad.name}
        </span>
        <span
          className={cn(
            "mt-1 block text-[10px] font-semibold tracking-[0.16em] uppercase",
            !lit ? "text-[var(--dim)]" : live ? "text-gold" : ready ? "text-success" : "text-[var(--dim)]",
          )}
        >
          {state}
        </span>
      </figcaption>

      <button
        type="button"
        role="checkbox"
        aria-checked={lit}
        aria-label={`${pad.display || pad.name}, ${state}. ${lit ? "Take off the stage" : "Put on the stage"}`}
        onClick={onToggle}
        className="group relative flex w-full items-end justify-center rounded-card focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-gold"
        style={{ aspectRatio: "1 / 1.06" }}
      >
        <span
          aria-hidden
          className={cn(
            "absolute inset-x-[1%] bottom-[-3px] h-[26px] rounded-[50%] transition-opacity duration-200",
            lit ? "opacity-100" : "opacity-45",
          )}
          style={{
            background: lit
              ? live
                ? "radial-gradient(ellipse at 50% 45%, rgba(231,193,105,0.34), rgba(231,193,105,0.07) 52%, transparent 74%)"
                : "radial-gradient(ellipse at 50% 45%, rgba(143,217,182,0.32), rgba(143,217,182,0.07) 52%, transparent 74%)"
              : "radial-gradient(ellipse at 50% 45%, rgba(231,193,105,0.10), transparent 70%)",
            boxShadow: lit
              ? "inset 0 0 0 1px rgba(231,193,105,0.34), inset 0 -1px 0 rgba(231,193,105,0.16)"
              : "inset 0 0 0 1px rgba(231,193,105,0.14)",
          }}
        />

        {/* A cut-out with no contact shadow reads as hovering, whatever the geometry says. */}
        <span
          aria-hidden
          className="absolute inset-x-[24%] bottom-[3px] h-[9px] rounded-[50%]"
          style={{
            background: "radial-gradient(ellipse at 50% 50%, rgba(0,0,0,0.55), rgba(0,0,0,0.22) 58%, transparent 76%)",
            opacity: lit ? 1 : 0.5,
          }}
        />

        {pad.body ? (
          <img
            src={pad.body}
            alt=""
            draggable={false}
            className={cn(
              "relative bottom-[-3%] w-[94%] transition-[filter,opacity] duration-200",
              lit ? "opacity-100" : "opacity-35 grayscale",
            )}
            style={{
              transform: "translate3d(calc(var(--px, 0) * 1px), calc(var(--py, 0) * 1px), 0)",
              filter: "drop-shadow(0 10px 12px rgba(0,0,0,0.6)) drop-shadow(0 26px 24px rgba(0,0,0,0.42))",
              animation: live && lit ? "breathe 7s ease-in-out infinite" : undefined,
            }}
          />
        ) : (
          <span className="relative mb-4 flex h-16 w-16 items-center justify-center rounded-full border border-white/10 text-[11px] text-[var(--dim)]">
            {pad.name.slice(0, 3)}
          </span>
        )}

        <span
          aria-hidden
          className={cn(
            "absolute inset-x-0 bottom-0 h-px transition-opacity duration-200",
            lit ? "opacity-100" : "opacity-0",
          )}
          style={{
            background:
              "linear-gradient(90deg, transparent, rgba(231,193,105,0.45), transparent)",
          }}
        />
      </button>

      <div className="mt-1.5 h-3.5 text-center text-[10.5px]">
        {outcome ? (
          <span
            className={cn(outcome.ok ? "text-success" : "text-danger")}
            title={outcome.detail}
          >
            {outcome.ok ? "started" : "stopped"}
          </span>
        ) : null}
      </div>
    </figure>
  );
}

function AddPad({ onPress }: { onPress: () => void }) {
  return (
    <figure className="m-0 flex w-[clamp(136px,17.5vw,232px)] flex-col items-center">
      <figcaption className="mb-1.5 min-h-[34px] text-center">
        <span className="block text-[12.5px] font-semibold text-muted">Open pad</span>
        <span className="mt-1 block text-[10px] font-semibold tracking-[0.16em] text-[var(--dim)] uppercase">
          add an account
        </span>
      </figcaption>
      <button
        type="button"
        onClick={onPress}
        aria-label="Add an account to this machine, which gives it a pad"
        className="group relative flex w-full items-end justify-center rounded-card"
        style={{ aspectRatio: "1 / 1.06" }}
      >
        <span
          aria-hidden
          className="ui absolute inset-x-[1%] bottom-[-3px] h-[26px] rounded-[50%] border border-dashed border-deep-gold/55 group-hover:border-gold"
          style={{ background: "radial-gradient(ellipse at 50% 45%, rgba(231,193,105,0.07), transparent 72%)" }}
        />
        <span className="ui mb-[9px] flex h-9 w-9 items-center justify-center rounded-full bg-gold/[0.06] text-deep-gold group-hover:bg-gold/10 group-hover:text-gold">
          <Plus size={24} strokeWidth={1.4} />
        </span>
      </button>
      <div className="mt-1.5 h-3.5" />
    </figure>
  );
}

function StageSkeleton() {
  return (
    <div className="mt-6 flex items-end justify-center gap-6" aria-hidden>
      {[0, 1, 2].map((index) => (
        <div key={index} className="flex w-[clamp(136px,17.5vw,232px)] flex-col items-center">
          <div className="mb-2 h-3 w-16 rounded-sm bg-white/6" />
          <div
            className="w-full animate-pulse rounded-card bg-white/[0.03]"
            style={{ aspectRatio: "1 / 1.06", animationDelay: `${index * 90}ms` }}
          />
        </div>
      ))}
    </div>
  );
}
