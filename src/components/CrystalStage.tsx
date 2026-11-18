import { useEffect, useRef } from "react";

import { mountCrystal, type CrystalHandle } from "../lib/crystal";

/**
 * The shader's host. A canvas and three setters: the intro owns the timeline, this
 * owns nothing but the GPU surface, so the opening's clocks stay the only clocks.
 *
 * `window.__kuroCrystal` is exposed on purpose. The frame rate and the resolution
 * the adaptive scaler settled on are the two numbers that decide whether this is
 * allowed to ship, and a screenshot cannot show either.
 */
export function CrystalStage({ form, exit }: { form: number; exit: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const handle = useRef<CrystalHandle | null>(null);

  useEffect(() => {
    if (!canvas.current) return;
    const mounted = mountCrystal(canvas.current);
    handle.current = mounted;
    if (mounted) {
      (window as unknown as { __kuroCrystal?: CrystalHandle }).__kuroCrystal = mounted;
    }
    return () => {
      mounted?.dispose();
      handle.current = null;
      delete (window as unknown as { __kuroCrystal?: CrystalHandle }).__kuroCrystal;
    };
  }, []);

  useEffect(() => {
    handle.current?.setForm(form);
  }, [form]);

  useEffect(() => {
    handle.current?.setExit(exit);
  }, [exit]);

  return <canvas ref={canvas} aria-hidden className="pointer-events-none absolute inset-0 h-full w-full" />;
}
