/**
 * Draws the pet: one sprite, one contact shadow.
 *
 * The shadow is not decoration. A character drawn with no contact shadow reads
 * as a sticker lying on top of the screen rather than something standing on
 * the desktop — it is the single cheapest thing that separates "feels good"
 * from "feels cheap", which is why it lives here rather than in the pack art.
 * Packs tune it (`shadow` in the manifest); the runtime draws it, so it stays
 * correct when the character moves or jumps.
 */

import { BlurFilter, Container, Graphics, Sprite, Texture } from "pixi.js";
import type { Manifest } from "../petpack";

export class PetRenderer {
  readonly view = new Container();

  private readonly textures = new Map<string, Texture>();
  private readonly sprite = new Sprite();
  private readonly shadow = new Graphics();
  private currentFrame = "";

  constructor(private readonly manifest: Manifest) {}

  /**
   * Decode every frame up front.
   *
   * Decoding lazily would stall the first time each new state is entered,
   * which is exactly when the pet is supposed to look responsive. A whole pack
   * at v0 canvas sizes is a few hundred kilobytes.
   *
   * Textures come from `HTMLImageElement` rather than `Assets.load` because
   * the frames arrive as `data:` URLs from Rust, and `decode()` gives a
   * definite point at which the bitmap is ready.
   */
  async load(frames: Record<string, string>): Promise<void> {
    await Promise.all(
      Object.entries(frames).map(async ([id, url]) => {
        const img = new Image();
        img.src = url;
        await img.decode();
        this.textures.set(id, Texture.from(img));
      }),
    );

    this.drawShadow();
    // Shadow first: it belongs under the character, always.
    this.view.addChild(this.shadow, this.sprite);

    const first = this.manifest.states.idle?.frames[0];
    if (first) this.setFrame(first);
  }

  /** Swap the displayed frame. Unknown ids are ignored rather than blanking
   *  the pet — a missing frame should look like a pause, not a disappearance. */
  setFrame(frameId: string): void {
    if (frameId === this.currentFrame) return;
    const tex = this.textures.get(frameId);
    if (!tex) return;
    this.sprite.texture = tex;
    this.currentFrame = frameId;
  }

  get frameCount(): number {
    return this.textures.size;
  }

  private drawShadow(): void {
    const cfg = this.manifest.shadow;
    if (!cfg?.enabled) return;

    const { baselineY, centroidX } = this.manifest.anchor;
    // Flattened ellipse sitting on the foot line, not centred on the canvas.
    const rx = (this.manifest.canvas.width * cfg.widthRatio) / 2;
    const ry = rx * 0.22;

    this.shadow
      .ellipse(centroidX, baselineY, rx, ry)
      .fill({ color: 0x000000, alpha: cfg.opacity });

    if (cfg.blur > 0) {
      this.shadow.filters = [new BlurFilter({ strength: cfg.blur })];
    }
  }
}
