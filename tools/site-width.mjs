// Widths for the published page, measured over the Chrome DevTools Protocol.
//
//   node tools/site-width.mjs <url> <width> <height> <out.png>
//
// Edge headless ignores --window-size for layout when it also has to dump the DOM, and
// a screenshot taken at a width the page never laid out at shows clipping that does not
// exist. Emulation.setDeviceMetricsOverride sets the viewport the layout actually uses,
// which is the only way to answer "does this fit at 390".

const [, , url, widthArg, heightArg, shot] = process.argv;
if (!url) {
  console.error("usage: node tools/site-width.mjs <url> <width> <height> [out.png]");
  process.exit(2);
}

const width = Number(widthArg ?? 390);
const height = Number(heightArg ?? 844);
const port = process.env.SITE_CDP_PORT ?? "9444";

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((target) => target.type === "page");
if (!page) {
  console.error("no page target; is Edge running with --remote-debugging-port?");
  process.exit(2);
}

const socket = new WebSocket(page.webSocketDebuggerUrl);
let next = 0;
const waiting = new Map();

socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  const slot = waiting.get(message.id);
  if (!slot) return;
  waiting.delete(message.id);
  if (message.error) slot.reject(new Error(JSON.stringify(message.error)));
  else slot.resolve(message.result);
});

await new Promise((resolve) => socket.addEventListener("open", resolve, { once: true }));

function send(method, params = {}) {
  const id = ++next;
  socket.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve, reject) => waiting.set(id, { resolve, reject }));
}

await send("Emulation.setDeviceMetricsOverride", {
  width,
  height,
  deviceScaleFactor: 2,
  mobile: width < 700,
});
await send("Page.enable");
await send("Page.navigate", { url });
// The page is static; this is the paint, not a guess at one.
await new Promise((resolve) => setTimeout(resolve, 1400));

const probe = `(() => {
  const w = innerWidth;
  const offenders = [...document.querySelectorAll("body *")]
    .map((el) => ({ el, box: el.getBoundingClientRect() }))
    .filter(({ box }) => box.width > 0 && box.right > w + 1 && !el.closest("[aria-hidden]"))
    .slice(0, 12)
    .map(({ el, box }) => el.tagName.toLowerCase() + "." + (String(el.className).split(" ")[0] || "-") + "[" + Math.round(box.left) + ".." + Math.round(box.right) + "]");
  return JSON.stringify({
    viewport: w,
    scrollWidth: document.documentElement.scrollWidth,
    bodyScroll: document.body.scrollWidth,
    offenders,
  });
})()`;

const answer = await send("Runtime.evaluate", { expression: probe, returnByValue: true });
console.log(JSON.stringify(JSON.parse(answer.result.value), null, 1));

if (shot) {
  const { data } = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  const { writeFileSync } = await import("node:fs");
  writeFileSync(shot, Buffer.from(data, "base64"));
  console.log("shot written", shot);
}

socket.close();
