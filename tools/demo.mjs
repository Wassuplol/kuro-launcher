// Records the README demo from the real app.
//
//   1. build with the debug port (see tools/cdp.mjs)
//   2. node tools/demo.mjs src-tauri/target/release/kuro-launcher.exe demo-frames
//   3. ffmpeg -f concat -safe 0 -i demo-frames/list.txt -vf "fps=16,scale=960:-1" \
//        -c:v libwebp -q:v 72 -loop 0 -an assets/demo.webp
//
// The app is spawned by this script so the intro is caught from its first frame.
// Frames carry their own timestamps, so a pause in the page does not turn into a
// jump cut: each frame's duration is written into the concat list.

import { spawn } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const PORT = process.env.CDP_PORT ?? "9333";
const BASE = `http://127.0.0.1:${PORT}`;
const [exe, outDir = "demo-frames"] = process.argv.slice(2);

if (!exe) {
  console.error("usage: node tools/demo.mjs <path-to-exe> [outDir]");
  process.exit(1);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

const app = spawn(exe, [], { detached: false, stdio: "ignore" });

async function findPage() {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const response = await fetch(`${BASE}/json/list`);
      const targets = await response.json();
      const page = targets.find((target) => target.type === "page");
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // not listening yet
    }
    await sleep(100);
  }
  throw new Error("the app never opened a debug port");
}

const socket = new WebSocket(await findPage());
let counter = 0;
const pending = new Map();
const frames = [];

function send(method, params = {}) {
  const id = ++counter;
  socket.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}

socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);

  if (message.id && pending.has(message.id)) {
    const slot = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) slot.reject(new Error(message.error.message));
    else slot.resolve(message.result);
    return;
  }

  if (message.method === "Page.screencastFrame") {
    const { data, metadata, sessionId } = message.params;
    const index = frames.length;
    const file = `frame-${String(index).padStart(5, "0")}.jpg`;
    writeFileSync(join(outDir, file), Buffer.from(data, "base64"));
    frames.push({ file, ts: metadata.timestamp });
    void send("Page.screencastFrameAck", { sessionId });
  }
});

await new Promise((resolve) => socket.addEventListener("open", resolve));

await send("Page.enable");
await send("Page.startScreencast", {
  format: "jpeg",
  quality: 82,
  maxWidth: 1180,
  maxHeight: 760,
  everyNthFrame: 1,
});

const click = async (predicate) => {
  const expression = `(() => {
    const target = [...document.querySelectorAll("button, [cmdk-item], [role=radio]")].find((element) => ${predicate});
    if (!target) return "missing";
    target.click();
    return "ok";
  })()`;
  const result = await send("Runtime.evaluate", { expression, returnByValue: true });
  return result.result.value;
};

const key = async (name, code, vk, modifiers = 0, text) => {
  await send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: name,
    code,
    windowsVirtualKeyCode: vk,
    nativeVirtualKeyCode: vk,
    modifiers,
    ...(text ? { text, unmodifiedText: text } : {}),
  });
  await send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: name,
    code,
    windowsVirtualKeyCode: vk,
    nativeVirtualKeyCode: vk,
    modifiers,
  });
};

// The intro runs on its own. Give it room, then walk the flow.
await sleep(1500);

await send("Runtime.evaluate", {
  expression: `document.querySelector("#onboarding-name")?.focus()`,
});
await send("Input.insertText", { text: "Mem" });
await sleep(600);

await key("Enter", "Enter", 13, 0, "\r");
await sleep(700);

await click('element.textContent.trim() === "Horror"');
await sleep(200);
await click('element.textContent.trim() === "RPG"');
await sleep(800);

await click('element.textContent.trim() === "Continue"');
await sleep(900);

await click('element.textContent.includes("Building something")');
await sleep(900);

await click('element.textContent.trim() === "Finish"');
await sleep(1400);

await key("k", "KeyK", 75, 2);
await sleep(900);

await click('element.textContent.includes("Open settings")');
await sleep(1500);

await key("Escape", "Escape", 27);
await sleep(1400);

await send("Page.stopScreencast");
await sleep(200);
socket.close();
app.unref();

// Durations come from the timestamps, so static stretches stay still.
const lines = [];
for (let index = 0; index < frames.length; index += 1) {
  const current = frames[index];
  const next = frames[index + 1];
  const duration = next ? Math.max(0.008, next.ts - current.ts) : 0.12;
  lines.push(`file '${current.file}'`, `duration ${duration.toFixed(3)}`);
}
lines.push(`file '${frames.at(-1).file}'`);

writeFileSync(join(outDir, "list.txt"), `${lines.join("\n")}\n`);
console.log(`${frames.length} frames over ${(frames.at(-1).ts - frames[0].ts).toFixed(1)}s`);
process.exit(0);
