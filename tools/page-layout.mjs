// Page layout check: the geometry a screenshot cannot be trusted to show.
//
// Run it against a build that has the debug port:
//   node tools/page-layout.mjs
//
// Every page is measured the same way: the scroll container's content column is
// found, its direct children are the stacked blocks, and four things are
// asserted.
//
//   overlap    no two blocks intersect. A drawer-era `sticky` footer left behind
//              when a panel became a page shows up here, and nowhere else: it
//              looks correct until the page is tall enough to scroll.
//   pinned     nothing inside `main` is `sticky`, `absolute` or `fixed`. On a page
//              those are almost always something that used to float over a drawer.
//   escaped    no block starts left of the column or ends right of it, which is
//              what a leftover `-mx-5` does.
//   tiny       no control under 16px in either direction.
//
// Exit code 1 on any failure, with the page and the numbers that failed. The
// route is set by hash plus reload, because the hash is an output of the shell's
// route rather than an input to it.

import { readFileSync } from "node:fs";

const PORT = process.env.KURO_CDP_PORT ?? "9333";
const BASE = `http://127.0.0.1:${PORT}`;

const router = readFileSync(new URL("../src/app/router.ts", import.meta.url), "utf8");

/**
 * One block of router.ts, by name.
 *
 * The file holds several maps that share the shape `name: "value",` — the page list,
 * the labels and the hotkey table. Scanning the whole file merged them, so every page
 * was looked up under its shortcut and the walk failed on the first one. Read the block
 * that is asked for and nothing else.
 */
function mapBlock(name) {
  const start = router.indexOf(`export const ${name}`);
  if (start < 0) throw new Error(`router.ts has no ${name}`);
  const end = router.indexOf("\n};", start);
  return router.slice(start, end < 0 ? undefined : end);
}

const PAGES = [...mapBlock("PAGES = [").matchAll(/^\s+"([a-z]+)",$/gm)].map((match) => match[1]);

// The rail says "Launch a place" and "Roblox versions", so the label has to come
// from the same table the rail reads rather than from the route id.
const LABELS = Object.fromEntries(
  [...mapBlock("PAGE_LABEL").matchAll(/^\s+([a-z]+):\s+"([^"]+)",$/gm)].map((match) => [
    match[1],
    match[2],
  ]),
);

/**
 * The main window's socket.
 *
 * With the strip open there are two page targets, and measuring the wrong one reads as
 * "the page never settled". The window label is the only thing that tells them apart,
 * so each candidate is asked for its own.
 */
async function pageSocket() {
  const response = await fetch(`${BASE}/json/list`);
  const targets = await response.json();
  const pages = targets.filter((target) => target.type === "page");
  if (pages.length === 0) throw new Error("no page target; is the launcher running with the debug port?");

  for (const page of pages) {
    const session = await connect(page.webSocketDebuggerUrl);
    try {
      const result = await session.send("Runtime.evaluate", {
        expression: "window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label ?? ''",
        returnByValue: true,
      });
      if (result.result.value === "main") return page.webSocketDebuggerUrl;
    } finally {
      session.close();
    }
  }

  // Older builds without the metadata object: one window, so the first page is it.
  return pages[0].webSocketDebuggerUrl;
}

function connect(url) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    let counter = 0;
    const pending = new Map();

    socket.addEventListener("open", () => {
      resolve({
        send(method, params = {}) {
          const id = ++counter;
          socket.send(JSON.stringify({ id, method, params }));
          return new Promise((res, rej) => pending.set(id, { res, rej }));
        },
        close() {
          socket.close();
        },
      });
    });

    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const slot = pending.get(message.id);
      if (!slot) return;
      pending.delete(message.id);
      if (message.error) slot.rej(new Error(message.error.message));
      else slot.res(message.result);
    });

    socket.addEventListener("error", () => reject(new Error("cdp socket error")));
  });
}

const probe = `(() => {
  const main = document.querySelector("main");
  if (!main) return { error: "no main element" };

  const scroller = [...main.querySelectorAll("div")].find(
    (el) => getComputedStyle(el).overflowY === "auto" || getComputedStyle(el).overflowY === "scroll",
  );
  const column = scroller?.firstElementChild;
  if (!column) return { error: "no content column" };

  const blocks = [...column.children]
    .filter((el) => {
      const style = getComputedStyle(el);
      const box = el.getBoundingClientRect();
      return style.display !== "none" && box.height > 0;
    })
    .map((el) => {
      const box = el.getBoundingClientRect();
      return {
        title: (el.querySelector("h2, h1")?.textContent ?? el.textContent ?? "?").trim().slice(0, 28),
        top: Math.round(box.top),
        bottom: Math.round(box.bottom),
        left: Math.round(box.left),
        right: Math.round(box.right),
      };
    });

  // Sticky and fixed are always wrong inside a page: they are what a drawer
  // leaves behind. Absolute is only wrong when the element is a block itself
  // (a direct child of the column), because an icon placed inside a field is
  // ordinary layout.
  const pinned = [...main.querySelectorAll("*")]
    .filter((el) => {
      const position = getComputedStyle(el).position;
      if (position === "sticky" || position === "fixed") return true;
      return position === "absolute" && el.parentElement === column;
    })
    .map((el) => el.tagName.toLowerCase() + " " + String(el.className).slice(0, 48));

  const tiny = [...main.querySelectorAll("button")]
    .map((el) => {
      const box = el.getBoundingClientRect();
      return { label: (el.textContent ?? "").trim().slice(0, 20), w: Math.round(box.width), h: Math.round(box.height) };
    })
    .filter((button) => button.w > 0 && (button.w < 16 || button.h < 16));

  const columnBox = column.getBoundingClientRect();

  return {
    hash: location.hash,
    heading: document.querySelector("main h1")?.textContent?.trim() ?? "",
    // A wide page fills the window ("w-full") instead of centring a column ("max-w-…").
    // Both are PageFrame pages. The token is matched exactly so "min-w-full" cannot pass.
    framed:
      String(column.className).includes("max-w-") ||
      String(column.className).split(" ").includes("w-full"),
    blocks,
    pinned,
    tiny,
    column: { left: Math.round(columnBox.left), right: Math.round(columnBox.right) },
  };
})()`;

const session = await connect(await pageSocket());
const failures = [];

async function evaluate(expression) {
  const result = await session.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(JSON.stringify(result.exceptionDetails.exception ?? {}));
  }
  return result.result.value;
}

/**
 * Waits for the reload to finish and for the geometry to stop moving.
 *
 * Counting DOM nodes is not enough: the shell slides the content in, so the node
 * count is already final while the column is still 200px to the left. Measuring
 * during that window produces overlaps that do not exist, which is the same class
 * of mistake as believing a screenshot.
 */
async function settle() {
  let previous = null;
  let empty = 0;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    await new Promise((resolve) => setTimeout(resolve, 200));
    const ready = await evaluate(
      `document.readyState === "complete" && Boolean(document.querySelector("main"))`,
    );
    if (!ready) continue;

    const shape = await evaluate(`(() => {
      const main = document.querySelector("main");
      if (!main) return null;
      const scroller = [...main.querySelectorAll("div")].find(
        (el) => getComputedStyle(el).overflowY === "auto" || getComputedStyle(el).overflowY === "scroll",
      );
      const column = scroller?.firstElementChild;
      if (!column) return null;
      const box = column.getBoundingClientRect();
      return [Math.round(box.left), Math.round(box.width), scroller.scrollHeight];
    })()`);

    // A page with no scroller (Search, Library and Friends are still centred
    // empty states) has no column to wait for. Three looks and it is done.
    if (!shape) {
      empty += 1;
      if (empty >= 3) return;
      continue;
    }
    empty = 0;

    if (previous && shape.every((value, i) => value === previous[i])) return;
    previous = shape;
  }
  throw new Error("the page never settled");
}

const label = (page) => LABELS[page] ?? page;

for (const page of PAGES) {
  // Clicking the rail navigates in place. A hash plus reload would replay the
  // opening splash, which resizes the window mid-measurement.
  // The rail words are longer than the route names on purpose: "Launch a place",
  // "Roblox versions". Match exactly first, then by prefix, then by containment.
  const clicked = await evaluate(`(() => {
    const wanted = ${JSON.stringify(label(page))}.toLowerCase();
    const candidates = [...document.querySelectorAll("button[aria-label]")].map((el) => ({
      el,
      name: el.getAttribute("aria-label").toLowerCase(),
    }));
    const hit =
      candidates.find((c) => c.name === wanted) ??
      candidates.find((c) => c.name.startsWith(wanted)) ??
      candidates.find((c) => c.name.includes(wanted));
    if (!hit) return false;
    hit.el.click();
    return true;
  })()`);
  if (!clicked) {
    failures.push(`${page}: no rail entry labelled "${label(page)}"`);
    continue;
  }
  await settle();

  // A page that is still swapping its skeleton for content can be caught between two
  // React commits, with an empty scroller for a few frames. That read is not a finding,
  // so it is asked again before anything is concluded from it.
  let measured = await evaluate(probe);
  if (!measured?.framed || !measured?.heading || !measured?.blocks?.length) {
    await settle();
    await new Promise((resolve) => setTimeout(resolve, 500));
    await settle();
    measured = await evaluate(probe);
  }

  const { heading, blocks, framed, pinned, tiny, column } = measured;

  if (!framed || !heading || !blocks?.length) {
    console.log(
      `${page.padEnd(12)} still its own layout, not a PageFrame page: nothing to check`,
      JSON.stringify({
        hash: measured?.hash,
        error: measured?.error,
        framed,
        heading,
        blocks: blocks?.length,
      }),
    );
    continue;
  }

  for (let i = 1; i < blocks.length; i += 1) {
    if (blocks[i].top < blocks[i - 1].bottom - 1) {
      failures.push(
        `${page}: "${blocks[i - 1].title}" (${blocks[i - 1].top}-${blocks[i - 1].bottom}) overlaps "${blocks[i].title}" (${blocks[i].top}-${blocks[i].bottom})`,
      );
    }
  }

  if (pinned.length) failures.push(`${page}: pinned elements in main: ${pinned.join(" | ")}`);
  if (tiny.length) {
    failures.push(
      `${page}: controls under 16px: ${tiny.map((b) => `${b.label || "(no label)"} ${b.w}x${b.h}`).join(", ")}`,
    );
  }

  for (const block of blocks) {
    if (block.left < column.left - 1 || block.right > column.right + 1) {
      failures.push(
        `${page}: "${block.title}" escapes the column (${block.left}-${block.right} vs ${column.left}-${column.right})`,
      );
    }
  }

  console.log(
    `${page.padEnd(9)} ${String(heading ?? "?").padEnd(22)} ${String(blocks.length).padStart(2)} blocks, column ${column.left}-${column.right}`,
  );
}

// One page is not on the rail: a game is opened from a tile, so it is reached the
// way a person reaches it, and measured the same way.
await evaluate(`(() => { location.hash = "#/home"; location.reload(); "ok" })()`);
await settle();
// The feed is a network call, so the tiles arrive after the page: wait for one
// instead of measuring the skeleton and reporting that there is nothing to open.
let opened = false;
for (let attempt = 0; attempt < 40 && !opened; attempt += 1) {
  await new Promise((resolve) => setTimeout(resolve, 250));
  opened = await evaluate(`(() => {
    const tile = [...document.querySelectorAll("main button[title]")].find((el) =>
      (el.getAttribute("title") ?? "").startsWith("Open"),
    );
    if (!tile) return false;
    tile.click();
    return true;
  })()`);
}

if (!opened) {
  failures.push("game: no tile to open");
} else {
  // Two settles with a pause between them: the first can land on the frame React
  // committed before the route change repainted, which measured a column 330px
  // off and a page that looked fine in the capture.
  await settle();
  await new Promise((resolve) => setTimeout(resolve, 700));
  await settle();
  const { heading, blocks, framed, pinned, tiny, column } = await evaluate(probe);
  if (!framed || !heading || !blocks?.length) {
    failures.push("game: the page did not render a column");
  } else {
    for (let i = 1; i < blocks.length; i += 1) {
      if (blocks[i].top < blocks[i - 1].bottom - 1) {
        failures.push(
          `game: "${blocks[i - 1].title}" (${blocks[i - 1].top}-${blocks[i - 1].bottom}) overlaps "${blocks[i].title}" (${blocks[i].top}-${blocks[i].bottom})`,
        );
      }
    }
    if (pinned.length) failures.push(`game: pinned elements in main: ${pinned.join(" | ")}`);
    if (tiny.length) {
      failures.push(
        `game: controls under 16px: ${tiny.map((b) => `${b.label || "(no label)"} ${b.w}x${b.h}`).join(", ")}`,
      );
    }
    for (const block of blocks) {
      if (block.left < column.left - 1 || block.right > column.right + 1) {
        failures.push(
          `game: "${block.title}" escapes the column (${block.left}-${block.right} vs ${column.left}-${column.right})`,
        );
      }
    }
    console.log(
      `${"game".padEnd(9)} ${String(heading ?? "?").slice(0, 22).padEnd(22)} ${String(blocks.length).padStart(2)} blocks, column ${column.left}-${column.right}`,
    );
  }
}

session.close();

if (failures.length) {
  console.error("");
  for (const failure of failures) console.error(`FAIL ${failure}`);
  process.exitCode = 1;
} else {
  console.log("\nlayout ok: no overlaps, nothing pinned, every block inside the column");
}
