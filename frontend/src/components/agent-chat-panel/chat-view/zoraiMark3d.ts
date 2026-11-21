import {
  ABSTRACT_GLYPHS,
  ZORAI_MARK_LETTERS,
  createEpisode,
  createPrng,
  type LetterEpisode,
  type Vec3,
  type ZoraiMarkLetter,
} from "./zoraiMarkGlyphs";
import {
  anamorphicLift,
  appendCageGeometry,
  clamp,
  project3D,
  rotateXYZ,
  type ProjectedArc,
  type ProjectedDot,
  type ProjectedFacet,
  type ProjectedLine,
} from "./zoraiMarkGeometry";

export { ZORAI_MARK_LETTERS, type ZoraiMarkLetter };
export type { ProjectedArc, ProjectedDot, ProjectedFacet, ProjectedLine };

export type ProjectedMarkFrame = {
  letter: ZoraiMarkLetter;
  episodeIndex: number;
  facets: ProjectedFacet[];
  backArcs: ProjectedArc[];
  backLines: ProjectedLine[];
  backDots: ProjectedDot[];
  frontLines: ProjectedLine[];
  frontArcs: ProjectedArc[];
  frontDots: ProjectedDot[];
};

type AmbientParticle = {
  amp: Vec3;
  freq: Vec3;
  phase: Vec3;
  r: number;
  tint: 0 | 1 | 2 | 3;
};

export type MarkTimeline = {
  sessionSeed: number;
  episodes: LetterEpisode[];
  startsMs: number[];
  ambient: AmbientParticle[];
};

// Continuous cubic inflection: slows smoothly near u=0 without ever stopping flat
function continuousInflect(u: number): number {
  const c = clamp(u, -1, 1);
  return 0.78 * c * c * c + 0.22 * c;
}

function createAmbientParticles(seed: number): AmbientParticle[] {
  const rng = createPrng(seed ^ 0x51f23c19);
  const irrationals = [1.0, 1.61803, 1.41421, 1.73205, 2.23606];
  // Minimal 4 floating 3D points on non-repeating irrational orbits (clean, never snowy)
  return Array.from({ length: 4 }, (_, idx) => {
    const base = 0.00058 + rng() * 0.00078;
    return {
      amp: [0.72 + rng() * 0.44, 0.72 + rng() * 0.44, 0.62 + rng() * 0.52],
      freq: [
        base * irrationals[idx % irrationals.length],
        base * irrationals[(idx + 2) % irrationals.length],
        base * irrationals[(idx + 3) % irrationals.length],
      ],
      phase: [rng() * Math.PI * 2, rng() * Math.PI * 2, rng() * Math.PI * 2],
      r: 0.82 + rng() * 0.52,
      tint: (idx % 4) as 0 | 1 | 2 | 3,
    };
  });
}

export function createMarkTimeline(sessionSeed: number): MarkTimeline {
  return {
    sessionSeed,
    episodes: [createEpisode(sessionSeed, 0)],
    startsMs: [0],
    ambient: createAmbientParticles(sessionSeed),
  };
}

function resolveEpisode(timeline: MarkTimeline, elapsedMs: number): { episode: LetterEpisode; localT: number } {
  const safeMs = Math.max(0, elapsedMs);
  while (true) {
    const lastIdx = timeline.episodes.length - 1;
    const endMs = timeline.startsMs[lastIdx] + timeline.episodes[lastIdx].durationMs;
    if (safeMs < endMs) break;
    timeline.episodes.push(createEpisode(timeline.sessionSeed, lastIdx + 1));
    timeline.startsMs.push(endMs);
  }
  let lo = 0;
  let hi = timeline.episodes.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (timeline.startsMs[mid] <= safeMs) lo = mid;
    else hi = mid - 1;
  }
  const episode = timeline.episodes[lo];
  const localT = clamp((safeMs - timeline.startsMs[lo]) / episode.durationMs, 0, 1);
  return { episode, localT };
}

function computeRotationAt(ep: LetterEpisode, t: number, staticPose: boolean): { rot: Vec3; alignment: number } {
  if (staticPose) {
    return { rot: [0.46, -0.58, -0.18], alignment: 0.55 };
  }
  const u = t * 2 - 1; // -1..+1, passing through 0 at t=0.5
  const s = continuousInflect(u);
  const src = s < 0 ? ep.entryRot : ep.exitRot;
  const mag = Math.abs(s);
  const alignment = clamp(1 - mag * 1.4, 0, 1);

  const rx = src[0] * mag * Math.sign(s || 1)
    + ep.obliqueBias[0] * alignment
    + Math.sin(t * Math.PI * 2 * ep.driftFreq[0] + ep.driftPhase[0]) * ep.driftAmp[0];
  const ry = src[1] * mag * Math.sign(s || 1)
    + ep.obliqueBias[1] * alignment
    + Math.cos(t * Math.PI * 2 * ep.driftFreq[1] + ep.driftPhase[1]) * ep.driftAmp[1];
  const rz = src[2] * mag * Math.sign(s || 1)
    + ep.obliqueBias[2] * alignment
    + Math.sin(t * Math.PI * 2 * ep.driftFreq[2] + ep.driftPhase[2]) * ep.driftAmp[2];

  return { rot: [rx, ry, rz], alignment };
}

export function sampleMarkFrame(timeline: MarkTimeline, elapsedMs: number, staticPose = false): ProjectedMarkFrame {
  const { episode: ep, localT } = staticPose
    ? { episode: timeline.episodes[0], localT: 0.5 }
    : resolveEpisode(timeline, elapsedMs);

  const glyph = ABSTRACT_GLYPHS[ep.letter];
  const pal = ep.palette;
  const timeSec = staticPose ? 0.45 : elapsedMs * 0.001;

  // Smooth cross-envelope at episode boundaries (never snaps or stalls)
  const epAlpha = staticPose
    ? 1
    : clamp(Math.min(localT * 5.2, (1 - localT) * 5.2), 0.08, 1);

  const facets: ProjectedFacet[] = [];
  const backArcs: ProjectedArc[] = [];
  const frontArcs: ProjectedArc[] = [];
  const backLines: ProjectedLine[] = [];
  const frontLines: ProjectedLine[] = [];
  const backDots: ProjectedDot[] = [];
  const frontDots: ProjectedDot[] = [];

  appendCageGeometry(ep, timeSec, epAlpha, facets, backArcs, frontArcs, backLines, frontLines);

  // Per-segment staggered anamorphic sweep so strokes glide past alignment asynchronously like rotating louvers
  const glyphAnchors: { x: number; y: number; z: number; color: string }[] = [];
  glyph.segments.forEach((seg, idx) => {
    const lagT = staticPose ? 0.5 : clamp(localT + ep.segmentLags[idx], 0, 1);
    const { rot, alignment } = computeRotationAt(ep, lagT, staticPose);
    const twist = staticPose ? 0.12 : ep.twistAmp * (1 - alignment * 0.65);

    const tx3 = (p: Vec3): Vec3 => {
      const tw = twist ? rotateXYZ(p, 0, p[1] * twist, p[2] * twist * 0.45) : p;
      return rotateXYZ(tw, rot[0], rot[1], rot[2]);
    };

    const [za, zb] = ep.segmentDepths[idx];
    const color = seg.role === "accent" ? pal.secondary : pal.primary;
    const trim = ep.trimFactor + (1 - alignment) * 0.16;
    const ax = seg.a[0] + (seg.b[0] - seg.a[0]) * trim;
    const ay = seg.a[1] + (seg.b[1] - seg.a[1]) * trim;
    const bx = seg.b[0] - (seg.b[0] - seg.a[0]) * trim;
    const by = seg.b[1] - (seg.b[1] - seg.a[1]) * trim;

    const p1 = project3D(tx3(anamorphicLift(ax, ay, za)));
    const p2 = project3D(tx3(anamorphicLift(bx, by, zb)));
    const depth = (p1.z + p2.z) * 0.5;
    const persp = (p1.persp + p2.persp) * 0.5;

    const line: ProjectedLine = {
      x1: p1.x,
      y1: p1.y,
      x2: p2.x,
      y2: p2.y,
      depth,
      width: (seg.role === "accent" ? 1.45 : 1.28) * clamp(persp, 0.72, 1.22),
      opacity: clamp(0.4 + (depth + 1) * 0.34, 0.26, 0.9) * epAlpha,
      color,
      dashed: ep.style === "shards" && idx % 2 === 1,
    };
    if (depth < 0) backLines.push(line);
    else frontLines.push(line);

    // Prism & wire styles add volumetric depth-echo shards so the letter blends into a full 3D structure
    if (ep.style === "prism" || ep.style === "wire") {
      const e1 = project3D(tx3(anamorphicLift(ax * 0.86, ay * 0.86, za - 0.44)));
      const e2 = project3D(tx3(anamorphicLift(bx * 0.86, by * 0.86, zb - 0.44)));
      const ed = (e1.z + e2.z) * 0.5;
      const echo: ProjectedLine = {
        x1: e1.x,
        y1: e1.y,
        x2: e2.x,
        y2: e2.y,
        depth: ed,
        width: 0.9,
        opacity: 0.42 * epAlpha,
        color: pal.tertiary,
        dashed: ep.style === "wire",
      };
      if (ed < 0) backLines.push(echo);
      else frontLines.push(echo);
    }
  });

  // Only 2 minimal focal dots per glyph
  const basePose = computeRotationAt(ep, localT, staticPose);
  const baseTwist = staticPose ? 0.12 : ep.twistAmp * (1 - basePose.alignment * 0.65);
  glyph.dots.forEach((d, idx) => {
    const z = ep.dotDepths[idx];
    const wobble = staticPose ? 0 : (1 - basePose.alignment) * 0.22;
    const raw = anamorphicLift(
      d.p[0] + Math.sin(timeSec * 2.3 + idx * 1.9) * wobble,
      d.p[1] + Math.cos(timeSec * 2.0 + idx * 1.4) * wobble,
      z,
    );
    const tw = baseTwist ? rotateXYZ(raw, 0, raw[1] * baseTwist, raw[2] * baseTwist * 0.45) : raw;
    const p = project3D(rotateXYZ(tw, basePose.rot[0], basePose.rot[1], basePose.rot[2]));
    const color = d.role === "accent" ? pal.secondary : pal.primary;
    const dot: ProjectedDot = {
      cx: p.x,
      cy: p.y,
      r: (d.role === "accent" ? 1.35 : 1.05) * clamp(p.persp, 0.74, 1.28),
      depth: p.z,
      opacity: clamp(0.45 + (p.z + 1) * 0.34, 0.3, 0.94) * epAlpha,
      color,
    };
    if (p.z < 0) backDots.push(dot);
    else frontDots.push(dot);
    glyphAnchors.push({ x: p.x, y: p.y, z: p.z, color });
  });

  // 4 floating ambient 3D points + max 2 whisper-fine dynamic proximity filaments
  const dustColors = [pal.dust, pal.primary, pal.secondary, pal.tertiary];
  const projectedDust: { x: number; y: number; z: number; color: string }[] = [];
  for (const particle of timeline.ambient) {
    const t = staticPose ? 1400 : elapsedMs;
    const p = project3D([
      Math.sin(t * particle.freq[0] + particle.phase[0]) * particle.amp[0],
      Math.cos(t * particle.freq[1] + particle.phase[1]) * particle.amp[1],
      Math.sin(t * particle.freq[2] + particle.phase[2]) * particle.amp[2],
    ]);
    const color = dustColors[particle.tint];
    const dot: ProjectedDot = {
      cx: p.x,
      cy: p.y,
      r: particle.r * clamp(p.persp, 0.7, 1.3),
      depth: p.z,
      opacity: clamp(0.32 + (p.z + 1) * 0.32, 0.22, 0.84),
      color,
    };
    if (p.z < 0) backDots.push(dot);
    else frontDots.push(dot);
    projectedDust.push({ x: p.x, y: p.y, z: p.z, color });
  }

  let filaments = 0;
  for (let i = 0; i < projectedDust.length && filaments < 2; i += 1) {
    const a = projectedDust[i];
    const anchor = glyphAnchors[i % Math.max(1, glyphAnchors.length)];
    if (anchor) {
      const dist = Math.hypot(a.x - anchor.x, a.y - anchor.y);
      if (dist < 10.5 && dist > 2.4) {
        const depth = (a.z + anchor.z) * 0.5;
        const line: ProjectedLine = {
          x1: a.x,
          y1: a.y,
          x2: anchor.x,
          y2: anchor.y,
          depth,
          width: 0.65,
          opacity: (1 - dist / 10.5) * 0.38 * epAlpha,
          color: anchor.color,
          dashed: false,
        };
        if (depth < 0) backLines.push(line);
        else frontLines.push(line);
        filaments += 1;
      }
    }
  }

  facets.sort((a, b) => a.depth - b.depth);
  backArcs.sort((a, b) => a.depth - b.depth);
  frontArcs.sort((a, b) => a.depth - b.depth);
  backLines.sort((a, b) => a.depth - b.depth);
  frontLines.sort((a, b) => a.depth - b.depth);
  backDots.sort((a, b) => a.depth - b.depth);
  frontDots.sort((a, b) => a.depth - b.depth);

  return {
    letter: ep.letter,
    episodeIndex: ep.index,
    facets,
    backArcs,
    backLines,
    backDots,
    frontLines,
    frontArcs,
    frontDots,
  };
}

export function staticMarkFrame(seed = 0x5a04a1): ProjectedMarkFrame {
  return sampleMarkFrame(createMarkTimeline(seed), 0, true);
}
