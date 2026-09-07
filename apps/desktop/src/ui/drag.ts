/**
 * Dragging the pet.
 *
 * ## Why not `startDragging()`
 *
 * Tauri's `startDragging` calls AppKit's `performWindowDragWithEvent:`, which
 * requires `NSApp.currentEvent` to still be the mouse-down that started the
 * gesture. Our window is a non-activating `NSPanel`: clicking it deliberately
 * does not activate the app or make the panel key, and by the time an async
 * IPC round trip reaches the native side the current event has moved on. The
 * call then does nothing at all — no error, no movement, which is exactly the
 * kind of silent failure that costs an afternoon.
 *
 * So the drag is ours. Position is computed from the pointer's *screen*
 * coordinates against the window origin sampled at press time, rather than by
 * accumulating deltas: accumulating drifts, and client coordinates are useless
 * here because they barely change while the window chases the cursor.
 *
 * A press that never travels more than `CLICK_SLOP_PX` is reported as a click
 * instead. Without that, every poke would also nudge the pet a pixel.
 */

import { PhysicalPosition } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";

const CLICK_SLOP_PX = 4;

export interface DragHandlers {
  onDragStart(): void;
  onDragEnd(): void;
  onClick(): void;
  onError(message: string): void;
}

export function installDrag(h: DragHandlers): void {
  const win = getCurrentWindow();

  let pressed = false;
  let dragging = false;
  let haveOrigin = false;
  let inFlight = false;

  let originX = 0;
  let originY = 0;
  let pressScreenX = 0;
  let pressScreenY = 0;
  let travelled = 0;

  window.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    pressed = true;
    dragging = false;
    haveOrigin = false;
    travelled = 0;
    pressScreenX = e.screenX;
    pressScreenY = e.screenY;

    win
      .outerPosition()
      .then((p) => {
        originX = p.x;
        originY = p.y;
        haveOrigin = true;
      })
      .catch((err) => h.onError(`outerPosition failed: ${String(err)}`));
  });

  window.addEventListener("pointermove", (e) => {
    if (!pressed) return;

    const dx = e.screenX - pressScreenX;
    const dy = e.screenY - pressScreenY;
    travelled = Math.max(travelled, Math.hypot(dx, dy));

    if (travelled < CLICK_SLOP_PX || !haveOrigin) return;

    if (!dragging) {
      dragging = true;
      h.onDragStart();
    }

    // One move in flight at a time. Queueing every pointermove would flood the
    // IPC channel and make the window lag behind the cursor under load.
    if (inFlight) return;
    inFlight = true;

    const dpr = window.devicePixelRatio || 1;
    win
      .setPosition(
        new PhysicalPosition(
          Math.round(originX + dx * dpr),
          Math.round(originY + dy * dpr),
        ),
      )
      .catch((err) => h.onError(`setPosition failed: ${String(err)}`))
      .finally(() => {
        inFlight = false;
      });
  });

  const release = () => {
    if (!pressed) return;
    pressed = false;
    if (dragging) {
      dragging = false;
      h.onDragEnd();
    } else {
      h.onClick();
    }
  };

  window.addEventListener("pointerup", release);
  window.addEventListener("pointercancel", release);
  // A press that ends outside the window never delivers pointerup here.
  window.addEventListener("blur", release);
}
