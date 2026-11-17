// Polls the opening from launch, so the timeline is measured rather than
// inferred from where screenshots happened to land.
//
//   node open-timeline.mjs <exe>

import { spawn } from "node:child_process";

const [, , exe] = process.argv;
const BASE = `http://127.0.0.1:${process.env.KURO_CDP_PORT ?? "9333"}`;
const t0 = Date.now();

async function pageSocket() {
  for (;;) {
    if (Date.now() - t0 > 25000) throw new Error("no debug port");
    try {
      const targets = await (await fetch(`${BASE}/json/list`)).json();
      const page = targets.find((t) => t.type === "page");
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 50));
  }
}

function connect(url) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    let counter = 0;
    const pending = new Map();
    socket.addEventListener("open", () =>
      resolve({
        send(method, params = {}) {
          const id = ++counter;
          socket.send(JSON.stringify({ id, method, params }));
          return new Promise((res, rej) => pending.set(id, { res, rej }));
        },
        close: () => socket.close(),
      }),
    );
    socket.addEventListener("error", reject);
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const slot = pending.get(message.id);
      if (!slot) return;
      pending.delete(message.id);
      if (message.error) slot.rej(new Error(JSON.stringify(message.error)));
      else slot.res(message.result);
    });
  });
}

const child = spawn(exe, { detached: true, stdio: "ignore" });
const session = await connect(await pageSocket());

const rows = [];
for (let i = 0; i < 30; i += 1) {
  const out = await session.send("Runtime.evaluate", {
    expression: `JSON.stringify({
      ready: !!document.querySelector('#root'),
      loader: !!document.querySelector('svg[viewBox="0 0 116 116"]'),
      status: (document.body.innerText.match(/Reading your settings|Finding your Roblox clients|Asking Roblox what is live|Ready/) || [null])[0],
      shell: document.body.innerText.includes('Commands'),
      w: innerWidth, h: innerHeight
    })`,
    returnByValue: true,
  });
  let value = {};
  try {
    value = JSON.parse(out.result.value ?? "{}");
  } catch {
    value = {};
  }
  rows.push([Date.now() - t0, value]);
  await new Promise((r) => setTimeout(r, 120));
}

for (const [ms, v] of rows) {
  console.log(
    `${String(ms).padStart(5)}ms  ${v.w}x${v.h}  loader=${v.loader ? "Y" : "-"}  shell=${v.shell ? "Y" : "-"}  ${v.status ?? ""}`,
  );
}

session.close();
child.unref();
