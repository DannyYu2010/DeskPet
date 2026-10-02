/**
 * Draws the pet: a perspective-projected sprite and a contact shadow.
 *
 * ## Why this is not a plain Sprite
 *
 * A cutout drawn flat reads as a sticker no matter how good the cutout is,
 * because nothing about it responds to where you are. Three cheap things fix
 * most of that, and none of them need the image to be decomposed into parts:
 *
 * 1. **Perspective tilt.** The quad turns a few degrees toward the cursor. The
 *    eye reads a shape that turns as a shape with a back.
 * 2. **A shadow that is coupled to the pose.** The contact shadow widens and
 *    fades as the character compresses and tightens as it extends. A shadow
 *    that never changes is a shadow the eye stops believing.
 * 3. **Secondary motion.** The tilt is sprung rather than snapped, so it
 *    overshoots slightly and settles. This is what reads as mass.
 *
 * What this cannot do is invent geometry. Turning the quad shows the same
 * pixels from a different angle; it does not reveal the other side of the
 * animal. Head turns, tail wags and limbs need the cutout cut into parts,
 * which is a different and much larger job.
 */

import { BlurFilter, Container, Graphics, PerspectiveMesh, Texture } from "pixi.js";
import type { Manifest } from "../petpack";

/** Maximum yaw/pitch, as a fraction of the sprite's size. Small on purpose:
 *  past a few percent the projection stops reading as a turn and starts
 *  reading as a warp. */
const MAX_YAW = 0.085;
const MAX_PITCH = 0.055;
/** How far the cursor has to be for the tilt to reach full deflection. */
const REACH = 260;
/** Spring constants for the tilt. Underdamped on purpose — the small
 *  overshoot is the part that reads as weight. */
const STIFFNESS = 90;
const DAMPING = 11;

export class PetRenderer {
  readonly view = new Container();

  private readonly textures = new Map<string, Texture>();
  private readonly shadow = new Graphics();
  private mesh: PerspectiveMesh | null = null;
  private currentFrame = "";

  /** Current and target tilt, each in [-1, 1]. */
  private tilt = { x: 0, y: 0 };
  private velocity = { x: 0, y: 0 };
  private target = { x: 0, y: 0 };

  /** 0 at rest, 1 fully compressed. Drives the shadow. */
  private breath = 0;

  constructor(private readonly manifest: Manifest) {}

  async load(frames: Record<string, string>): Promise<void> {
    await Promise.all(
      Object.entries(frames).map(async ([id, url]) => {
        const img = new Image();
        img.src = url;
        await img.decode();
        this.textures.set(id, Texture.from(img));
      }),
    );

    const first = this.manifest.states.idle?.frames[0] ?? "";
    const texture = this.textures.get(first) ?? Texture.EMPTY;
    const { width: w, height: h } = this.manifest.canvas;

    // More vertices than strictly needed for a flat quad: the projection is
    // interpolated per vertex, and a coarse grid makes straight edges bend.
    this.mesh = new PerspectiveMesh({
      texture,
      verticesX: 12,
      verticesY: 12,
      x0: 0, y0: 0,
      x1: w, y1: 0,
      x2: w, y2: h,
      x3: 0, y3: h,
    });
    this.currentFrame = first;

    this.drawShadow();
    // Shadow first: it belongs under the character, always.
    this.view.addChild(this.shadow, this.mesh);
  }

  /** Unknown ids are ignored rather than blanking the pet — a missing frame
   *  should look like a pause, not a disappearance. */
  setFrame(frameId: string): void {
    if (frameId === this.currentFrame || !this.mesh) return;
    const tex = this.textures.get(frameId);
    if (!tex) return;
    this.mesh.texture = tex;
    this.currentFrame = frameId;

    // Breathing is baked into the frames, so the shadow reads the pose from
    // the frame index rather than from a physics model it does not have.
    const idle = this.manifest.states.idle?.frames ?? [];
    const i = idle.indexOf(frameId);
    if (i >= 0 && idle.length > 1) {
      this.breath = Math.sin((i / idle.length) * Math.PI * 2) * 0.5 + 0.5;
    }
  }

  /**
   * Point the tilt at a cursor position in window-local pixels, or `null` to
   * return to rest.
   */
  lookAt(x: number | null, y: number | null): void {
    if (x === null || y === null) {
      this.target.x = 0;
      this.target.y = 0;
      return;
    }
    const { width: w, height: h } = this.manifest.canvas;
    const { baselineY, centroidX } = this.manifest.anchor;
    this.target.x = clamp((x - centroidX) / REACH, -1, 1);
    // Measured against the body, not the canvas: a cursor level with the
    // character's middle should read as level.
    this.target.y = clamp((y - baselineY * 0.6) / REACH, -1, 1);
    void w;
    void h;
  }

  /** Advance the spring and redraw. Call once per rendered frame. */
  update(dt: number): void {
    // Clamp dt: a backgrounded window can hand us a huge step, and an
    // explicit spring integrated over 2 seconds explodes.
    const step = Math.min(dt, 1 / 30);

    for (const axis of ["x", "y"] as const) {
      const displacement = this.target[axis] - this.tilt[axis];
      const accel = displacement * STIFFNESS - this.velocity[axis] * DAMPING;
      this.velocity[axis] += accel * step;
      this.tilt[axis] += this.velocity[axis] * step;
    }

    this.applyTilt();
    this.updateShadow();
  }

  /** True while the tilt is still settling, so the render loop knows not to
   *  park mid-motion. */
  get moving(): boolean {
    return (
      Math.abs(this.velocity.x) > 0.01 ||
      Math.abs(this.velocity.y) > 0.01 ||
      Math.abs(this.target.x - this.tilt.x) > 0.005 ||
      Math.abs(this.target.y - this.tilt.y) > 0.005
    );
  }

  private applyTilt(): void {
    if (!this.mesh) return;
    const { width: w, height: h } = this.manifest.canvas;
    const { baselineY } = this.manifest.anchor;

    const yaw = this.tilt.x * MAX_YAW;
    const pitch = this.tilt.y * MAX_PITCH;

    // Yaw: the side the cursor is on comes forward, so it grows; the far side
    // shrinks. Pitch lifts or drops the top edge.
    const left = 1 - yaw;
    const right = 1 + yaw;

    // The feet stay on the floor. Scaling about the canvas centre would slide
    // the character off its own contact shadow every time it turned.
    const topLeft = baselineY - baselineY * left * (1 - pitch);
    const topRight = baselineY - baselineY * right * (1 - pitch);

    const inset = w * yaw * 0.5;

    this.mesh.setCorners(
      -inset, topLeft,
      w - inset, topRight,
      w + inset * 0.2, h,
      inset * 0.2, h,
    );
  }

  private drawShadow(): void {
    const cfg = this.manifest.shadow;
    if (!cfg?.enabled) return;
    this.shadow.filters = cfg.blur > 0 ? [new BlurFilter({ strength: cfg.blur })] : [];
    this.updateShadow();
  }

  private updateShadow(): void {
    const cfg = this.manifest.shadow;
    if (!cfg?.enabled) return;

    const { baselineY, centroidX } = this.manifest.anchor;
    const base = (this.manifest.canvas.width * cfg.widthRatio) / 2;

    // Compressed: wider and softer, as the body spreads onto the floor.
    // Extended: tighter and darker. Turning shifts the shadow slightly the
    // other way, which sells the turn more than the tilt itself does.
    const spread = 1 + this.breath * 0.14;
    const rx = base * spread;
    const ry = rx * 0.22;
    const alpha = cfg.opacity * (1 - this.breath * 0.18);

    this.shadow.clear();
    this.shadow
      .ellipse(centroidX - this.tilt.x * 6, baselineY, rx, ry)
      .fill({ color: 0x000000, alpha });
  }
}

function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}
