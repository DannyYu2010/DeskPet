/**
 * Behaviour engine.
 *
 * The goal is that the character reads as having intent rather than shuffling
 * a deck. Random state switching is instantly recognisable as random; a slowly
 * rising "boredom" drive that gates certain transitions is not much more code
 * and reads completely differently.
 *
 * No platform branches here, or anywhere in the frontend. See ARCHITECTURE.md.
 */

import type { Manifest, StateName, Transition } from "../petpack";

export interface BrainEvent {
  kind: "poke" | "drag_start" | "drag_end" | "notify" | "cursor_near" | "action";
  state?: StateName;
}

export interface BrainOutput {
  state: StateName;
  /** Seconds since this state was entered; one-shot animations start at frame 0. */
  stateElapsedSeconds: number;
  /** 0-1, exposed so the renderer can slow blinks and droop posture. */
  boredom: number;
  /** True when nothing is animating and the render loop can be parked. */
  quiescent: boolean;
}

const MIN_STATE_SECONDS = 1.2; // stops visible flip-flopping

export class Brain {
  private boredom = 0;
  private state: StateName;
  private timeInState = 0;
  private forcedUntil = 0;
  private now = 0;
  private sleepAfterSeconds: number;
  private wakeAfterLieDown = false;

  constructor(private manifest: Manifest, sleepAfterSeconds = 90) {
    this.sleepAfterSeconds = Math.max(1, sleepAfterSeconds);
    // Packs may provide a one-shot appearance animation. Older packs start
    // exactly as before because `idle` remains the compatibility fallback.
    this.state = manifest.states.launch ? "launch" : "idle";
  }

  setSleepAfterSeconds(seconds: number): void {
    this.sleepAfterSeconds = Math.max(1, seconds);
  }

  handle(event: BrainEvent): void {
    // Any interaction resets the drive. This is what makes the pet perk up
    // when you come back to the machine.
    this.boredom = 0;

    switch (event.kind) {
      case "poke":
        // A sleeping pet should first get up; jumping straight from the lying
        // silhouette into a standing poke reads as a broken frame.
        if (this.state === "sleep" && this.manifest.states.wake) {
          this.enter("wake", 0);
        } else if (this.state === "lie_down") {
          this.wakeAfterLieDown = true;
        } else {
          this.enter("poke", 0.8);
        }
        break;
      case "action":
        if (event.state) this.enter(event.state, 0);
        break;
      case "drag_start":
        this.enter("drag", 0);
        break;
      case "drag_end":
        this.enter("idle", 0.3);
        break;
      case "notify":
        this.enter("notify", 1.5);
        break;
      case "cursor_near":
        // Sleeping is the one state where approach has intent: wake at once.
        // A dedicated one-shot `wake` animation may bridge into idle; packs
        // without it still respond immediately instead of ignoring the user.
        if (this.state === "sleep") {
          this.enter(this.manifest.states.wake ? "wake" : "idle", 0);
        } else if (this.state === "lie_down") {
          // Finish the physically coherent descent first, then immediately
          // play the matching wake transition from its exact sleep endpoint.
          this.wakeAfterLieDown = true;
        }
        break;
    }
  }

  tick(dt: number): BrainOutput {
    this.now += dt;
    this.timeInState += dt;
    this.boredom = Math.min(1, this.boredom + dt / this.sleepAfterSeconds);

    const locked = this.now < this.forcedUntil;

    // A non-looping state is a one-shot: once it has played through, it falls
    // into `next` (see PETPACK_SPEC.md). Without this, `poke` and `notify` are
    // traps — neither has a `transitions` entry, so nothing else can ever move
    // the pet out of them and one click freezes the character for good.
    const def = this.manifest.states[this.state];
    if (!locked && def && def.loop === false && def.next) {
      const playedFor = def.frames.length / def.fps;
      if (this.timeInState >= playedFor) {
        const completed = this.state;
        this.enter(def.next, 0);
        if (completed === "lie_down" && this.wakeAfterLieDown && this.state === "sleep") {
          this.wakeAfterLieDown = false;
          this.enter(this.manifest.states.wake ? "wake" : "idle", 0);
        }
      }
    }

    // Sleeping is a timer promise to the user, so it must not depend on the
    // random transition sampler. Once inactivity reaches the configured
    // duration, enter sleep on the next tick.
    if (!locked && this.state === "idle" && this.boredom >= 1 && this.manifest.states.sleep) {
      this.enter(this.manifest.states.lie_down ? "lie_down" : "sleep", 0);
    } else if (!locked && this.timeInState >= MIN_STATE_SECONDS) {
      const next = this.pick(this.state);
      if (next && next !== this.state) this.enter(next, 0);
    }

    return {
      state: this.state,
      stateElapsedSeconds: this.timeInState,
      boredom: this.boredom,
      quiescent: this.isQuiescent(),
    };
  }

  /** Weighted choice among transitions whose boredom gates are satisfied. */
  private pick(from: StateName): StateName | null {
    const options = (this.manifest.transitions?.[from] ?? []).filter(
      (t: Transition) =>
        (t.minBoredom === undefined || this.boredom >= t.minBoredom) &&
        (t.maxBoredom === undefined || this.boredom <= t.maxBoredom),
    );
    if (options.length === 0) return null;

    // Rate-limit: even when eligible, only re-roll occasionally, so the pet
    // lingers instead of twitching between equally-valid states.
    if (Math.random() > 0.02) return null;

    const total = options.reduce((s, t) => s + (t.weight ?? 1), 0);
    let r = Math.random() * total;
    for (const t of options) {
      r -= t.weight ?? 1;
      if (r <= 0) return t.to;
    }
    return options[options.length - 1]?.to ?? null;
  }

  private enter(state: StateName, lockSeconds: number): void {
    if (!this.manifest.states[state]) state = "idle"; // packs may omit states
    this.state = state;
    this.timeInState = 0;
    this.forcedUntil = this.now + lockSeconds;
  }

  /**
   * True only when nothing is moving *and* nothing is pending. Lets the
   * renderer stop the rAF loop entirely, which is how idle CPU reaches ~0%
   * rather than "low".
   */
  private isQuiescent(): boolean {
    const def = this.manifest.states[this.state];
    if (!def) return false;

    // A one-shot state still owes us the fall-through into `next`. Parking the
    // render loop here would make a temporary reaction permanent: the loop
    // stops, `tick` is never called again, and the transition never happens.
    // "Nothing is moving right now" is not the same as "nothing is pending".
    if (def.loop === false) return false;

    return def.frames.length <= 1;
  }
}
