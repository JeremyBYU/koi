// The page around the pond: sizes the canvas, draws each frame the wasm pond makes, and
// turns taps, keys and the painted stones into what they do in the terminal.
import init, { Icon, Pond, Touch } from "./pkg/koi_web.js";

const FOODS = ["pellets", "flakes", "petals", "seeds", "a treat"];
// The synth's share of the volume, as the terminal's audio.ambient_volume, and how far ahead of
// the speakers it is made. ?ambient= in the address sets the level of its bed, the drone and
// the water's rumble, from 0 to 1, for speakers too small for them.
const AMBIENT_VOLUME = 0.6;
const BED = Math.min(Math.max(Number(new URLSearchParams(location.search).get("ambient") ?? 1) || 0, 0), 1);
const SOUND_LEAD = 0.2;
// The canvas's pixel budget, and its most pixels per CSS pixel. The koi are posed on one core
// at the canvas's resolution, so a large or dense screen gets a canvas of fewer pixels than
// it has, which the browser scales up, as the terminal caps its CPU koi.
const MAX_PIXELS = 1.2e6;
const MAX_DENSITY = 1.5;

const $ = (id) => document.getElementById(id);
const saved = (key) => {
  try {
    return localStorage.getItem(`koi.${key}`);
  } catch {
    return null;
  }
};
const save = (key, value) => {
  try {
    localStorage.setItem(`koi.${key}`, String(value));
  } catch {
    // Private windows may refuse storage; the page works the same without it.
  }
};

const pondBox = $("pond");
const canvas = $("water");
const hud = $("hud");
const card = $("card");
const player = $("player");

init().then(
  (wasm) => {
    // The pond is made once its box has a size: a page opened in the background, or before
    // the browser has sized its window, lays the box out at zero width first.
    new ResizeObserver((_, observer) => {
      if (pondBox.clientWidth < 64 || pondBox.clientHeight < 64) return;
      observer.disconnect();
      start(wasm);
    }).observe(pondBox);
  },
  (error) => {
    $("loading").textContent = "The pond didn't load. Try reloading, or use a recent Chrome, Firefox or Safari.";
    console.error(error);
  },
);

function start(wasm) {
  const ctx = canvas.getContext("2d", { alpha: false });
  const density = (w, h) => {
    let d = Math.min(window.devicePixelRatio || 1, MAX_DENSITY);
    while (d > 1 && w * h * d * d > MAX_PIXELS) d -= 0.25;
    return Math.min(d, Math.sqrt(MAX_PIXELS / (w * h)));
  };
  let size = [pondBox.clientWidth, pondBox.clientHeight];
  const pond = new Pond(size[0], size[1], density(...size), new URLSearchParams(location.search).get("scene") ?? saved("theme") ?? "summer-garden", (Math.random() * 2 ** 32) >>> 0);
  let food = Math.min(Number(saved("food")) || 0, FOODS.length - 1);

  // Drawing, in layers: the water image scaled to the canvas, the koi sprites through a sheet
  // canvas, and the food from an atlas painted once per theme and size. rAF stops in a hidden
  // tab, and the loop also rests while the pond is scrolled out of sight. A device that cannot
  // draw a frame in 12 ms draws every other one.
  const water = document.createElement("canvas");
  const waterCtx = water.getContext("2d");
  const sheet = document.createElement("canvas");
  const sheetCtx = sheet.getContext("2d");
  const atlas = document.createElement("canvas");
  let foodRects = [];
  const pixels = (ptr, w, h) => new ImageData(new Uint8ClampedArray(wasm.memory.buffer, ptr, w * h * 4), w, h);
  const fitCanvas = () => {
    canvas.width = pond.frame_width();
    canvas.height = pond.frame_height();
    const sides = Array.from({ length: pond.food_sprites() }, (_, i) => pond.food_side(i));
    atlas.width = sides.reduce((sum, side) => sum + side, 0);
    atlas.height = Math.max(...sides);
    let x = 0;
    foodRects = sides.map((side, i) => {
      const rgba = pond.food_sprite(i);
      atlas.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.length), side, side), x, 0);
      x += side;
      return [x - side, side];
    });
  };
  const draw = (now) => {
    const fresh = pond.tick(now);
    const koi = pond.koi_places();
    const food = pond.food_places();
    const k = pond.pixel();
    const [iw, ih] = [pond.water_width(), pond.water_height()];
    if (fresh) {
      if (water.width !== iw || water.height !== ih) [water.width, water.height] = [iw, ih];
      waterCtx.putImageData(pixels(pond.water_ptr(), iw, ih), 0, 0);
    }
    ctx.imageSmoothingEnabled = k === 0;
    if (k > 0) ctx.drawImage(water, 0, 0, iw * k, ih * k);
    else ctx.drawImage(water, 0, 0, canvas.width, canvas.height);
    ctx.imageSmoothingEnabled = false;
    let rows = 0;
    let widest = 0;
    for (let i = 0; i < koi.length; i += 5) {
      widest = Math.max(widest, koi[i + 1]);
      rows += koi[i + 2];
    }
    if (sheet.width < widest || sheet.height < rows) [sheet.width, sheet.height] = [Math.max(sheet.width, widest), Math.max(sheet.height, rows)];
    const base = pond.koi_ptr();
    for (let i = 0, top = 0; i < koi.length; i += 5) {
      const [at, w, h, x, y] = koi.subarray(i, i + 5);
      sheetCtx.putImageData(pixels(base + at, w, h), 0, top);
      ctx.drawImage(sheet, 0, top, w, h, x, y, w * Math.max(k, 1), h * Math.max(k, 1));
      top += h;
    }
    for (let i = 0; i < food.length; i += 3) {
      const [sx, side] = foodRects[food[i]];
      ctx.drawImage(atlas, sx, 0, side, side, food[i + 1], food[i + 2], side, side);
    }
    pumpSound();
  };
  let raf = 0;
  let onScreen = true;
  let cost = 0;
  let odd = false;
  const frame = (now) => {
    raf = 0;
    odd = !odd;
    if (cost <= 12 || odd) {
      const began = performance.now();
      draw(now);
      cost = cost * 0.9 + (performance.now() - began) * 0.1;
    }
    schedule();
  };
  const schedule = () => {
    if (!raf && onScreen && !document.hidden) raf = requestAnimationFrame(frame);
  };
  fitCanvas();
  new IntersectionObserver(([entry]) => {
    onScreen = entry.isIntersecting;
    schedule();
  }).observe(pondBox);

  let resizing = 0;
  new ResizeObserver(() => {
    clearTimeout(resizing);
    resizing = setTimeout(() => {
      const next = [pondBox.clientWidth, pondBox.clientHeight];
      if (next[0] === size[0] && next[1] === size[1]) return;
      size = next;
      pond.resize(size[0], size[1], density(...size));
      fitCanvas();
      layoutHud();
    }, 150);
  }).observe(pondBox);

  // The pond. A press on a koi pets it and holding keeps the hand there; anywhere else drops
  // food. A finger gets a wider reach than a mouse.
  let holding = null;
  const at = (e) => {
    const r = canvas.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  canvas.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    closeTray();
    if (pond.press(...at(e), food, e.pointerType === "mouse" ? 0.25 : 0.5) === Touch.Petted) {
      holding = e.pointerId;
      canvas.setPointerCapture(e.pointerId);
    }
  });
  canvas.addEventListener("pointermove", (e) => {
    if (e.pointerId === holding) pond.drag(...at(e));
  });
  const letGo = (e) => {
    if (e.pointerId !== holding) return;
    holding = null;
    pond.release();
  };
  canvas.addEventListener("pointerup", letGo);
  canvas.addEventListener("pointercancel", letGo);
  canvas.addEventListener("lostpointercapture", letGo);
  canvas.addEventListener("contextmenu", (e) => e.preventDefault());

  // The scene: the page takes its colours, and the stones are painted again in them.
  const themeColor = document.querySelector('meta[name="theme-color"]');
  const applyTheme = () => {
    document.documentElement.style.cssText = pond.page_style();
    themeColor.content = getComputedStyle(document.documentElement).getPropertyValue("--ground").trim();
    $("scene-name").textContent = pond.theme_name();
    pondBox.classList.toggle("pixel", pond.pixel() > 0);
    const times = pond.has_times();
    $("time").classList.toggle("only", !times);
    $("time").setAttribute("aria-label", times ? "Later in the day" : `Time of day: ${pond.time_name()}, the only one in this scene`);
    save("theme", pond.theme_id());
    paintHud();
  };
  const nextScene = (forward) => pond.next_scene(forward) && (fitCanvas(), applyTheme());
  // Most scenes have one time of day; there the stone says so instead of doing nothing.
  let chipTimer = 0;
  const nextTime = (later) => {
    if (pond.next_time(later)) {
      fitCanvas();
      applyTheme();
      return;
    }
    const time = pond.time_name();
    $("chip").textContent = `${time[0].toUpperCase()}${time.slice(1)} only here`;
    $("chip").hidden = false;
    clearTimeout(chipTimer);
    chipTimer = setTimeout(() => {
      $("chip").hidden = true;
    }, 2200);
  };

  // The stones, painted by the game at the device's pixel density. --cell is the terminal's
  // cell height: 20 px, less on a narrow pond, and the music stone drops its volume dots
  // and title before the stones shrink.
  let cell = 20;
  let cols = 30;
  let lifted = null;
  const paint = (target, rgba, w, h) => {
    target.width = w;
    target.height = h;
    target.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.length), w, h), 0, 0);
  };
  const deviceCell = () => Math.round(cell * Math.min(window.devicePixelRatio || 1, 3));
  const pebble = (button, icon, index, seed) => {
    const c = deviceCell();
    paint(button.querySelector("canvas"), pond.paint_pebble(icon, index, c, lifted === button, seed), 3 * c, 3 * c);
  };
  const music = $("music");
  const paintMusic = () => {
    const c = deviceCell();
    const w = Math.floor((cols * c) / 2);
    paint(music.querySelector("canvas"), pond.paint_music(cols, c, volume, !playing, lifted === music, 1), w, 3 * c);
    music.style.width = `${(cols * cell) / 2}px`;
    music.classList.toggle("off", !playing);
    $("music-title").hidden = cols < 30;
    $("music-volume").hidden = cols < 30;
  };
  const paintHud = () => {
    paintMusic();
    pebble($("food"), Icon.Food, food, 2);
    pebble($("scene"), Icon.Scene, 0, 3);
    pebble($("time"), Icon.Time, 0, 4);
    pebble($("help"), Icon.Help, 0, 5);
    if (!tray.hidden) paintTray();
  };
  const layoutHud = () => {
    const room = pondBox.clientWidth - 32;
    const need = (c, n) => (n * c) / 2 + 4 * 3 * c + 4 * 0.35 * c;
    [cell, cols] = [20, 30];
    if (need(cell, cols) > room) cols = 10;
    if (need(cell, cols) > room) cell = Math.max(12, Math.floor(room / (cols / 2 + 13.4)));
    hud.style.setProperty("--cell", `${cell}px`);
    paintHud();
  };
  for (const button of [music, $("food"), $("scene"), $("time"), $("help")]) {
    button.addEventListener("pointerenter", (e) => {
      if (e.pointerType !== "mouse") return;
      lifted = button;
      paintHud();
    });
    button.addEventListener("pointerleave", () => {
      if (lifted !== button) return;
      lifted = null;
      paintHud();
    });
  }

  // The food tray, a row of pebbles over the food stone.
  const tray = $("tray");
  const foodButton = $("food");
  const paintTray = () => {
    tray.querySelectorAll("button").forEach((button, i) => {
      const c = Math.round(deviceCell() * 0.87);
      paint(button.querySelector("canvas"), pond.paint_pebble(Icon.Food, i, c, i === food, 10 + i), 3 * c, 3 * c);
      button.setAttribute("aria-checked", String(i === food));
    });
  };
  FOODS.forEach((name, i) => {
    const button = document.createElement("button");
    button.setAttribute("role", "menuitemradio");
    button.setAttribute("aria-label", `${name} (${i + 1})`);
    button.append(document.createElement("canvas"));
    button.addEventListener("click", () => {
      pickFood(i);
      closeTray();
      foodButton.focus();
    });
    tray.append(button);
  });
  const closeTray = () => {
    tray.hidden = true;
    foodButton.setAttribute("aria-expanded", "false");
  };
  const pickFood = (i) => {
    food = i;
    save("food", i);
    foodButton.setAttribute("aria-label", `Food: ${FOODS[i]}`);
    paintHud();
  };
  foodButton.addEventListener("click", () => {
    tray.hidden = !tray.hidden;
    foodButton.setAttribute("aria-expanded", String(!tray.hidden));
    if (!tray.hidden) paintTray();
  });
  $("scene").addEventListener("click", () => nextScene(true));
  $("time").addEventListener("click", () => nextTime(true));
  $("help").addEventListener("click", () => card.showModal());

  // Sound: the three tracks built into the binary, and the ambient sound, chimes and petting
  // sound, which koi-synth makes in the wasm and synth.js plays. Nothing sounds until the
  // music stone is tapped, which makes the Web Audio graph within that tap, as browsers
  // require. The music goes through it too, for each track's loudness gain and a volume that
  // works on iPhones. The browser picks Ogg Vorbis, or AAC where it has no Vorbis.
  const title = $("music-title");
  let tracks = [];
  let track = 0;
  fetch("music/tracks.json")
    .then((response) => response.json())
    .then((list) => {
      tracks = list;
      // Every visit opens with Clear Waters, as the terminal does.
      track = Math.max(0, tracks.findIndex((t) => t.file === "clear-waters"));
    })
    .catch(() => {
      title.textContent = "No music";
    });
  let volume = Math.min(Math.max(Number(saved("volume") ?? 0.6), 0), 1);
  let playing = false;
  let audio = null;
  const load = () => {
    track %= tracks.length;
    const { file, title: name, gain } = tracks[track];
    const source = (ext, type) => Object.assign(document.createElement("source"), { src: `music/${file}.${ext}`, type });
    player.replaceChildren(source("ogg", 'audio/ogg; codecs="vorbis"'), source("m4a", 'audio/mp4; codecs="mp4a.40.2"'));
    player.load();
    audio.music.gain.value = gain;
    title.textContent = name;
  };
  const connect = () => {
    try {
      // Plays through an iPhone's silent switch, as music does.
      navigator.audioSession.type = "playback";
    } catch {
      // Only Safari has it.
    }
    const ctx = new AudioContext({ sampleRate: Pond.sound_rate(), latencyHint: "playback" });
    const master = ctx.createGain();
    master.connect(ctx.destination);
    const music = ctx.createGain();
    ctx.createMediaElementSource(player).connect(music).connect(master);
    audio = { ctx, master, music, synth: null, sent: 0 };
    pond.set_bed(BED);
    // Without worklets, or at another rate, the music plays alone.
    if (ctx.audioWorklet && ctx.sampleRate === Pond.sound_rate()) {
      ctx.audioWorklet.addModule("synth.js").then(() => {
        const synth = new AudioWorkletNode(ctx, "koi-synth", { outputChannelCount: [2] });
        const ambient = ctx.createGain();
        ambient.gain.value = AMBIENT_VOLUME;
        synth.connect(ambient).connect(master);
        Object.assign(audio, { synth, sent: ctx.currentTime * ctx.sampleRate });
      }, console.error);
    }
  };
  // Keeps SOUND_LEAD seconds of the ambient sound queued, from the draw loop.
  const pumpSound = () => {
    if (!playing || !audio?.synth) return;
    const played = audio.ctx.currentTime * audio.ctx.sampleRate;
    audio.sent = Math.max(audio.sent, played);
    const want = Math.floor(played + SOUND_LEAD * audio.ctx.sampleRate - audio.sent);
    if (want < 512) return;
    const chunk = pond.sound(want);
    audio.synth.port.postMessage(chunk, [chunk.buffer]);
    audio.sent += want;
  };
  const play = () => {
    if (tracks.length === 0) return;
    if (!audio) connect();
    audio.ctx.resume();
    audio.master.gain.value = volume;
    if (!player.firstChild) load();
    playing = true;
    pond.set_sound(true);
    // A new track interrupts the last one's play(), which then rejects with AbortError;
    // only a failure of the track loaded now is worth showing.
    const loaded = track;
    player.play().catch((error) => {
      if (error.name !== "AbortError" && loaded === track) title.textContent = "Music could not play";
    });
    $("music-note").setAttribute("aria-label", "Pause sound");
    paintMusic();
  };
  const pause = () => {
    playing = false;
    pond.set_sound(false);
    player.pause();
    audio?.ctx.suspend();
    $("music-note").setAttribute("aria-label", "Play music");
    paintMusic();
  };
  const nextTrack = () => {
    if (tracks.length === 0) return;
    if (!audio) connect();
    track += 1;
    load();
    play();
  };
  const setVolume = (v) => {
    volume = Math.round(Math.min(Math.max(v, 0), 1) * 10) / 10;
    if (audio) audio.master.gain.value = volume;
    save("volume", volume);
    paintMusic();
  };
  player.addEventListener("ended", nextTrack);
  // A track that will not load is skipped, once round the list at most. With <source>
  // children the error fires on each source in turn, so only the last one's counts.
  let failures = 0;
  player.addEventListener(
    "error",
    (event) => {
      if (event.target !== player.lastElementChild) return;
      failures += 1;
      if (failures < tracks.length) nextTrack();
      else title.textContent = "Music could not play";
    },
    true,
  );
  player.addEventListener("playing", () => {
    failures = 0;
  });
  $("music-note").addEventListener("click", () => (playing ? pause() : play()));
  $("music-next").addEventListener("click", nextTrack);
  $("music-volume").addEventListener("click", (e) => {
    const r = e.currentTarget.getBoundingClientRect();
    setVolume((e.clientX - r.left) / r.width + 0.1);
  });
  // A hidden tab stops drawing, and so making the ambient sound: everything rests until the
  // page is back.
  document.addEventListener("visibilitychange", () => {
    if (playing && document.hidden) {
      player.pause();
      audio.ctx.suspend();
    } else if (playing) {
      audio.ctx.resume();
      player.play().catch(() => {});
    }
    schedule();
  });
  window.addEventListener("blur", () => {
    if (holding !== null) {
      holding = null;
      pond.release();
    }
  });

  // The terminal's keys, less Tab, which moves between the stones here; h hides them.
  const keys = {
    f: () => pond.feed_anywhere(food),
    p: () => pond.pet_nearest(),
    t: () => nextScene(true),
    T: () => nextScene(false),
    l: () => nextTime(true),
    L: () => nextTime(false),
    n: nextTrack,
    m: () => (playing ? pause() : play()),
    "+": () => setVolume(volume + 0.1),
    "=": () => setVolume(volume + 0.1),
    "-": () => setVolume(volume - 0.1),
    "?": () => card.showModal(),
    h: () => hud.classList.toggle("away"),
    Escape: closeTray,
  };
  FOODS.forEach((_, i) => {
    keys[String(i + 1)] = () => pickFood(i);
  });
  window.addEventListener("keydown", (e) => {
    if (e.metaKey || e.ctrlKey || e.altKey || card.open) return;
    const act = keys[e.key];
    if (!act) return;
    e.preventDefault();
    act();
  });

  pickFood(food);
  applyTheme();
  layoutHud();
  $("loading").remove();
  hud.hidden = false;
  schedule();
  requestAnimationFrame(() => document.documentElement.classList.add("settled"));
}
