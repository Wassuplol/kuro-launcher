// A minimal Chrome DevTools Protocol driver for the running launcher.
// Verification only: the app must be started with
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333"
//
//   node tools/cdp.mjs eval "<javascript expression>"
//   node tools/cdp.mjs shot <path.png>
//   node tools/cdp.mjs type "<css selector>" <text>
//   node tools/cdp.mjs key <Enter|Tab|Escape>

import { writeFileSync } from "node:fs";

const PORT = process.env.KURO_CDP_PORT ?? "9333";
const BASE = `http://127.0.0.1:${PORT}`;

async function pageSocket() {
  const response = await fetch(`${BASE}/json/list`);
  const targets = await response.json();
  const page = targets.find((target) => target.type === "page");
  if (!page) throw new Error("no page target; is the launcher running with the debug port?");
  return page.webSocketDebuggerUrl;
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

const KEYS = { Enter: 13, Tab: 9, Escape: 27, Backspace: 8, Slash: 191 };
const KEY_LABEL = { Slash: "/" };

const [command, ...rest] = process.argv.slice(2);
const session = await connect(await pageSocket());

try {
  if (command === "eval") {
    const expression = rest.join(" ");
    const result = await session.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      console.error(JSON.stringify(result.exceptionDetails.exception ?? result.exceptionDetails));
      process.exitCode = 1;
    } else {
      const value = result.result.value;
      console.log(typeof value === "string" ? value : JSON.stringify(value, null, 2));
    }
  } else if (command === "shot") {
    const target = rest[0];
    const shot = await session.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(target, Buffer.from(shot.data, "base64"));
    console.log(`saved ${target}`);
  } else if (command === "type") {
    const [selector, ...words] = rest;
    await session.send("Runtime.evaluate", {
      expression: `document.querySelector(${JSON.stringify(selector)}).focus()`,
    });
    await session.send("Input.insertText", { text: words.join(" ") });
    console.log("typed");
  } else if (command === "key") {
    const name = rest[0];
    const mods = rest[1] ?? "";
    const modifiers = mods.includes("ctrl") ? 2 : 0;
    const text = name === "Enter" ? "\r" : undefined;
    const vk = KEYS[name] ?? name.toUpperCase().charCodeAt(0);
    const codeString = KEYS[name]
      ? name
      : /^[0-9]$/.test(name)
        ? `Digit${name}`
        : `Key${name.toUpperCase()}`;
    await session.send("Input.dispatchKeyEvent", {
      type: "keyDown",
      key: KEY_LABEL[name] ?? name,
      code: codeString,
      windowsVirtualKeyCode: vk,
      nativeVirtualKeyCode: vk,
      modifiers,
      ...(text ? { text, unmodifiedText: text } : {}),
    });
    await session.send("Input.dispatchKeyEvent", {
      type: "keyUp",
      key: KEY_LABEL[name] ?? name,
      code: codeString,
      windowsVirtualKeyCode: vk,
      nativeVirtualKeyCode: vk,
      modifiers,
    });
    console.log(`pressed ${mods ? mods + "+" : ""}${name}`);
  } else {
    console.error("usage: eval | shot | type | key");
    process.exitCode = 1;
  }
} finally {
  session.close();
}
