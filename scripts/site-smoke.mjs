// Usage: node scripts/site-smoke.mjs [site-dir] [--shots DIR]
// Opens the web page (built by scripts/build-site.sh, default target/site) in headless Chrome
// in every scene, as a desktop window and as a phone (390 x 844 CSS pixels at 3x, with touch),
// and fails on any error in the console: a panic in the wasm or in the page's script. Then it
// taps the music stone, feeds and pets on the desktop page, and fails unless the music plays,
// and taps the time and scene stones, failing unless they change the scene.
// --shots saves a screenshot of each. Chrome runs on the CPU (SwiftShader) with a throwaway profile,
// driven through the DevTools protocol, since a plain headless window cannot be narrower than
// 500 pixels. Set CHROME to the browser's command if it is not google-chrome. Needs node 22.
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, resolve } from "node:path";

const args = process.argv.slice(2);
const shotsAt = args.indexOf("--shots");
const shots = shotsAt >= 0 ? resolve(args.splice(shotsAt, 2)[1]) : null;
const dir = resolve(args[0] ?? "target/site");
const devices = [
  { name: "desktop", width: 1280, height: 900, deviceScaleFactor: 1, mobile: false },
  { name: "phone", width: 390, height: 844, deviceScaleFactor: 3, mobile: true },
];
const scenes = readdirSync("themes")
  .filter((file) => file.endsWith(".toml") && !/^hidden = true$/m.test(readFileSync(join("themes", file), "utf8")))
  .map((file) => file.replace(/\.toml$/, ""));

const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm", ".ogg": "audio/ogg", ".m4a": "audio/mp4" };
const server = createServer((req, res) => {
  const path = join(dir, decodeURIComponent(new URL(req.url, "http://x").pathname).replace(/\/$/, "/index.html"));
  try {
    const body = readFileSync(path);
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" }).end(body);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
const origin = `http://127.0.0.1:${server.address().port}`;

const profile = mkdtempSync(join(tmpdir(), "koi-smoke-"));
const chrome = spawn(
  process.env.CHROME ?? "google-chrome",
  ["--headless=new", "--disable-gpu", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", `--user-data-dir=${profile}`, "--no-first-run", "--remote-debugging-port=0", "about:blank"],
  { stdio: ["ignore", "ignore", "pipe"] },
);
const socketUrl = await new Promise((found, fail) => {
  let text = "";
  chrome.stderr.on("data", (chunk) => {
    text += chunk;
    const match = text.match(/DevTools listening on (ws:\S+)/);
    if (match) found(match[1]);
  });
  chrome.on("exit", () => fail(new Error(`Chrome exited before it listened:\n${text}`)));
});

// One page, over the browser's socket.
const socket = new WebSocket(socketUrl);
await new Promise((open) => socket.addEventListener("open", open));
let next = 0;
const waiting = new Map();
const problems = [];
socket.addEventListener("message", ({ data }) => {
  const message = JSON.parse(data);
  if (message.id !== undefined) {
    const { ok, fail } = waiting.get(message.id);
    waiting.delete(message.id);
    message.error ? fail(new Error(message.error.message)) : ok(message.result);
  } else if (message.method === "Runtime.exceptionThrown") {
    problems.push(message.params.exceptionDetails.exception?.description ?? message.params.exceptionDetails.text);
  } else if (message.method === "Runtime.consoleAPICalled" && message.params.type === "error") {
    problems.push(message.params.args.map((a) => a.value ?? a.description).join(" "));
  }
});
const send = (method, params = {}, sessionId) =>
  new Promise((ok, fail) => {
    const id = next++;
    waiting.set(id, { ok, fail });
    socket.send(JSON.stringify({ id, method, params, sessionId }));
  });

let failed = false;
try {
  for (const device of devices) {
    for (const scene of scenes) {
      const { targetId } = await send("Target.createTarget", { url: "about:blank" });
      const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
      await send("Runtime.enable", {}, sessionId);
      await send("Page.enable", {}, sessionId);
      await send("Emulation.setDeviceMetricsOverride", device, sessionId);
      await send("Emulation.setTouchEmulationEnabled", { enabled: device.mobile, maxTouchPoints: 5 }, sessionId);
      problems.length = 0;
      await send("Page.navigate", { url: `${origin}/?scene=${scene}` }, sessionId);
      await new Promise((wait) => setTimeout(wait, 3000));
      if (shots) {
        const { data } = await send("Page.captureScreenshot", { format: "png" }, sessionId);
        mkdirSync(shots, { recursive: true });
        writeFileSync(join(shots, `${scene}-${device.name}.png`), Buffer.from(data, "base64"));
      }
      await send("Target.closeTarget", { targetId });
      if (problems.length > 0) {
        failed = true;
        console.error(`FAIL ${scene} on ${device.name}:\n  ${problems.join("\n  ")}`);
      } else {
        console.log(`ok   ${scene} on ${device.name}`);
      }
    }
  }

  // Sound: tap the music stone as a person would, since browsers only start sound on a
  // gesture, then drop food across the pond, which chimes, and hold a press, which may pet.
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Runtime.enable", {}, sessionId);
  await send("Page.enable", {}, sessionId);
  await send("Emulation.setDeviceMetricsOverride", devices[0], sessionId);
  problems.length = 0;
  await send("Page.navigate", { url: origin }, sessionId);
  await new Promise((wait) => setTimeout(wait, 2000));
  const value = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true }, sessionId)).result.value;
  const click = async ([x, y], hold = 0) => {
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 }, sessionId);
    await new Promise((wait) => setTimeout(wait, hold));
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 }, sessionId);
  };
  const centre = (id) => `(r => [r.x + r.width / 2, r.y + r.height / 2])(document.getElementById("${id}").getBoundingClientRect())`;
  await click(await value(centre("music-note")));
  const [left, top, width, height] = await value(`(r => [r.x, r.y, r.width, r.height])(document.getElementById("water").getBoundingClientRect())`);
  for (let n = 1; n <= 5; n++) await click([left + (width * n) / 6, top + height / 2], 400);
  await new Promise((wait) => setTimeout(wait, 2500));
  const [played, name] = await value(`[document.getElementById("player").currentTime, document.getElementById("music-title").textContent]`);
  // The HUD: in the garden the time stone steps to the evening and the scene stone to the next
  // scene.
  const sceneName = () => value(`document.getElementById("scene-name").textContent`);
  const hud = [];
  const before = await sceneName();
  await click(await value(centre("time")));
  await new Promise((wait) => setTimeout(wait, 300));
  const later = await sceneName();
  await click(await value(centre("scene")));
  await new Promise((wait) => setTimeout(wait, 300));
  const next = await sceneName();
  if (later === before || next === later) hud.push(`time went ${before} to ${later}, scene to ${next}`);
  await send("Target.closeTarget", { targetId });
  if (hud.length > 0) {
    failed = true;
    console.error(`FAIL HUD: ${hud.join("; ")}`);
  } else {
    console.log(`ok   HUD: time ${before} to ${later}, scene to ${next}`);
  }
  if (problems.length > 0 || !(played > 1)) {
    failed = true;
    console.error(`FAIL sound: ${played.toFixed(1)} s of "${name}" played\n  ${problems.join("\n  ")}`);
  } else {
    console.log(`ok   sound: ${played.toFixed(1)} s of "${name}" played`);
  }
} finally {
  socket.close();
  chrome.kill();
  server.close();
  await new Promise((gone) => chrome.on("exit", gone));
  rmSync(profile, { recursive: true, force: true });
}
process.exit(failed ? 1 : 0);
