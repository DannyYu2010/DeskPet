/**
 * Settings window: build a pet out of the user's own material.
 *
 * The heavy lifting is in Rust (`core::import`). This file collects assets,
 * decodes clips with the webview's own video decoder, lets the user say which
 * pose each one is, and reports what happened — including failures, because
 * "I added photos and nothing happened" is the failure mode that makes a
 * feature feel broken even when it is only slow.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { extractFrames } from "./ui/video";
import type { LoadedPack } from "./petpack";

/** Poses the runtime knows about. `idle` is required; the rest are optional
 *  and fall back to idle when missing (see PETPACK_SPEC.md). */
const STATES: { value: string; label: string }[] = [
  { value: "idle", label: "日常陪伴 — 平时循环显示" },
  { value: "launch", label: "程序启动 — 打开时播放一次" },
  { value: "sleep", label: "睡眠模式 — 长时间未互动后显示" },
  { value: "lie_down", label: "进入睡眠 — 日常到睡眠的过渡" },
  { value: "wake", label: "醒来起身 — 睡眠到日常的过渡" },
  { value: "poke", label: "点击回应 — 点击宠物后播放" },
  { value: "notify", label: "通知回应 — 收到提醒后播放" },
];

/** Limits from docs/IMPORTING.md, enforced here so a bad file is named and
 *  skipped rather than silently producing a bad pet. */
const MAX_IMAGE_BYTES = 25 * 1024 * 1024;
const MAX_CLIP_BYTES = 60 * 1024 * 1024;
const MAX_CLIP_SECONDS = 10;
const MIN_EDGE = 256;

function log(level: "info" | "error", message: string) {
  console[level === "error" ? "error" : "log"](message);
  void invoke("frontend_log", { level, message }).catch(() => {});
}

window.addEventListener("error", (ev) =>
  log("error", `settings uncaught: ${String(ev.error ?? ev.message)}`),
);
window.addEventListener("unhandledrejection", (ev) =>
  log("error", `settings unhandled rejection: ${String(ev.reason)}`),
);

const drop = document.getElementById("drop") as HTMLDivElement;
const fileInput = document.getElementById("file") as HTMLInputElement;
const list = document.getElementById("assets") as HTMLDivElement;
const go = document.getElementById("go") as HTMLButtonElement;
const status = document.getElementById("status") as HTMLSpanElement;
const backend = document.getElementById("backend") as HTMLElement;
const sleepAfter = document.getElementById("sleep-after") as HTMLSelectElement;
const sleepStatus = document.getElementById("sleep-status") as HTMLSpanElement;
const startAtLogin = document.getElementById("start-at-login") as HTMLInputElement;
const startAtLoginStatus = document.getElementById("start-at-login-status") as HTMLSpanElement;
const actionName = document.getElementById("action-name") as HTMLInputElement;
const actionTrigger = document.getElementById("action-trigger") as HTMLSelectElement;
const actionUpload = document.getElementById("action-upload") as HTMLButtonElement;

for (const tab of Array.from(document.querySelectorAll<HTMLButtonElement>(".tab"))) {
  tab.addEventListener("click", () => {
    const page = tab.dataset.page;
    for (const candidate of Array.from(document.querySelectorAll<HTMLButtonElement>(".tab"))) {
      candidate.classList.toggle("active", candidate === tab);
    }
    for (const section of Array.from(document.querySelectorAll<HTMLElement>(".page"))) {
      section.hidden = section.id !== page;
    }
  });
}

document.getElementById("contact-email")?.addEventListener("click", (event) => {
  event.preventDefault();
  void openUrl("mailto:yuziyi2010@qq.com");
});

void invoke("get_config").then((raw) => {
  const cfg = raw as { sleep_after_seconds?: number };
  sleepAfter.value = String(cfg.sleep_after_seconds ?? 90);
});

void invoke<boolean>("get_start_at_login")
  .then((enabled) => {
    startAtLogin.checked = enabled;
  })
  .catch((e) => {
    startAtLogin.disabled = true;
    startAtLoginStatus.textContent = `读取失败：${String(e)}`;
  });

startAtLogin.addEventListener("change", async () => {
  const enabled = startAtLogin.checked;
  startAtLogin.disabled = true;
  startAtLoginStatus.textContent = "保存中…";
  try {
    startAtLogin.checked = await invoke<boolean>("set_start_at_login", { enabled });
    startAtLoginStatus.textContent = "已保存";
  } catch (e) {
    startAtLogin.checked = !enabled;
    startAtLoginStatus.textContent = `保存失败：${String(e)}`;
  } finally {
    startAtLogin.disabled = false;
  }
});

sleepAfter.addEventListener("change", async () => {
  sleepAfter.disabled = true;
  sleepStatus.textContent = "保存中…";
  try {
    await invoke("set_sleep_after_seconds", { seconds: Number(sleepAfter.value) });
    sleepStatus.textContent = "已保存";
  } catch (e) {
    sleepStatus.textContent = `保存失败：${String(e)}`;
  } finally {
    sleepAfter.disabled = false;
  }
});

interface Asset {
  id: number;
  name: string;
  state: string;
  isClip: boolean;
  rotation: number;
  frames: string[];
  thumbUrl: string;
  note: string;
  label?: string;
  trigger?: "singleClick" | "doubleClick";
}

interface Assignment {
  state: string;
  label?: string;
  trigger?: "singleClick" | "doubleClick";
}

let nextId = 1;
const assets: Asset[] = [];
let pendingAssignment: Assignment | null = null;
let installedPack: LoadedPack | null = null;
let installedPackError = false;

async function refreshInstalledPack() {
  try {
    installedPack = await invoke<LoadedPack>("load_active_pack");
    installedPackError = false;
  } catch (e) {
    installedPackError = true;
    log("error", `reading installed material failed: ${String(e)}`);
  }
  render();
}

void refreshInstalledPack();
void listen("deskpet://pack-changed", () => void refreshInstalledPack());

function say(text: string, bad = false) {
  status.textContent = text;
  status.classList.toggle("bad", bad);
  log(bad ? "error" : "info", `settings: ${text}`);
}

async function add(file: File, assignment?: Assignment) {
  const isClip = file.type.startsWith("video/");
  const isImage = file.type.startsWith("image/");
  if (!isClip && !isImage) {
    say(`${file.name} is neither an image nor a clip`, true);
    return;
  }

  const cap = isClip ? MAX_CLIP_BYTES : MAX_IMAGE_BYTES;
  if (file.size > cap) {
    say(`${file.name} is ${(file.size / 1048576).toFixed(0)} MB — the limit is ${cap / 1048576} MB`, true);
    return;
  }

  const url = URL.createObjectURL(file);
  let frames: string[];
  let note: string;

  if (isClip) {
    say(`decoding ${file.name}…`);
    const clip = await extractFrames(file, (done, total) => {
      say(`decoding ${file.name} — frame ${done} of ${total}`);
    });
    if (clip.duration > MAX_CLIP_SECONDS + 0.5) {
      say(`${file.name} is ${clip.duration.toFixed(1)}s — clips have to be ${MAX_CLIP_SECONDS}s or shorter`, true);
      URL.revokeObjectURL(url);
      return;
    }
    frames = clip.frames;
    note = `clip · ${clip.frames.length} frames · ${clip.duration.toFixed(1)}s`;
  } else {
    const dims = await imageSize(url);
    if (Math.min(dims.w, dims.h) < MIN_EDGE) {
      say(`${file.name} is ${dims.w}×${dims.h} — the short edge has to be at least ${MIN_EDGE}px`, true);
      URL.revokeObjectURL(url);
      return;
    }
    frames = [toBase64(await file.arrayBuffer())];
    note = `photo · ${dims.w}×${dims.h}`;
  }

  // A named slot owns one asset. Re-uploading replaces it, which keeps the
  // settings page understandable and prevents ambiguous interaction targets.
  const state = assignment?.state ?? (assets.some((a) => a.state === "idle") ? "sleep" : "idle");
  if (assignment) {
    const existing = assets.findIndex((a) => a.state === state);
    if (existing >= 0) {
      URL.revokeObjectURL(assets[existing]!.thumbUrl);
      assets.splice(existing, 1);
    }
  }
  assets.push({
    id: nextId++,
    name: file.name,
    state,
    isClip,
    rotation: 0,
    frames,
    thumbUrl: url,
    note,
    label: assignment?.label,
    trigger: assignment?.trigger,
  });
  log("info", `added ${file.name} (${note}) as ${state}`);
  render();
  say(`${assets.length} item${assets.length === 1 ? "" : "s"} ready`);
}

function render() {
  list.replaceChildren(
    ...assets.map((a) => {
      const row = document.createElement("div");
      row.className = "asset";

      const thumb = document.createElement("div");
      thumb.className = "thumb";
      const img = new Image();
      img.src = a.thumbUrl;
      img.style.transform = `rotate(${a.rotation}deg)`;
      thumb.append(img);

      const meta = document.createElement("div");
      meta.className = "meta";
      const name = document.createElement("div");
      name.className = "name";
      name.textContent = a.name;
      const sub = document.createElement("div");
      sub.className = "sub";
      const triggerText = a.trigger === "singleClick" ? "单击" : a.trigger === "doubleClick" ? "双击" : "";
      sub.textContent = [a.note, a.label, triggerText].filter(Boolean).join(" · ");
      meta.append(name, sub);

      const rotate = document.createElement("button");
      rotate.className = "ghost";
      rotate.title = "Rotate right — the head should point up";
      rotate.textContent = "↻";
      rotate.addEventListener("click", () => {
        a.rotation = (a.rotation + 90) % 360;
        render();
      });

      const select = document.createElement("select");
      const selectableStates = a.trigger
        ? [{ value: a.state, label: `${a.label ?? "自定义动作"} — ${triggerText}` }]
        : STATES;
      for (const s of selectableStates) {
        const opt = document.createElement("option");
        opt.value = s.value;
        opt.textContent = s.label;
        opt.selected = s.value === a.state;
        select.append(opt);
      }
      select.addEventListener("change", () => {
        a.state = select.value;
        a.label = undefined;
        a.trigger = undefined;
        log("info", `${a.name} is now ${a.state}`);
        render();
      });

      const remove = document.createElement("button");
      remove.className = "drop-me";
      remove.title = "Remove";
      remove.textContent = "✕";
      remove.addEventListener("click", () => {
        const i = assets.findIndex((x) => x.id === a.id);
        if (i >= 0) {
          URL.revokeObjectURL(assets[i]!.thumbUrl);
          assets.splice(i, 1);
        }
        render();
      });

      row.append(thumb, meta, rotate, select, remove);
      return row;
    }),
  );

  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-status]"))) {
    const state = el.dataset.status;
    const asset = assets.find((candidate) => candidate.state === state);
    const installed = state ? installedPack?.manifest.states[state] : undefined;
    const directory = installedPack ? `${installedPack.sourceDir}/sprites` : "";
    el.textContent = asset
      ? `待应用：${asset.name}`
      : installed
        ? `已配置 · 素材目录：${directory}`
        : installedPackError ? "读取素材失败，请重新打开设置" : installedPack ? "未上传" : "读取中…";
    el.title = asset ? asset.name : installed ? `${directory}\n${installed.frames.join("\n")}` : "";
    el.classList.toggle("ready", Boolean(asset || installed));
  }
  for (const button of Array.from(document.querySelectorAll<HTMLButtonElement>(".slot-upload"))) {
    const state = button.dataset.state!;
    const configured = Boolean(installedPack?.manifest.states[state]) || assets.some((a) => a.state === state);
    button.textContent = configured ? "更改" : "上传";
  }

  go.disabled = !assets.some((a) => a.state === "idle");
  if (assets.length > 0 && go.disabled) {
    say("one of them has to be the idle pose", true);
  }
}

function imageSize(url: string): Promise<{ w: number; h: number }> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve({ w: img.naturalWidth, h: img.naturalHeight });
    img.onerror = () => reject(new Error("could not read that image"));
    img.src = url;
  });
}

function toBase64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  // Chunked: String.fromCharCode(...bytes) blows the argument limit on
  // anything bigger than a thumbnail.
  let s = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    s += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(s);
}

function chooseFiles(assignment: Assignment | null) {
  pendingAssignment = assignment;
  fileInput.multiple = assignment === null;
  fileInput.click();
}

for (const button of Array.from(document.querySelectorAll<HTMLButtonElement>(".slot-upload"))) {
  button.addEventListener("click", () => chooseFiles({ state: button.dataset.state! }));
}

actionUpload.addEventListener("click", () => {
  const label = actionName.value.trim();
  if (!label) {
    say("请先填写动作名称，例如“奔跑”", true);
    actionName.focus();
    return;
  }
  const trigger = actionTrigger.value as "singleClick" | "doubleClick";
  chooseFiles({
    state: trigger === "singleClick" ? "custom_single_click" : "custom_double_click",
    label,
    trigger,
  });
});

drop.addEventListener("click", () => chooseFiles(null));
fileInput.addEventListener("change", async () => {
  const selected = Array.from(fileInput.files ?? []);
  const assignment = pendingAssignment;
  pendingAssignment = null;
  if (assignment && selected[0]) {
    await add(selected[0], assignment);
    actionName.value = "";
  } else {
    for (const f of selected) await add(f);
  }
  fileInput.value = "";
});

for (const ev of ["dragenter", "dragover"] as const) {
  drop.addEventListener(ev, (e) => {
    e.preventDefault();
    drop.classList.add("over");
  });
}
for (const ev of ["dragleave", "drop"] as const) {
  drop.addEventListener(ev, (e) => {
    e.preventDefault();
    drop.classList.remove("over");
  });
}
drop.addEventListener("drop", (e) => {
  pendingAssignment = null;
  for (const f of Array.from(e.dataTransfer?.files ?? [])) void add(f);
});

// The first import downloads the matte model — minutes on a slow connection.
// Showing progress is the difference between "slow" and "frozen".
void listen<string>("deskpet://import-progress", (e) => say(e.payload));

go.addEventListener("click", async () => {
  if (assets.length === 0) return;
  go.disabled = true;
  say("starting…");

  try {
    const res = (await invoke("import_assets", {
      name: assets.find((a) => a.state === "idle")?.name.replace(/\.[^.]+$/, "") ?? "My pet",
      assets: assets.map((a) => ({
        state: a.state,
        frames: a.frames,
        rotateDegrees: a.rotation,
        isClip: a.isClip,
        label: a.label,
        trigger: a.trigger,
      })),
    })) as { packId: string; backend: string; preview: string; frames: number };

    say(`done — ${res.frames} frames, now on your desktop`);
    backend.textContent =
      `Cutout by ${res.backend}. Pack id ${res.packId}. ` +
      `Imported packs live in the app data folder and override the bundled one.`;
    for (const asset of assets) URL.revokeObjectURL(asset.thumbUrl);
    assets.length = 0;
    await refreshInstalledPack();
  } catch (e) {
    say(String(e), true);
    log("error", `import_assets rejected: ${JSON.stringify(String(e))}`);
  } finally {
    render();
  }
});
