/**
 * Pet window entry point.
 *
 * The important part of this file is the render loop, not the rendering:
 * a resident app that keeps a rAF loop alive all day will be uninstalled,
 * however good it looks. The loop parks itself whenever the brain reports
 * a quiescent state and wakes on events.
 */

import { Application, Container } from "pixi.js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Brain } from "./behavior/brain";
import type { Manifest, Notification } from "./petpack";

const UNFOCUSED_FPS = 10;
const FOCUSED_FPS = 60;

async function boot() {
  const manifest = (await invoke("load_active_pack")) as Manifest;

  const app = new Application();
  await app.init({
    width: manifest.canvas.width,
    height: manifest.canvas.height,
    backgroundAlpha: 0, // must stay 0 — see index.html
    antialias: true,
    autoStart: false, // we drive the ticker ourselves
  });
  document.body.appendChild(app.canvas);

  const stage = new Container();
  app.stage.addChild(stage);

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
      app.render();
      // Report the displayed frame so the Rust hit-test loop knows which
      // mask to consult.
      void invoke("set_current_frame", { frameId: currentFrameId(out.state) });
    }

    if (out.quiescent) {
      // Nothing is moving. Stop entirely — this is the difference between
      // "low CPU" and "no CPU".
      running = false;
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

  window.addEventListener("pointerdown", () => {
    brain.handle({ kind: "poke" });
    wake();
  });

  window.addEventListener("focus", () => {
    budgetFps = FOCUSED_FPS;
    wake();
  });
  window.addEventListener("blur", () => {
    budgetFps = UNFOCUSED_FPS;
  });

  await listen<Notification>("deskpet://notify", (event) => {
    // Mapping source -> animation is a pack decision, not a runtime one.
    const _state = manifest.sourceMap?.[event.payload.source]
      ?? manifest.sourceMap?.default
      ?? "notify";
    brain.handle({ kind: "notify" });
    wake();
  });

  wake();
}

void boot();
