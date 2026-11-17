import type { Transition } from "motion/react";

/** The single easing from the identity. Every transition uses it. */
export const EASE: [number, number, number, number] = [0.2, 0.8, 0.2, 1];

export const ENTER: Transition = { duration: 0.18, ease: EASE };
export const SETTLE: Transition = { duration: 0.26, ease: EASE };
