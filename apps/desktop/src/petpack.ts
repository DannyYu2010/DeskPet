/** Types mirroring docs/PETPACK_SPEC.md. Keep them in sync. */

export type StateName = string;

export interface StateDef {
  frames: string[];
  fps: number;
  loop: boolean;
  /** State to fall into when a non-looping animation finishes. */
  next?: StateName;
}

export interface Transition {
  to: StateName;
  weight?: number;
  minBoredom?: number;
  maxBoredom?: number;
}

export interface Manifest {
  format: number;
  id: string;
  name: string;
  author: string;
  license: string;
  canvas: { width: number; height: number };
  /** Frames align on foot baseline + horizontal centroid, NOT canvas centre. */
  anchor: { baselineY: number; centroidX: number };
  states: Record<StateName, StateDef>;
  transitions?: Record<StateName, Transition[]>;
  gaze?: {
    enabled: boolean;
    eyes: { x: number; y: number; radius: number }[];
    maxOffset: number;
  };
  shadow?: {
    enabled: boolean;
    opacity: number;
    widthRatio: number;
    blur: number;
  };
  sourceMap?: Record<string, StateName>;
}

/** What `load_active_pack` returns. Mirrors `core::pack::LoadedPack`. */
export interface LoadedPack {
  manifest: Manifest;
  /** Frame id -> `data:` URL. Frames are inlined by Rust; see core/pack.rs. */
  frames: Record<string, string>;
  sourceDir: string;
}

export interface Notification {
  title: string;
  body: string;
  source: string;
  action_url?: string;
  priority: number;
}
