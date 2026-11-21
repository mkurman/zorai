export const ZORAI_MARK_LETTERS = ["z", "o", "r", "a", "i"] as const;
export type ZoraiMarkLetter = (typeof ZORAI_MARK_LETTERS)[number];

export type Vec2 = readonly [number, number];
export type Vec3 = readonly [number, number, number];

export type EpisodeStyle = "minimal" | "prism" | "shards" | "wire";
export type CageKind = "none" | "octahedron" | "tetrahedron" | "rings" | "planes" | "prism";

export type ChromaticPalette = {
  primary: string;
  secondary: string;
  tertiary: string;
  facet: string;
  dust: string;
};

export type AbstractSegment2D = {
  a: Vec2;
  b: Vec2;
  role: "core" | "accent";
};

export type AbstractGlyph2D = {
  segments: readonly AbstractSegment2D[];
  dots: readonly { p: Vec2; role: "core" | "accent" }[];
};

export type LetterEpisode = {
  index: number;
  letter: ZoraiMarkLetter;
  durationMs: number;
  palette: ChromaticPalette;
  style: EpisodeStyle;
  cageKind: CageKind;
  // Deep randomized 3D anamorphic scatter per segment & dot
  segmentDepths: readonly (readonly [number, number])[];
  segmentLags: readonly number[];
  dotDepths: readonly number[];
  // Continuous freaky 3D rotation trajectories (never pauses flat)
  entryRot: Vec3;
  exitRot: Vec3;
  obliqueBias: Vec3;
  driftAmp: Vec3;
  driftFreq: Vec3;
  driftPhase: Vec3;
  twistAmp: number;
  cageTilt: Vec3;
  cageSpin: Vec3;
  cageScale: number;
  trimFactor: number;
};

// 100% theme-native color harmonies derived live from Zorai CSS theme variables
const T_ACCENT = "var(--zorai-accent, var(--accent))";
const T_ACCENT_HI = "var(--accent-hover, var(--accent))";
const T_ACCENT_SOFT = "color-mix(in srgb, var(--zorai-accent, var(--accent)) 56%, var(--text-primary))";
const T_INK = "color-mix(in srgb, var(--text-primary) 82%, var(--text-muted))";
const T_MUTED = "var(--text-muted)";
const T_SECONDARY = "color-mix(in srgb, var(--accent-secondary, var(--info, var(--accent))) 44%, var(--text-primary))";
const T_COOL = "color-mix(in srgb, var(--info, var(--accent)) 40%, var(--text-muted))";
const T_ORGANIC = "color-mix(in srgb, var(--success, var(--accent)) 36%, var(--text-muted))";

const PALETTES: readonly ChromaticPalette[] = [
  { primary: T_ACCENT, secondary: T_ACCENT_SOFT, tertiary: T_MUTED, facet: T_ACCENT, dust: T_INK },
  { primary: T_INK, secondary: T_ACCENT_HI, tertiary: T_SECONDARY, facet: T_ACCENT_SOFT, dust: T_ACCENT },
  { primary: T_SECONDARY, secondary: T_ACCENT, tertiary: T_COOL, facet: T_SECONDARY, dust: T_INK },
  { primary: T_ACCENT_SOFT, secondary: T_ACCENT, tertiary: T_ORGANIC, facet: T_ACCENT, dust: T_MUTED },
];

const STYLES: readonly EpisodeStyle[] = ["minimal", "prism", "shards", "wire"];
const CAGE_SHAPES: readonly Exclude<CageKind, "none">[] = [
  "octahedron",
  "tetrahedron",
  "rings",
  "planes",
  "prism",
];

// Minimalist deconstructed glyphs: open gaps, asymmetric fragments, only 2 focal dots
export const ABSTRACT_GLYPHS: Record<ZoraiMarkLetter, AbstractGlyph2D> = {
  z: {
    segments: [
      { a: [-0.66, -0.66], b: [0.36, -0.66], role: "core" },
      { a: [0.54, -0.44], b: [-0.48, 0.46], role: "accent" },
      { a: [-0.32, 0.66], b: [0.68, 0.66], role: "core" },
    ],
    dots: [
      { p: [0.66, -0.66], role: "accent" },
      { p: [-0.66, 0.66], role: "core" },
    ],
  },
  o: {
    segments: [
      { a: [-0.66, -0.14], b: [-0.38, -0.64], role: "core" },
      { a: [-0.26, -0.7], b: [0.32, -0.64], role: "core" },
      { a: [0.66, -0.18], b: [0.46, 0.52], role: "accent" },
      { a: [0.24, 0.7], b: [-0.42, 0.56], role: "accent" },
    ],
    dots: [
      { p: [0.56, -0.46], role: "accent" },
      { p: [-0.58, 0.34], role: "core" },
    ],
  },
  r: {
    segments: [
      { a: [-0.54, 0.72], b: [-0.54, -0.48], role: "core" },
      { a: [-0.28, -0.68], b: [0.38, -0.62], role: "accent" },
      { a: [0.48, -0.38], b: [0.36, 0.02], role: "accent" },
      { a: [0.06, 0.22], b: [0.62, 0.72], role: "core" },
    ],
    dots: [
      { p: [-0.54, -0.7], role: "core" },
      { p: [0.52, -0.56], role: "accent" },
    ],
  },
  a: {
    segments: [
      { a: [-0.68, 0.72], b: [-0.18, -0.42], role: "core" },
      { a: [0.18, -0.42], b: [0.68, 0.72], role: "accent" },
      { a: [-0.28, 0.26], b: [0.28, 0.26], role: "accent" },
    ],
    dots: [
      { p: [0.0, -0.76], role: "accent" },
      { p: [0.68, 0.72], role: "core" },
    ],
  },
  i: {
    segments: [
      { a: [0.0, 0.66], b: [0.0, -0.14], role: "core" },
      { a: [-0.24, 0.72], b: [0.24, 0.72], role: "accent" },
    ],
    dots: [
      { p: [0.0, -0.66], role: "accent" },
      { p: [0.0, -0.24], role: "core" },
    ],
  },
};

export function createPrng(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function randRange(rng: () => number, min: number, max: number): number {
  return min + rng() * (max - min);
}

function randSign(rng: () => number): number {
  return rng() < 0.5 ? -1 : 1;
}

export function createEpisode(sessionSeed: number, episodeIndex: number): LetterEpisode {
  const mixedSeed = Math.imul(sessionSeed ^ 0x9e3779b9, 0x85ebca6b) ^ Math.imul(episodeIndex + 1, 0xc2b2ae35);
  const rng = createPrng(mixedSeed);
  const letter = ZORAI_MARK_LETTERS[episodeIndex % ZORAI_MARK_LETTERS.length];
  const glyph = ABSTRACT_GLYPHS[letter];

  const palette = PALETTES[Math.floor(rng() * PALETTES.length)];
  const style = STYLES[Math.floor(rng() * STYLES.length)];
  // 3D geometric figures appear randomly (~48% of episodes), leaving clean open space otherwise
  const cageKind: CageKind = rng() < 0.48
    ? CAGE_SHAPES[Math.floor(rng() * CAGE_SHAPES.length)]
    : "none";

  const depthSpread = style === "prism" ? 0.85 : 1.1;
  const segmentDepths = glyph.segments.map(() => [
    randRange(rng, -depthSpread, depthSpread),
    randRange(rng, -depthSpread, depthSpread),
  ] as const);
  const segmentLags = glyph.segments.map(() => randRange(rng, -0.075, 0.075));
  const dotDepths = glyph.dots.map(() => randRange(rng, -depthSpread, depthSpread));

  const entryRot: Vec3 = [
    randSign(rng) * randRange(rng, 1.8, 3.3),
    randSign(rng) * randRange(rng, 2.5, 4.4),
    randSign(rng) * randRange(rng, 1.1, 2.6),
  ];
  const exitRot: Vec3 = [
    randSign(rng) * randRange(rng, 1.7, 3.2),
    randSign(rng) * randRange(rng, 2.6, 4.5),
    randSign(rng) * randRange(rng, 1.1, 2.7),
  ];
  const obliqueBias: Vec3 = [
    randSign(rng) * randRange(rng, 0.12, 0.28),
    randSign(rng) * randRange(rng, 0.14, 0.34),
    randSign(rng) * randRange(rng, 0.06, 0.2),
  ];

  const driftAmp: Vec3 = [
    randRange(rng, 0.12, 0.28),
    randRange(rng, 0.15, 0.32),
    randRange(rng, 0.08, 0.2),
  ];
  const driftFreq: Vec3 = [
    randRange(rng, 1.2, 2.5),
    randRange(rng, 1.1, 2.3),
    randRange(rng, 1.0, 2.1),
  ];
  const driftPhase: Vec3 = [
    randRange(rng, 0, Math.PI * 2),
    randRange(rng, 0, Math.PI * 2),
    randRange(rng, 0, Math.PI * 2),
  ];

  return {
    index: episodeIndex,
    letter,
    durationMs: Math.round(randRange(rng, 1650, 2150)),
    palette,
    style,
    cageKind,
    segmentDepths,
    segmentLags,
    dotDepths,
    entryRot,
    exitRot,
    obliqueBias,
    driftAmp,
    driftFreq,
    driftPhase,
    twistAmp: randSign(rng) * randRange(rng, 0.34, 0.68),
    cageTilt: [
      randRange(rng, 0, Math.PI),
      randRange(rng, 0, Math.PI),
      randRange(rng, 0, Math.PI),
    ],
    cageSpin: [
      randSign(rng) * randRange(rng, 1.7, 3.6),
      randSign(rng) * randRange(rng, 2.0, 4.2),
      randSign(rng) * randRange(rng, 1.2, 3.0),
    ],
    cageScale: randRange(rng, 0.92, 1.12),
    trimFactor: randRange(rng, 0.1, 0.19),
  };
}
