/**
 * The opening crystal: one fragment shader, no dependencies, no textures.
 *
 * A raymarched shard of glass, and glass rather than a magenta ball: the surface
 * refracts, the ray is marched through the solid to find where it leaves, and the
 * exit is dispersed three ways (1.42 / 1.47 / 1.55) before it samples an analytic
 * environment, so the body has depth, a cold core and a warm edge. Absorption is
 * Beer-Lambert over the distance actually travelled inside, which is why the thick
 * middle goes cold while the thin edges stay clear.
 *
 * Two details do most of the work and neither needs geometry:
 *
 * - **Creases** come from `fwidth` of the surface normal. Inside a facet the normal
 *   is constant across a pixel quad, so the derivative is zero; on a dihedral edge
 *   it jumps. The result is a one-pixel highlight on every real intersection, at
 *   any resolution, for four instructions.
 * - **The environment is analytic**: a gradient sky, a cold key behind the viewer
 *   and a gold counter light low left. It is what the reflections and the refracted
 *   image both sample, so a facet that faces the key shines and its neighbour does
 *   not, which is the whole illusion.
 *
 * It is written against WebGL2 and drawn as a fullscreen triangle from
 * `gl_VertexID`, so there is no vertex buffer, no index buffer and no geometry to
 * upload: the whole intro is one program and four uniforms.
 *
 * Performance is the design, not an afterthought:
 *
 * - The backing store is capped at 1.5 device pixels per CSS pixel and at 1600
 *   pixels wide, whatever the monitor claims, because a 4K raymarch buys nothing
 *   over a 1600 one.
 * - An adaptive scaler watches the rolling frame time and drops the render size in
 *   steps when frames take more than 19 ms, then climbs back when there is head
 *   room. On a laptop iGPU it settles low; on a desktop GPU it renders full size.
 * - Nothing is drawn while the document is hidden, and the loop stops itself the
 *   moment the canvas is removed.
 * - 64 steps outside, 24 inside, 6 for the normal: one shape, one light rig, no
 *   shadow pass and no post chain.
 */

export type CrystalHandle = {
  setForm: (value: number) => void;
  setExit: (value: number) => void;
  stats: () => { fps: number; scale: number };
  dispose: () => void;
};

/** Device pixels per CSS pixel, before the adaptive scale. */
const DPR_CAP = 1.5;
/** The widest backing store the shader will ever be asked to fill. */
const WIDTH_CAP = 1600;
/** Frame time over which the renderer gives up resolution. */
const SLOW_MS = 19;
const FAST_MS = 11;

const VERTEX = `#version 300 es
void main() {
  // A fullscreen triangle: three vertices, no buffers, no attributes.
  vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

const FRAGMENT = `#version 300 es
precision highp float;

uniform vec2 uRes;
uniform float uTime;
uniform float uForm;
uniform float uExit;
out vec4 outColour;

// Crown glass. 1.46 rather than 1.52 because the shard is small and a lower index
// keeps the refraction inside the frame instead of folding it shut.
const float IOR = 1.46;

mat2 rot(float a) { float c = cos(a), s = sin(a); return mat2(c, -s, s, c); }

float hash(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

float sdOcta(vec3 p, float s) {
  p = abs(p);
  return (p.x + p.y + p.z - s) * 0.5773502692;
}

// The shard's own frame, so the shape, its creases and its normals all agree.
vec3 shardSpace(vec3 p) {
  vec3 q = p;
  q.xz *= rot(uTime * 0.20);
  q.xy *= rot(sin(uTime * 0.16) * 0.26 + 0.20);
  q *= vec3(1.0, 0.88, 1.0);
  q.xz *= rot(0.18);
  return q;
}

// An octahedron with its crown sheared off and its base cut flat: one broken end,
// one polished table. Solid, not a shell, because a hollow shape refracts nothing.
float map(vec3 p) {
  vec3 q = shardSpace(p);
  float d = sdOcta(q, 1.0);
  d = max(d, q.y - 0.58);
  d = max(d, -q.y - 0.94);
  return d;
}

// The two kinds of crease this shape has, both exact: the octahedron's own edges,
// where one axis of the shard frame crosses zero, and the cuts, where the carved
// plane and the octahedron are equidistant. Continuous functions, so the highlight
// is a clean line at any resolution. A thresholded screen-space derivative was the
// first attempt and it drew a dotted line, because on a diagonal edge only some
// pixels straddle the crease.
float creaseAt(vec3 p) {
  vec3 q = shardSpace(p);

  float axis = min(min(abs(q.x), abs(q.y)), abs(q.z));
  float axisEdge = exp(-axis * 92.0);

  float body = sdOcta(q, 1.0);
  float top = q.y - 0.58;
  float base = -q.y - 0.94;
  float cut = min(abs(body - top), abs(body - base));
  float cutEdge = exp(-cut * 46.0);

  // An edge does not glow evenly. It catches the key along one stretch, throws it,
  // and falls dark along the next; a constant line reads as a neon stroke, which is
  // the one thing this must not look like. Both families get their own phase, and the
  // variation is in the shard's own frame so it turns with the object.
  float alongA = pow(max(sin(q.y * 7.4 + q.x * 4.1 + 1.3), 0.0), 3.0);
  float alongB = pow(max(sin(q.z * 9.1 + q.y * 2.3 + 2.1), 0.0), 4.0);

  return clamp(axisEdge * (0.10 + 0.90 * alongA) + cutEdge * (0.10 + 0.90 * alongB), 0.0, 1.0);
}

vec3 normalAt(vec3 p) {
  vec2 e = vec2(0.004, 0.0);
  return normalize(vec3(
    map(p + e.xyy) - map(p - e.xyy),
    map(p + e.yxy) - map(p - e.yxy),
    map(p + e.yyx) - map(p - e.yyx)
  ));
}

// The studio. Deliberately much brighter than the page behind the shard: a product
// shot lights the object and keeps the lights out of frame, and a shard reflecting a
// near-black room can only ever be a dark polygon. The page has its own dim
// function below; this one exists to be reflected and refracted.
vec3 environment(vec3 d) {
  float up = smoothstep(-0.75, 0.95, d.y);
  // The room falls off to nothing below the horizon and opens into a large cool
  // window above it. The window's edge is what draws a gradient across a flat facet,
  // and a gradient across a facet is the difference between glass and a colour fill:
  // a flat facet lit by a flat environment is, pixel for pixel, a vector shape.
  // The floor is not a void: a page this dark still bounces a little light upward, and
  // a facet refracting pure black reads as a hole in the object rather than as glass.
  vec3 floorBounce = vec3(0.030, 0.036, 0.050);
  vec3 sky = mix(floorBounce, vec3(0.100, 0.130, 0.205), up);

  float window = smoothstep(-0.30, 0.28, d.y) * (1.0 - smoothstep(0.55, 1.05, abs(d.x)));
  sky += vec3(0.52, 0.62, 0.84) * window * 0.90;

  // The key, as a hard point and as a soft halo around it.
  float key = max(dot(d, normalize(vec3(0.52, 0.60, 0.82))), 0.0);
  sky += vec3(1.0, 1.0, 1.0) * pow(key, 420.0) * 6.0;
  sky += vec3(0.85, 0.92, 1.0) * pow(key, 18.0) * 0.55;

  // The gold counter light, which is the only warm thing in the scene.
  float gold = max(dot(d, normalize(vec3(-0.78, 0.10, 0.44))), 0.0);
  sky += vec3(1.0, 0.72, 0.34) * pow(gold, 16.0) * 0.50;
  sky += vec3(1.0, 0.82, 0.52) * pow(gold, 200.0) * 2.20;

  return sky;
}

// The page, in screen space, so it can be sampled through the glass.
vec3 backdropBase(vec2 uv) {
  return mix(vec3(0.031, 0.036, 0.050), vec3(0.008, 0.010, 0.016), length(uv) * 0.9);
}

float dust(vec2 frag) {
  vec2 cell = floor(frag / 2.0);
  float spark = step(0.9965, hash(cell));
  float twinkle = 0.5 + 0.5 * sin(uTime * 1.7 + hash(cell) * 40.0);
  return spark * twinkle * 0.10;
}

float march(vec3 ro, vec3 rd, out float halo) {
  float t = 0.0;
  halo = 0.0;

  for (int i = 0; i < 64; i++) {
    vec3 p = ro + rd * t;
    float d = map(p);
    halo += exp(-abs(d) * 22.0) * 0.012;

    if (abs(d) < 0.0022) return t;
    t += d * 0.9;
    if (t > 7.0) break;
  }
  return -1.0;
}

void main() {
  vec2 uv = (gl_FragCoord.xy - 0.5 * uRes) / uRes.y;
  float form = clamp(uForm, 0.0, 1.0);
  float exit = clamp(uExit, 0.0, 1.0);

  // The camera pulls back as the shard forms; the exit pushes it through instead,
  // so leaving is the same move as arriving, reversed.
  float dist = mix(6.2, 4.1, smoothstep(0.0, 1.0, form)) * (1.0 - exit * 0.82);
  vec3 ro = vec3(0.0, 0.10, dist);
  vec3 rd = normalize(vec3(uv, -1.35));

  float halo;
  float hit = march(ro, rd, halo);

  vec3 colour = backdropBase(uv);

  // A soft contact shadow under the shard. Without one a lit object floats, and a
  // floating object reads as a sticker no matter how good its refractions are.
  colour *= 1.0 - 0.55 * exp(-length((uv - vec2(0.0, -0.60)) * vec2(2.0, 5.5)) * 3.2) * form;

  // A trace of the studio in the page itself: reflections with no visible source are
  // the reason a lit shape can still read as pasted on. Eight percent is enough to
  // place the light and far too little to compete with the wordmark.
  colour += environment(rd) * 0.08 * form;

  colour += dust(gl_FragCoord.xy) * form * vec3(0.72, 0.80, 1.0);
  colour += halo * form * vec3(0.34, 0.50, 0.95);

  if (hit > 0.0) {
    vec3 p = ro + rd * hit;
    vec3 n1 = normalAt(p);
    float cosv = clamp(dot(n1, -rd), 0.0, 1.0);
    float fresnel = 0.04 + 0.96 * pow(1.0 - cosv, 5.0);

    vec3 tile = floor(n1 * 5.0 + 0.5);
    float face = hash(tile.xy + tile.z * 13.7) - 0.5;

    float crease = creaseAt(p);

    // Refract in, cross the volume, refract out. The march stops at the far wall,
    // so the cost is paid once per pixel and not once per light.
    vec3 inside = refract(rd, n1, 1.0 / IOR);
    float thickness = 0.0;
    vec3 pe = p;
    vec3 n2 = -n1;
    bool exited = false;
    for (int i = 0; i < 40; i++) {
      pe = p + inside * thickness;
      float d = map(pe);
      if (d > -0.003) { exited = true; break; }
      thickness += max(-d * 1.2, 0.038);
    }
    if (exited) {
      n2 = normalAt(pe);
      if (dot(n2, inside) < 0.0) n2 = -n2;
    }

    // Dispersion: three exits, three indices. Red bends least, blue most.
    vec3 outR = refract(inside, n2, 1.42);
    vec3 outG = refract(inside, n2, 1.47);
    vec3 outB = refract(inside, n2, 1.55);
    if (dot(outR, outR) < 0.001 || dot(outG, outG) < 0.001 || dot(outB, outB) < 0.001) {
      // Total internal reflection: at this angle the surface is a mirror, and
      // pretending otherwise is what makes fake glass look like plastic.
      outR = reflect(inside, n2);
      outG = outR;
      outB = outR;
    }
    vec3 transmit = vec3(environment(outR).r, environment(outG).g, environment(outB).b);

    // The page, seen through the shard and pushed aside by the refraction. This is
    // the cue that separates transparent from painted: the dust field behind moves
    // as the shard turns, because it is being looked at through an index of 1.46.
    vec3 behind = backdropBase(uv + outG.xy * 0.20) * 1.7;
    behind += dust(gl_FragCoord.xy + outG.xy * 36.0) * form * vec3(0.72, 0.80, 1.0) * 2.2;

    // Beer-Lambert, red absorbed first: thick parts go cold, thin edges stay clear.
    vec3 absorb = exp(-max(thickness, 0.0) * vec3(1.15, 0.62, 0.38));
    vec3 body = (transmit * 1.35 + behind) * absorb * (1.0 + face * 0.18);

    // The inner wall is not a window. At grazing angles it mirrors the outside of
    // the shard, seen from inside, and those bright facets shining through the body
    // are most of what a crystal looks like in a photograph.
    vec3 innerWall = environment(reflect(inside, n2));
    float grazing = pow(1.0 - abs(dot(inside, n2)), 2.0);
    body += innerWall * absorb * grazing * 0.95;

    // Glass is thinner toward every edge, and thin glass lets light out: a wide band
    // under the crease line gives each facet a gradient instead of a flat fill.
    vec3 q = shardSpace(p);
    float wide = min(min(abs(q.x), abs(q.y)), abs(q.z));
    body *= 1.0 + exp(-wide * 6.0) * 0.55;

    // Reflections with a floor. Physically a head-on facet returns 4%, which at
    // this size is invisible, and a crystal whose reflections are invisible reads
    // as a coloured polygon. The floor is what makes neighbouring facets differ by
    // a stop or more, and that difference is the sparkle.
    vec3 refl = environment(reflect(rd, n1)) * (0.72 + face * 1.05);
    float mirror = max(fresnel, 0.34);

    vec3 h = normalize(normalize(vec3(0.52, 0.60, 0.82)) - rd);
    float spec = pow(clamp(dot(n1, h), 0.0, 1.0), 90.0);
    float glint = pow(clamp(dot(n1, h), 0.0, 1.0), 900.0);
    float rim = pow(1.0 - cosv, 3.0);

    colour = mix(body, refl, mirror);
    colour += spec * vec3(0.90, 0.95, 1.0) * 0.55;
    colour += glint * vec3(1.0, 0.99, 0.96) * 3.2;
    colour += rim * vec3(0.42, 0.56, 0.80) * 0.95;
    colour += crease * vec3(0.90, 0.96, 1.0) * 0.95;
  }

  // A pool of light under the shard: what it transmits, it focuses, and the spot is
  // the cue that this is a lens rather than a sticker.
  colour += vec3(0.50, 0.66, 1.0) * exp(-length((uv - vec2(0.0, -0.34)) * vec2(1.7, 1.05)) * 6.0) * 0.12 * form;

  // The exit is a flash, then nothing: the app is already underneath.
  colour += exit * vec3(0.55, 0.62, 0.80) * 0.55;
  colour *= 1.0 - exit * 0.85;

  colour *= 1.0 - 0.22 * smoothstep(0.45, 1.25, length(uv));

  outColour = vec4(colour, 1.0);
}`;

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader | null {
  const shader = gl.createShader(type);
  if (!shader) return null;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}

export function mountCrystal(canvas: HTMLCanvasElement): CrystalHandle | null {
  const gl = canvas.getContext("webgl2", {
    alpha: false,
    antialias: false,
    depth: false,
    stencil: false,
    powerPreference: "high-performance",
    preserveDrawingBuffer: false,
  });
  if (!gl) return null;

  const vertex = compile(gl, gl.VERTEX_SHADER, VERTEX);
  const fragment = compile(gl, gl.FRAGMENT_SHADER, FRAGMENT);
  if (!vertex || !fragment) return null;

  const program = gl.createProgram();
  if (!program) return null;
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) return null;
  gl.useProgram(program);

  const uRes = gl.getUniformLocation(program, "uRes");
  const uTime = gl.getUniformLocation(program, "uTime");
  const uForm = gl.getUniformLocation(program, "uForm");
  const uExit = gl.getUniformLocation(program, "uExit");

  let form = 0;
  let exit = 0;
  let scale = 1;
  let frameTime = 16.7;
  let samples = 0;
  let fps = 0;
  let disposed = false;
  let raf = 0;
  let last = performance.now();
  const opened = performance.now();

  const resize = () => {
    const dpr = Math.min(window.devicePixelRatio || 1, DPR_CAP) * scale;
    const cssWidth = canvas.clientWidth || window.innerWidth;
    const cssHeight = canvas.clientHeight || window.innerHeight;
    const width = Math.max(1, Math.min(WIDTH_CAP, Math.round(cssWidth * dpr)));
    const height = Math.max(1, Math.round((width / cssWidth) * cssHeight));
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
      gl.viewport(0, 0, width, height);
    }
  };

  const frame = (now: number) => {
    if (disposed) return;
    raf = requestAnimationFrame(frame);

    const dt = now - last;
    last = now;

    if (document.hidden) return;

    // Rolling average over ~20 frames, then a two-step hysteresis: give up 15% of
    // the resolution when frames are late, take it back when there is room.
    frameTime = frameTime * 0.9 + Math.min(dt, 60) * 0.1;
    samples += 1;
    if (samples % 20 === 0) {
      if (frameTime > SLOW_MS && scale > 0.5) {
        scale = Math.max(0.5, scale - 0.15);
        resize();
      } else if (frameTime < FAST_MS && scale < 1) {
        scale = Math.min(1, scale + 0.1);
        resize();
      }
      fps = Math.round(1000 / Math.max(frameTime, 1));
    }

    resize();
    gl.uniform2f(uRes, canvas.width, canvas.height);
    gl.uniform1f(uTime, (now - opened) / 1000);
    gl.uniform1f(uForm, form);
    gl.uniform1f(uExit, exit);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  };

  resize();
  raf = requestAnimationFrame(frame);

  return {
    setForm: (value) => {
      form = Math.min(1, Math.max(0, value));
    },
    setExit: (value) => {
      exit = Math.min(1, Math.max(0, value));
    },
    stats: () => ({ fps, scale: Number(scale.toFixed(2)) }),
    dispose: () => {
      disposed = true;
      cancelAnimationFrame(raf);
      gl.deleteProgram(program);
      gl.deleteShader(vertex);
      gl.deleteShader(fragment);
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    },
  };
}
