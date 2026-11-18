// A minimal Chrome DevTools Protocol driver for the running launcher.
// Verification only: the app must be started with
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333"
//
//   node tools/cdp.mjs eval "<javascript expression>"
//   node tools/cdp.mjs script <file.js>          (same, read from disk)
//   node tools/cdp.mjs shot <path.png>
//   node tools/cdp.mjs type "<css selector>" <text>
//   node tools/cdp.mjs key <Enter|Tab|Escape>
//   node tools/cdp.mjs click <x> <y>
//   node tools/cdp.mjs drag <x1> <y1> <x2> <y2>

import { readFileSync, writeFileSync } from "node:fs";

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
  if (command === "eval" || command === "script") {
    // `script` reads the expression from a file. Quoting a multi-line probe
    // through PowerShell into a native command mangles the inner quotes, so
    // anything longer than one line goes to disk first.
    const expression = command === "script" ? readFileSync(rest[0], "utf8") : rest.join(" ");
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
  } else if (command === "click") {
    // A trusted press and release, which is the only kind of click the opening
    // accepts as "skip". A synthetic MouseEvent would not prove anything.
    const x = Number(rest[0]);
    const y = Number(rest[1]);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await session.send("Input.dispatchMouseEvent", {
        type,
        x,
        y,
        button: "left",
        clickCount: 1,
        buttons: type === "mousePressed" ? 1 : 0,
      });
    }
    console.log(`clicked ${x},${y}`);
  } else if (command === "drag") {
    // A press, a stream of moves, a release, all trusted by the renderer. This
    // proves a drag handler runs and what it calls; it cannot prove the OS moved
    // the window, because the move loop tracks the physical button.
    const [x1, y1, x2, y2] = rest.map(Number);
    await session.send("Input.dispatchMouseEvent", {
      type: "mousePressed",
      x: x1,
      y: y1,
      button: "left",
      clickCount: 1,
      buttons: 1,
    });
    const steps = 12;
    for (let i = 1; i <= steps; i++) {
      await session.send("Input.dispatchMouseEvent", {
        type: "mouseMoved",
        x: x1 + ((x2 - x1) * i) / steps,
        y: y1 + ((y2 - y1) * i) / steps,
        button: "left",
        buttons: 1,
      });
      await new Promise((r) => setTimeout(r, 25));
    }
    await session.send("Input.dispatchMouseEvent", {
      type: "mouseReleased",
      x: x2,
      y: y2,
      button: "left",
      clickCount: 1,
      buttons: 0,
    });
    console.log(`dragged ${x1},${y1} to ${x2},${y2}`);
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
    console.error("usage: eval | script | shot | type | key | click | drag");
    process.exitCode = 1;
  }
} finally {
  session.close();
}
