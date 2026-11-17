import { animate, stagger } from "animejs";

/**
 * Draws an icon's strokes in when its page becomes active. anime.js owns SVG
 * only, per the stack rules: `motion` keeps the DOM transitions and the two
 * libraries never touch the same node.
 *
 * The dash attributes are removed on completion so later renders start clean.
 */
const DURATION = 260;

export function revealIcon(svg: SVGSVGElement): void {
  const shapes = Array.from(
    svg.querySelectorAll<SVGGeometryElement>("path, line, circle, rect, polyline, polygon"),
  ).filter((shape) => {
    let length = 0;
    try {
      length = shape.getTotalLength();
    } catch {
      return false;
    }
    if (!length) return false;
    shape.style.strokeDasharray = `${length} ${length}`;
    shape.style.strokeDashoffset = `${length}`;
    return true;
  });

  if (shapes.length === 0) return;

  animate(shapes, {
    strokeDashoffset: 0,
    duration: DURATION,
    delay: stagger(18),
    ease: "outQuad",
    onComplete: () => {
      for (const shape of shapes) {
        shape.style.strokeDasharray = "";
        shape.style.strokeDashoffset = "";
      }
    },
  });
}
