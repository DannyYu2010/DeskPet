/**
 * Pet window entry point.
 *
 * The important part of this file is the render loop, not the rendering:
 * a resident app that keeps a rAF loop alive all day will be uninstalled,
 * however good it looks. The loop parks itself whenever the brain reports
 * a quiescent state and wakes on events.
 */

// MUST come before any renderer is constructed.
//
// PixiJS builds its shader and uniform sync functions with `new Function` by
// default, which our CSP forbids — and forbidding it is the right call for an
// app that will eventually load user-supplied packs. This module swaps those
// generators for polyfills that do the same work without eval. Without it
// `Application.init()` rejects with "Current environment does not allow
// unsafe-eval" and the window renders nothing at all.
//
// Do NOT "fix" this by adding 'unsafe-eval' to the CSP in tauri.conf.json.
import "pixi.js/unsafe-eval";

import { Application } from "pixi.js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Brain } from "./behavior/brain";
import type { LoadedPack, Manifest, Notification } from "./petpack";
import { PetRenderer } from "./render/pet";
import { installDrag } from "./ui/drag";

const UNFOCUSED_FPS = 10;
const FOCUSED_FPS = 60;

/**
 * Send a line to the Rust side so it lands in the same log as everything else.
 *
 * The pet window has no devtools open by default and no visible chrome; a
 * thrown exception there is completely silent from the outside. Anything worth
 * knowing about boot goes through here.
 */
function log(level: "info" | "error", message: string) {
  console[level === "error" ? "error" : "log"](message);
  void invoke("frontend_log", { level, message }).catch(() => {
    /* not running under Tauri (plain browser); console is enough */
  });
}

function describe(e: unknown): string {
  if (e instanceof Error) return `${e.name}: ${e.message}\n${e.stack ?? ""}`;
  return String(e);
}

window.addEventListener("error", (ev) =>
  log("error", `uncaught: ${describe(ev.error ?? ev.message)}`),
);
window.addEventListener("unhandledrejection", (ev) =>
  log("error", `unhandled rejection: ${describe(ev.reason)}`),
);

/**
 * Name whatever is making the document bigger than the window.
 *
 * A scrollbar inside a window that is pretending not to be a window is
 * opaque chrome in the middle of the desktop, so this is worth catching
 * precisely rather than inferring from a screenshot.
 */
function reportOverflow() {
  const d = document.documentElement;
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const over = d.scrollWidth > vw || d.scrollHeight > vh;

  const parts: string[] = [
    `viewport ${vw}x${vh}`,
    `docScroll ${d.scrollWidth}x${d.scrollHeight}`,
    `bodyScroll ${document.body.scrollWidth}x${document.body.scrollHeight}`,
    `overflowing: ${over}`,
  ];

  if (over) {
    for (const el of Array.from(document.querySelectorAll("*"))) {
      const r = el.getBoundingClientRect();
      if (r.right > vw + 0.5 || r.bottom > vh + 0.5 || r.left < -0.5 || r.top < -0.5) {
        const cs = getComputedStyle(el);
        parts.push(
          `OFFENDER <${el.tagName.toLowerCase()}${el.id ? "#" + el.id : ""}` +
            `${el.className ? "." + String(el.className).split(" ").join(".") : ""}> ` +
            `rect=(${r.x.toFixed(1)},${r.y.toFixed(1)} ${r.width.toFixed(1)}x${r.height.toFixed(1)}) ` +
            `position=${cs.position} margin=${cs.margin} transform=${cs.transform}`,
        );
      }
    }
    if (parts.length === 4) parts.push("no single element exceeds the viewport");
  }

  log(over ? "error" : "info", parts.join(" | "));
}

async function boot() {
  let pack: LoadedPack;
  try {
    pack = (await invoke("load_active_pack")) as LoadedPack;
  } catch (e) {
    // No fallback rendering. A pack that will not load is a real failure and
    // has to look like one — a stand-in blob would hide it, and on a
    // transparent window an empty pet is invisible either way.
    log("error", `load_active_pack failed, nothing will be drawn: ${describe(e)}`);
    return;
  }
  const manifest: Manifest = pack.manifest;
  log(
    "info",
    `pack "${manifest.name}" by ${manifest.author} (${manifest.license}) ` +
      `from ${pack.sourceDir}, ${Object.keys(pack.frames).length} frames`,
  );

  const app = new Application();
  await app.init({
    width: manifest.canvas.width,
    height: manifest.canvas.height,
    backgroundAlpha: 0, // must stay 0 — see index.html
    antialias: true,
    autoStart: false, // we drive the ticker ourselves
  });
  document.body.appendChild(app.canvas);

  const pet = new PetRenderer(manifest);
  await pet.load(pack.frames);
  app.stage.addChild(pet.view);

  reportOverflow();

  const brain = new Brain(manifest);

  let running = false;
  let last = performance.now();
  let budgetFps = FOCUSED_FPS;
  let accumulator = 0;

  function wake() {
    if (running) return;
    running = true;
    last = performance.now();
    requestAnimationFrame(frame);
  }

  function frame(now: number) {
    const dt = Math.min((now - last) / 1000, 0.1);
    last = now;

    // Frame pacing: skip render work rather than skip ticks, so behaviour
    // timing stays correct at 10 fps.
    accumulator += dt;
    const step = 1 / budgetFps;

    const out = brain.tick(dt);

    if (accumulator >= step) {
      accumulator = 0;
      pet.setFrame(currentFrameId(out.state));
      app.render();
      // Report the displayed frame so the Rust hit-test loop knows which
      // mask to consult.
      void invoke("set_current_frame", { frameId: currentFrameId(out.state) });
    }

    if (out.quiescent) {
      // Nothing is moving. Stop entirely — this is the difference between
      // "low CPU" and "no CPU".
      running = false;
      pet.setFrame(currentFrameId(out.state));
      app.render();
      return;
    }
    requestAnimationFrame(frame);
  }

  function currentFrameId(state: string): string {
    const def = manifest.states[state];
    if (!def || def.frames.length === 0) return "";
    const i = Math.floor((performance.now() / 1000) * def.fps) % def.frames.length;
    return def.frames[i] ?? "";
  }

  installDrag({
    onDragStart: () => {
      brain.handle({ kind: "drag_start" });
      wake();
    },
    onDragEnd: () => {
      brain.handle({ kind: "drag_end" });
      wake();
    },
    onClick: () => {
      brain.handle({ kind: "poke" });
      wake();
    },
    onError: (m) => log("error", m),
  });

  window.addEventListener("focus", () => {
    budgetFps = FOCUSED_FPS;
    wake();
  });
  window.addEventListener("blur", () => {
    budgetFps = UNFOCUSED_FPS;
  });

  // A new pack means new textures, new masks, a new brain. Rebuilding this
  // module in place would mean unwinding every listener and every cached
  // texture correctly; a reload re-runs the one boot path that is known to
  // produce a correct pet.
  await listen("deskpet://pack-changed", () => {
    log("info", "active pack changed, reloading");
    window.location.reload();
  });

  await listen<Notification>("deskpet://notify", (event) => {
    // Mapping source -> animation is a pack decision, not a runtime one.
    // TODO: route to the mapped state once the pack pipeline lands.
    void (manifest.sourceMap?.[event.payload.source]
      ?? manifest.sourceMap?.default
      ?? "notify");
    brain.handle({ kind: "notify" });
    wake();
  });

  wake();
}

log("info", "main.ts loaded");
void boot().catch((e) => log("error", `boot failed outright: ${describe(e)}`));
