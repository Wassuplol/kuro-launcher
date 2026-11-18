import { animate } from "animejs";
import gsap from "gsap";
import { useCallback, useEffect, useRef, useState } from "react";

import { CrystalStage } from "../components/CrystalStage";
import { api } from "../lib/ipc";
import { greeting } from "../lib/format";
import { useReducedMotion } from "../lib/useReducedMotion";
import { useShell } from "../store/shell";
import { useVersions } from "../store/versions";

import welcomeArt from "../assets/mascot/aikowelcome.webp";

/**
 * The opening. It replaces the old 900 ms intro rather than following it: two
 * entrances in a row is one too many, so the wordmark, the gold hairline and the
 * mascot all live here now, with the room they were missing.
 *
 * It is also the app's preload. The three things it reports are three things the
 * app actually needs, and they happen here so the versions screen is already
 * warm when it is first opened.
 *
 * Ownership stays as the stack rules require: anime.js draws the SVG ring,
 * GSAP runs the entrance timeline, and CSS transitions the one thing that
 * changes after the entrance (the progress fill and the status line).
 */

/** How long the scene takes at minimum, so it never flashes on a fast machine. */
const SCENE = 2000;
/** Past this, the scene stops waiting for the network and hands over anyway. */
const PATIENCE = 4200;

type Stage = "settings" | "versions" | "live";

const STEPS: { stage: Stage; label: string }[] = [
  { stage: "settings", label: "Reading your settings" },
  { stage: "versions", label: "Finding your Roblox clients" },
  { stage: "live", label: "Checking what Roblox is serving" },
];

type Props = {
  /** The size the app will end up at. */
  target: { width: number; height: number };
  /** Called once the window has finished growing and the app is in place. */
  onDone: () => void;
};

export function Opening({ target, onDone }: Props) {
  const reduced = useReducedMotion();
  const ready = useShell((state) => state.ready);
  const name = useShell((state) => state.settings?.identity.name ?? "");
  const version = useShell((state) => state.info?.version ?? "");

  const root = useRef<HTMLDivElement>(null);
  const ring = useRef<SVGSVGElement>(null);
  const [stage, setStage] = useState<Stage>("settings");
  const [done, setDone] = useState(0);
  const [leaving, setLeaving] = useState(false);

  const finished = useRef(false);
  // The clock starts at the first painted frame, not at mount. The page mounts
  // while the window is still hidden, and starting the timer there quietly ate
  // a third of the scene before anyone could see it.
  const startedAt = useRef(0);
  const workDone = useRef(false);

  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      startedAt.current = performance.now();
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  const finish = useCallback(() => {
    if (finished.current) return;
    finished.current = true;

    setLeaving(true);

    // The window grows while the scene is still on screen, so the app is
    // revealed out of the loader rather than swapped in after it.
    void api
      .windowGrow(target.width, target.height, reduced ? 0 : 720)
      .catch(() => undefined)
      .then(() => {
        window.setTimeout(onDone, reduced ? 0 : 60);
      });
  }, [onDone, reduced, target.height, target.width]);

  // The work, one named step at a time. A failure is not fatal here: the app can
  // run without a version answer, and the versions screen says so itself.
  useEffect(() => {
    if (!ready) return;
    let alive = true;

    void (async () => {
      await Promise.resolve();

      if (alive) {
        setStage("versions");
        setDone(1);
      }
      await useVersions.getState().prepareInstalled();

      if (alive) {
        setStage("live");
        setDone(2);
      }
      await useVersions.getState().prepareLive();

      if (alive) {
        setDone(3);
        workDone.current = true;
      }
    })();

    return () => {
      alive = false;
    };
  }, [ready]);

  // Skip. One key, or one real click, from half a second in.
  //
  // A pointerdown listener looked right and was wrong: this app's window is
  // shown by the page, and the activation that follows delivers a pointerdown
  // that nobody performed. It skipped the scene at 424 ms every single time,
  // which is how it was found. A click is only dispatched after a genuine press
  // and release over the window, so that is what is listened for.
  useEffect(() => {
    if (reduced) return;
    let armed = false;
    const timer = window.setTimeout(() => {
      armed = true;
    }, 500);

    const skip = () => {
      if (armed) finish();
    };

    const onClick = (event: MouseEvent) => {
      if (event.button === 0 && event.detail > 0) skip();
    };

    window.addEventListener("keydown", skip);
    window.addEventListener("click", onClick);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("keydown", skip);
      window.removeEventListener("click", onClick);
    };
  }, [finish, reduced]);

  // Hand over when the work is done and the scene has had its time, or when
  // patience runs out, whichever comes first.
  useEffect(() => {
    if (!ready) return;

    const check = () => {
      if (startedAt.current === 0) return;
      const elapsed = performance.now() - startedAt.current;
      if ((workDone.current && elapsed >= SCENE) || elapsed >= PATIENCE) {
        finish();
      }
    };

    const timer = window.setInterval(check, 50);
    return () => window.clearInterval(timer);
  }, [finish, ready]);

  // The ring draws itself once. anime.js owns SVG and nothing else does.
  useEffect(() => {
    const svg = ring.current;
    const circle = svg?.querySelector("circle");
    if (!svg || !circle || reduced) return;

    const length = circle.getTotalLength();
    circle.style.strokeDasharray = `${length}`;
    circle.style.strokeDashoffset = `${length}`;

    const animation = animate(circle, {
      strokeDashoffset: 0,
      duration: 820,
      ease: "outCubic",
      onComplete: () => {
        // Cleared so a later render starts from a clean circle rather than a
        // dash pattern that happens to look closed.
        circle.style.strokeDasharray = "";
        circle.style.strokeDashoffset = "";
      },
    });

    return () => {
      animation.pause();
    };
  }, [reduced]);

  // The entrance. Entrance only: nothing here loops, nothing floats while the
  // person waits.
  useEffect(() => {
    const scope = root.current;
    if (!scope) return;

    const context = gsap.context(() => {
      if (reduced) {
        gsap.set([".open-art", ".open-mark", ".open-name", ".open-track"], {
          opacity: 1,
          y: 0,
          scale: 1,
          scaleX: 1,
        });
        return;
      }

      gsap
        .timeline()
        .fromTo(
          ".open-art",
          { opacity: 0, scale: 0.92 },
          { opacity: 1, scale: 1, duration: 0.62, ease: "power2.out" },
          0.08,
        )
        .fromTo(
          ".open-mark",
          { opacity: 0, y: 12 },
          { opacity: 1, y: 0, duration: 0.44, stagger: 0.03, ease: "power2.out" },
          0.36,
        )
        .fromTo(
          ".open-name",
          { opacity: 0, y: 5 },
          { opacity: 1, y: 0, duration: 0.34, ease: "power2.out" },
          0.74,
        )
        .fromTo(
          ".open-track",
          { scaleX: 0 },
          { scaleX: 1, duration: 0.58, ease: "power2.out" },
          0.7,
        );
    }, scope);

    return () => context.revert();
  }, [reduced]);

  const label = STEPS.find((step) => step.stage === stage)?.label ?? "Ready";
  const progress = done / STEPS.length;
  // The shard forms as the work completes, and is always whole by the time the
  // window starts growing: the scene never runs long for decoration's sake.
  const form = Math.min(1, 0.25 + progress * 0.75);
  const greetingText = name ? `${greeting()}, ${name}` : version ? `v${version}` : "Welcome";

  return (
    <div
      ref={root}
      className={`absolute inset-0 z-10 flex flex-col items-center justify-center bg-page ${
        leaving && !reduced ? "pointer-events-none" : ""
      }`}
      style={
        leaving && !reduced
          ? { transition: "opacity 320ms cubic-bezier(0.2,0.8,0.2,1)", opacity: 0 }
          : undefined
      }
    >
      {reduced ? null : (
        <>
          <CrystalStage form={form} exit={leaving ? 1 : 0} />
          {/* A scrim between the shader and the words: the crystal is allowed to be
              bright, the type is not allowed to be unreadable. */}
          <div
            aria-hidden
            className="absolute inset-0"
            style={{
              // A vignette only. An earlier version also dimmed the centre, which
              // is exactly where the shard is, and it turned a lit crystal into a
              // dark watermark. The type sits over the shard's calm middle band.
              background:
                "radial-gradient(120% 95% at 50% 40%, rgba(8,10,16,0) 34%, rgba(8,10,16,0.55) 74%, rgba(8,10,16,0.92) 100%)",
            }}
          />
        </>
      )}

      <div
        className="open-art relative flex h-[116px] w-[116px] items-center justify-center"
        style={{ opacity: reduced ? 1 : undefined }}
      >
        <svg
          ref={ring}
          viewBox="0 0 116 116"
          className="absolute inset-0 h-full w-full -rotate-90"
          aria-hidden
        >
          <circle cx="58" cy="58" r="55" fill="none" stroke="#B8873C" strokeWidth="1.25" />
        </svg>
        <div className="rounded-full bg-page p-[3px]">
          <img
            src={welcomeArt}
            alt=""
            draggable={false}
            className="h-[100px] w-[100px] rounded-full object-cover object-[50%_22%] select-none"
          />
        </div>
      </div>

      <div className="mt-5 flex flex-col items-center overflow-hidden">
        <div className="flex text-[24px] leading-none font-medium tracking-[0.18em] text-body">
          {"KURO".split("").map((letter, index) => (
            <span key={index} className="open-mark" style={{ opacity: reduced ? 1 : undefined }}>
              {letter}
            </span>
          ))}
        </div>
      </div>

      <div
        className="open-name mt-2 h-[16px] text-[12px] text-body/90"
        style={{ opacity: reduced ? 1 : undefined }}
      >
        {greetingText}
      </div>

      <div className="mt-6 w-[240px]">
        <div className="relative h-[2px] w-full overflow-hidden rounded-full">
          <div
            className="open-track absolute inset-0 origin-left rounded-full bg-white/16"
            style={{ transform: reduced ? "scaleX(1)" : undefined }}
          />
          <div
            className="absolute inset-y-0 left-0 rounded-full bg-gold"
            style={{
              width: `${Math.round(progress * 100)}%`,
              transition: reduced ? undefined : "width 420ms cubic-bezier(0.2,0.8,0.2,1)",
            }}
          />
        </div>

        <div
          className="mt-2.5 h-[15px] text-center text-[11px] tracking-[0.01em] text-muted"
          aria-live="polite"
        >
          {label}
        </div>
      </div>
    </div>
  );
}
