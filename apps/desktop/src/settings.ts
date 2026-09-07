/**
 * Settings window: turn a picture into the active pet.
 *
 * The heavy lifting is in Rust (`core::import`). This file only moves bytes
 * and reports what happened — including when it goes wrong, because "I picked
 * a photo and nothing happened" is the failure mode that makes a feature feel
 * broken even when it is merely slow.
 */

import { invoke } from "@tauri-apps/api/core";

const drop = document.getElementById("drop") as HTMLDivElement;
const fileInput = document.getElementById("file") as HTMLInputElement;
const plateSrc = document.getElementById("plate-src") as HTMLDivElement;
const plateOut = document.getElementById("plate-out") as HTMLDivElement;
const go = document.getElementById("go") as HTMLButtonElement;
const status = document.getElementById("status") as HTMLSpanElement;
const backend = document.getElementById("backend") as HTMLElement;

interface ImportResult {
  packId: string;
  backend: string;
  /** `data:` URL of the first generated frame, for the preview. */
  preview: string;
  frames: number;
}

let chosen: { name: string; base64: string } | null = null;

function say(text: string, bad = false) {
  status.textContent = text;
  status.classList.toggle("bad", bad);
}

function showImage(plate: HTMLElement, src: string) {
  plate.replaceChildren(Object.assign(new Image(), { src }));
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

async function choose(file: File) {
  if (!file.type.startsWith("image/")) {
    say(`${file.name} is not an image`, true);
    return;
  }
  const buf = await file.arrayBuffer();
  chosen = { name: file.name, base64: toBase64(buf) };
  showImage(plateSrc, URL.createObjectURL(file));
  plateOut.replaceChildren(
    Object.assign(document.createElement("span"), {
      className: "empty",
      textContent: "no result yet",
    }),
  );
  go.disabled = false;
  say(`${file.name} — ${(file.size / 1024).toFixed(0)} KB`);
}

drop.addEventListener("click", () => fileInput.click());
fileInput.addEventListener("change", () => {
  const f = fileInput.files?.[0];
  if (f) void choose(f);
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
  const f = e.dataTransfer?.files?.[0];
  if (f) void choose(f);
});

go.addEventListener("click", async () => {
  if (!chosen) return;
  go.disabled = true;
  say("cutting out…");

  try {
    const res = (await invoke("import_image", {
      fileName: chosen.name,
      dataBase64: chosen.base64,
    })) as ImportResult;

    showImage(plateOut, res.preview);
    say(`done — ${res.frames} frames, now showing on your desktop`);
    backend.textContent =
      `Cutout by ${res.backend}. Pack id ${res.packId}. ` +
      `Imported packs live in the app data folder and override the bundled one.`;
  } catch (e) {
    say(String(e), true);
  } finally {
    go.disabled = false;
  }
});
