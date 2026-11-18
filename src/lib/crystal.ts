/**
 * The opening crystal: one fragment shader, no dependencies, no textures.
 *
 * A raymarched octahedron, chiselled by three planes and shaded with fresnel and
 * an internal tint, over a dust field. It is written against WebGL2 and drawn as a
 * fullscreen triangle from `gl_VertexID`, so there is no vertex buffer, no index
 * buffer and no geometry to upload: the whole intro is one program and five
 * uniforms.
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
 * - The march is 56 steps with an early exit at 0.0015, and the shape is one SDF
 *   with two plane subtractions: no textures, no shadow pass, no post chain.
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

mat2 rot(float a) { float c = cos(a), s = sin(a); return mat2(c, -s, s, c); }

float hash(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

// An octahedron, which is a rhombus of three absolute planes.
float sdOcta(vec3 p, float s) {
  p = abs(p);
  return (p.x + p.y + p.z - s) * 0.5773502692;
}

float map(vec3 p) {
  vec3 q = p;
  q.xz *= rot(uTime * 0.22);
  q.xy *= rot(sin(uTime * 0.17) * 0.30 + 0.18);
  q.yz *= rot(0.16);

  float body = sdOcta(q, 1.05);

  // Three planes carve the octahedron into a shard: the flat faces catch light in
  // steps, which is what reads as glass rather than as a ball.
  body = max(body, q.y - 0.62);
  body = max(body, -q.y - 1.15);

  // The cut faces are slightly inset, so the silhouette keeps a thin edge.
  float bevel = sdOcta(q, 1.11);
  return max(body, -bevel);
}

vec3 normalAt(vec3 p) {
  vec2 e = vec2(0.0012, 0.0);
  return normalize(vec3(
    map(p + e.xyy) - map(p - e.xyy),
    map(p + e.yxy) - map(p - e.yxy),
    map(p + e.yyx) - map(p - e.yyx)
  ));
}

// March one ray. Returns the hit distance, or -1, and accumulates a halo so the
// shape glows into the background without a blur pass.
float march(vec3 ro, vec3 rd, out float halo, out float edge) {
  float t = 0.0;
  halo = 0.0;
  edge = 0.0;

  for (int i = 0; i < 56; i++) {
    vec3 p = ro + rd * t;
    float d = map(p);
    halo += exp(-abs(d) * 22.0) * 0.012;

    if (abs(d) < 0.0015) {
      edge = clamp(1.0 - float(i) / 56.0, 0.0, 1.0);
      return t;
    }
    t += d * 0.85;
    if (t > 7.0) break;
  }
  return -1.0;
}

void main() {
  vec2 uv = (gl_FragCoord.xy - 0.5 * uRes) / uRes.y;
  float form = clamp(uForm, 0.0, 1.0);
  float exit = clamp(uExit, 0.0, 1.0);

  // The camera starts close and pulls back as the shard forms; the exit pushes it
  // through the shard instead, so leaving is the same move as arriving, reversed.
  // The splash window is 520 wide, so the shard is framed to fit inside it: a
  // shape that overflows the frame shows one facet and reads as a dark wall.
  float dist = mix(6.2, 4.1, smoothstep(0.0, 1.0, form)) * (1.0 - exit * 0.82);
  vec3 ro = vec3(0.0, 0.12, dist);
  vec3 rd = normalize(vec3(uv, -1.35));

  float halo, edge;
  float hit = march(ro, rd, halo, edge);

  // Background: cold dark, with a faint dust field that only shows once the scene
  // has started forming.
  vec3 colour = mix(vec3(0.031, 0.036, 0.050), vec3(0.008, 0.010, 0.016), length(uv) * 0.9);

  vec2 cell = floor(gl_FragCoord.xy / 2.0);
  float spark = step(0.9965, hash(cell));
  float twinkle = 0.5 + 0.5 * sin(uTime * 1.7 + hash(cell) * 40.0);
  colour += spark * twinkle * 0.10 * form * vec3(0.72, 0.80, 1.0);

  colour += halo * form * vec3(0.34, 0.50, 0.95);

  if (hit > 0.0) {
    vec3 p = ro + rd * hit;
    vec3 n = normalAt(p);
    float fresnel = pow(1.0 - clamp(dot(n, -rd), 0.0, 1.0), 2.6);

    // Flat faces: snap the normal so the facets read as cut planes, not as a
    // smooth blob. 0.22 is one facet's worth of quantisation at this size.
    vec3 facet = normalize(floor(n / 0.22) * 0.22 + 0.11);

    float key = clamp(dot(facet, normalize(vec3(0.55, 0.9, 0.6))), 0.0, 1.0);
    float fill = clamp(dot(facet, normalize(vec3(-0.6, -0.2, 0.7))), 0.0, 1.0);
    float rim = pow(clamp(dot(facet, normalize(vec3(-0.7, 0.3, 0.5))), 0.0, 1.0), 2.2);

    // Three lights on purpose: a cold key from above, a weak fill from the lower
    // left so the cut faces never go fully black, and a gold rim that is the only
    // warm thing in an otherwise blue scene.
    vec3 body = vec3(0.26, 0.40, 0.66) * (0.55 + key * 1.45) + vec3(0.07, 0.10, 0.18) * fill;
    vec3 sheen = vec3(0.80, 0.62, 0.30) * rim * 0.85;
    vec3 glass = mix(body, vec3(0.72, 0.88, 1.20), fresnel * 0.90);

    // A tight specular lobe on the snapped facet normal is what separates glass
    // from matte plastic: the highlight is small, hard and moves with the rotation.
    vec3 half = normalize(normalize(vec3(0.55, 0.9, 0.6)) - rd);
    float spec = pow(clamp(dot(facet, half), 0.0, 1.0), 96.0);
    float spec2 = pow(clamp(dot(facet, normalize(normalize(vec3(-0.7, 0.3, 0.5)) - rd)), 0.0, 1.0), 48.0);

    colour = mix(colour, glass + sheen, 0.96);
    colour += spec * vec3(1.0, 0.99, 0.96) * 2.6;
    colour += spec2 * vec3(1.0, 0.86, 0.62) * 0.9;
    colour += fresnel * vec3(0.75, 0.88, 1.0) * 1.05;
    // Internal light: the facet just under the surface picks up the cold body and
    // gives the shard something to look like it is made of.
    colour += glass * 0.22;
    colour += edge * 0.14 * vec3(0.85, 0.93, 1.0);
  }

  // The exit is a flash, then nothing: the app is already underneath.
  colour += exit * vec3(0.55, 0.62, 0.80) * 0.55;
  colour *= 1.0 - exit * 0.85;

  // A slight vignette keeps the wordmark legible over the centre.
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
