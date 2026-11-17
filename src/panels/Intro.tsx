import gsap from "gsap";
import { useCallback, useEffect, useRef, useState } from "react";

import { useShell } from "../store/shell";
import { useReducedMotion } from "../lib/useReducedMotion";

import welcomeArt from "../assets/mascot/aikowelcome.webp";

type Props = { onDone: () => void };

/**
 * Covers work that is already happening, and adds no delay of its own.
 * 900 ms of timeline, skippable from 400 ms, once per process.
 */
export function Intro({ onDone }: Props) {
  const reduced = useReducedMotion();
  const version = useShell((state) => state.info?.version ?? "");
  const root = useRef<HTMLDivElement>(null);
  const finished = useRef(false);
  const [skippable, setSkippable] = useState(false);

  const doneRef = useRef(onDone);
  doneRef.current = onDone;

  const finish = useCallback(() => {
    if (finished.current) return;
    finished.current = true;
    doneRef.current();
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => setSkippable(true), 400);
    return () => window.clearTimeout(timer);
  }, []);

  useEffect(() => {
    if (!skippable) return;
    const skip = () => finish();
    window.addEventListener("keydown", skip);
    window.addEventListener("pointerdown", skip);
    return () => {
      window.removeEventListener("keydown", skip);
      window.removeEventListener("pointerdown", skip);
    };
  }, [skippable, finish]);

  useEffect(() => {
    const scope = root.current;
    if (!scope) return;

    const context = gsap.context(() => {
      if (reduced) {
        gsap.set([".intro-rule", ".intro-mark", ".intro-art"], {
          opacity: 1,
          x: 0,
          y: 0,
          scaleX: 1,
        });
        gsap.delayedCall(0.5, finish);
        return;
      }

      const timeline = gsap.timeline({ onComplete: finish });
      timeline
        .fromTo(".intro-rule", { scaleX: 0 }, { scaleX: 1, duration: 0.26, ease: "power2.out" }, 0)
        .fromTo(
          ".intro-mark",
          { opacity: 0, y: 6 },
          { opacity: 1, y: 0, duration: 0.18, stagger: 0.024, ease: "power2.out" },
          0.06,
        )
        .fromTo(
          ".intro-art",
          { opacity: 0, x: 18 },
          { opacity: 1, x: 0, duration: 0.26, ease: "power2.out" },
          0.18,
        )
        .to({}, { duration: 0.46 });
    }, scope);

    return () => context.revert();
  }, [reduced, finish]);

  return (
    <div
      ref={root}
      className="fixed inset-0 z-[60] grid grid-cols-[1fr_auto] items-center gap-10 bg-page pr-[6vw] pl-[8vw]"
    >
      <div>
        <div className="intro-rule h-px w-[140px] origin-left bg-deep-gold" />
        <div className="mt-6 overflow-hidden">
          <div className="intro-mark text-[40px] leading-none font-medium tracking-[0.14em] text-body">
            KURO
          </div>
        </div>
        <div className="intro-mark num mt-4 text-[11px] text-muted">{version}</div>
      </div>
      <img
        src={welcomeArt}
        alt=""
        draggable={false}
        className="intro-art h-[64vh] max-h-[460px] w-auto self-end select-none"
      />
    </div>
  );
}
