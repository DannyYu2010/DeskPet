/**
 * Turning a clip into frames, using the webview's own decoder.
 *
 * DeskPet ships no video library. Decoding H.264 in Rust means linking ffmpeg
 * (LGPL, tens of megabytes, a packaging problem on both platforms) or a
 * patent-encumbered decoder — and neither is necessary, because the webview
 * already has a decoder that both WKWebView and WebView2 provide. Seeking a
 * `<video>` onto a `<canvas>` is the whole implementation.
 */

/** Frames per second we sample at. Above this the pack grows fast and the
 *  pet does not read as any smoother. */
const SAMPLE_FPS = 12;
/** Hard ceiling on frames per clip. At 12 fps this is ten seconds, which is
 *  also the documented limit in docs/IMPORTING.md. */
const MAX_FRAMES = 120;

export interface ClipFrames {
  frames: string[]; // base64 PNG, no data: prefix
  width: number;
  height: number;
  duration: number;
}

export async function extractFrames(
  file: File,
  onProgress?: (done: number, total: number) => void,
): Promise<ClipFrames> {
  const url = URL.createObjectURL(file);
  try {
    const video = document.createElement("video");
    video.src = url;
    video.muted = true;
    video.playsInline = true;
    // Without this the first seek can resolve before any pixels exist and the
    // opening frames come out blank.
    video.preload = "auto";

    await once(video, "loadeddata");

    const duration = video.duration;
    if (!isFinite(duration) || duration <= 0) {
      throw new Error("that clip has no readable duration");
    }

    const count = Math.min(Math.ceil(duration * SAMPLE_FPS), MAX_FRAMES);
    const canvas = document.createElement("canvas");
    canvas.width = video.videoWidth;
    canvas.height = video.videoHeight;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("could not get a 2d canvas");

    const frames: string[] = [];
    for (let i = 0; i < count; i++) {
      // Sample at frame centres. Seeking to exactly 0 or exactly the duration
      // lands on boundaries some decoders refuse to render.
      video.currentTime = ((i + 0.5) / count) * duration;
      await once(video, "seeked");
      ctx.drawImage(video, 0, 0);
      frames.push(toBase64Png(canvas));
      onProgress?.(i + 1, count);
    }

    return { frames, width: canvas.width, height: canvas.height, duration };
  } finally {
    URL.revokeObjectURL(url);
  }
}

function once(el: HTMLMediaElement, event: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const ok = () => {
      cleanup();
      resolve();
    };
    const fail = () => {
      cleanup();
      reject(new Error(`the clip could not be decoded (${el.error?.message ?? event})`));
    };
    const cleanup = () => {
      el.removeEventListener(event, ok);
      el.removeEventListener("error", fail);
    };
    el.addEventListener(event, ok, { once: true });
    el.addEventListener("error", fail, { once: true });
  });
}

function toBase64Png(canvas: HTMLCanvasElement): string {
  // `toDataURL` rather than `toBlob`: the frames go straight to Rust as
  // base64 anyway, and this keeps the loop synchronous and ordered.
  return canvas.toDataURL("image/png").split(",")[1] ?? "";
}
