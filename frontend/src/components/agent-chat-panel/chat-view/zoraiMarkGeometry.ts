import type { LetterEpisode, Vec3 } from "./zoraiMarkGlyphs";

export const MARK_CENTER = 18;
export const MARK_SCALE = 10.6;
export const MARK_FOCAL = 3.6;

export type ProjectedFacet = {
  points: string;
  depth: number;
  fill: string;
  stroke: string;
  fillOpacity: number;
  strokeOpacity: number;
};

export type ProjectedLine = {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  depth: number;
  width: number;
  opacity: number;
  color: string;
  dashed: boolean;
};

export type ProjectedDot = {
  cx: number;
  cy: number;
  r: number;
  depth: number;
  opacity: number;
  color: string;
};

export type ProjectedArc = {
  d: string;
  depth: number;
  width: number;
  opacity: number;
  color: string;
  dashed: boolean;
};

export function clamp(v: number, lo: number, hi: number): number {
  return v < lo ? lo : v > hi ? hi : v;
}

export function rotateXYZ(p: Vec3, rx: number, ry: number, rz: number): Vec3 {
  let [x, y, z] = p;
  if (rx) {
    const cx = Math.cos(rx);
    const sx = Math.sin(rx);
    const ny = y * cx - z * sx;
    const nz = y * sx + z * cx;
    y = ny;
    z = nz;
  }
  if (ry) {
    const cy = Math.cos(ry);
    const sy = Math.sin(ry);
    const nx = x * cy + z * sy;
    const nz = -x * sy + z * cy;
    x = nx;
    z = nz;
  }
  if (rz) {
    const cz = Math.cos(rz);
    const sz = Math.sin(rz);
    const nx = x * cz - y * sz;
    const ny = x * sz + y * cz;
    x = nx;
    y = ny;
  }
  return [x, y, z];
}

// Inverse perspective lift: at rot=(0,0,0) projects onto (u,v), off-axis explodes across 3D volume
export function anamorphicLift(u: number, v: number, z: number): Vec3 {
  const inv = (MARK_FOCAL - z) / MARK_FOCAL;
  return [u * inv, v * inv, z];
}

export function project3D(p: Vec3, scaleMul = 1): { x: number; y: number; z: number; persp: number } {
  const z = clamp(p[2], -2.3, 2.3);
  const persp = MARK_FOCAL / (MARK_FOCAL - z);
  return {
    x: MARK_CENTER + p[0] * MARK_SCALE * scaleMul * persp,
    y: MARK_CENTER + p[1] * MARK_SCALE * scaleMul * persp,
    z,
    persp,
  };
}

export function buildRingArcs(
  rx: number,
  ry: number,
  rot: Vec3,
  color: string,
  width: number,
  dashed: boolean,
  alpha: number,
): { back: ProjectedArc | null; front: ProjectedArc | null } {
  const steps = 26;
  const pts: { x: number; y: number; z: number }[] = [];
  for (let i = 0; i <= steps; i += 1) {
    const theta = (i / steps) * Math.PI * 2;
    const p = rotateXYZ([Math.cos(theta) * rx, Math.sin(theta) * ry, 0], rot[0], rot[1], rot[2]);
    pts.push(project3D(p));
  }
  let backD = "";
  let frontD = "";
  let backOpen = false;
  let frontOpen = false;
  let backZ = 0;
  let backN = 0;
  let frontZ = 0;
  let frontN = 0;
  for (let i = 0; i < steps; i += 1) {
    const a = pts[i];
    const b = pts[i + 1];
    const mz = (a.z + b.z) * 0.5;
    if (mz < 0) {
      if (!backOpen) {
        backD += `M${a.x.toFixed(1)} ${a.y.toFixed(1)}`;
        backOpen = true;
      }
      backD += `L${b.x.toFixed(1)} ${b.y.toFixed(1)}`;
      frontOpen = false;
      backZ += mz;
      backN += 1;
    } else {
      if (!frontOpen) {
        frontD += `M${a.x.toFixed(1)} ${a.y.toFixed(1)}`;
        frontOpen = true;
      }
      frontD += `L${b.x.toFixed(1)} ${b.y.toFixed(1)}`;
      backOpen = false;
      frontZ += mz;
      frontN += 1;
    }
  }
  return {
    back: backD ? { d: backD, depth: backZ / Math.max(1, backN), width: width * 0.8, opacity: 0.4 * alpha, color, dashed } : null,
    front: frontD ? { d: frontD, depth: frontZ / Math.max(1, frontN), width, opacity: 0.82 * alpha, color, dashed } : null,
  };
}

export function appendCageGeometry(
  ep: LetterEpisode,
  timeSec: number,
  alpha: number,
  facets: ProjectedFacet[],
  backArcs: ProjectedArc[],
  frontArcs: ProjectedArc[],
  backLines: ProjectedLine[],
  frontLines: ProjectedLine[],
) {
  if (ep.cageKind === "none") return;

  const rot: Vec3 = [
    ep.cageTilt[0] + ep.cageSpin[0] * timeSec,
    ep.cageTilt[1] + ep.cageSpin[1] * timeSec,
    ep.cageTilt[2] + ep.cageSpin[2] * timeSec,
  ];
  const s = ep.cageScale;
  const pal = ep.palette;

  const pushPoly = (verts: Vec3[], fill: string, stroke: string, fillAlpha: number, strokeAlpha: number) => {
    const projected = verts.map((v) => project3D(rotateXYZ([v[0] * s, v[1] * s, v[2] * s], rot[0], rot[1], rot[2])));
    const depth = projected.reduce((acc, p) => acc + p.z, 0) / projected.length;
    facets.push({
      points: projected.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(" "),
      depth,
      fill,
      stroke,
      fillOpacity: fillAlpha * alpha,
      strokeOpacity: strokeAlpha * alpha,
    });
  };

  const pushEdge = (a: Vec3, b: Vec3, color: string, dashed = false) => {
    const pa = project3D(rotateXYZ([a[0] * s, a[1] * s, a[2] * s], rot[0], rot[1], rot[2]));
    const pb = project3D(rotateXYZ([b[0] * s, b[1] * s, b[2] * s], rot[0], rot[1], rot[2]));
    const depth = (pa.z + pb.z) * 0.5;
    const line: ProjectedLine = {
      x1: pa.x,
      y1: pa.y,
      x2: pb.x,
      y2: pb.y,
      depth,
      width: 0.95,
      opacity: clamp(0.28 + (depth + 1) * 0.18, 0.18, 0.56) * alpha,
      color,
      dashed,
    };
    if (depth < 0) backLines.push(line);
    else frontLines.push(line);
  };

  if (ep.cageKind === "rings") {
    const r1 = buildRingArcs(1.04 * s, 0.58 * s, rot, pal.tertiary, 1.1, true, alpha);
    const r2 = buildRingArcs(0.94 * s, 0.48 * s, [-rot[1] * 0.9, rot[0] * 1.15, -rot[2]], pal.secondary, 0.95, false, alpha * 0.85);
    if (r1.back) backArcs.push(r1.back);
    if (r1.front) frontArcs.push(r1.front);
    if (r2.back) backArcs.push(r2.back);
    if (r2.front) frontArcs.push(r2.front);
  } else if (ep.cageKind === "octahedron") {
    const v: Vec3[] = [[0, -1, 0], [0, 1, 0], [1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];
    pushPoly([v[0], v[2], v[4]], pal.facet, pal.tertiary, 0.18, 0.5);
    pushPoly([v[1], v[3], v[5]], pal.primary, pal.primary, 0.13, 0.42);
    const pairs: [number, number][] = [[0, 2], [0, 3], [0, 4], [0, 5], [1, 2], [1, 3], [2, 4], [3, 5]];
    for (const [i, j] of pairs) pushEdge(v[i], v[j], pal.tertiary, i === 1);
  } else if (ep.cageKind === "tetrahedron") {
    const v: Vec3[] = [[0.86, 0.86, 0.86], [0.86, -0.86, -0.86], [-0.86, 0.86, -0.86], [-0.86, -0.86, 0.86]];
    pushPoly([v[0], v[1], v[2]], pal.facet, pal.secondary, 0.18, 0.52);
    pushPoly([v[0], v[2], v[3]], pal.tertiary, pal.primary, 0.14, 0.44);
    pushEdge(v[1], v[3], pal.primary, true);
    pushEdge(v[0], v[3], pal.secondary, false);
  } else if (ep.cageKind === "planes") {
    const p1: Vec3[] = [[0, -0.98, 0.34], [0.86, 0.54, -0.28], [-0.86, 0.54, -0.28]];
    const p2: Vec3[] = [[0, 0.98, -0.34], [-0.86, -0.54, 0.28], [0.86, -0.54, 0.28]];
    const p3: Vec3[] = [[-0.74, -0.44, -0.56], [0.74, 0.44, 0.56], [0.22, -0.82, 0.34]];
    pushPoly(p1, pal.facet, pal.primary, 0.16, 0.5);
    pushPoly(p2, pal.secondary, pal.tertiary, 0.13, 0.44);
    pushPoly(p3, pal.primary, pal.secondary, 0.1, 0.36);
  } else if (ep.cageKind === "prism") {
    const top: Vec3[] = [0, 1, 2].map((k) => {
      const a = (k * Math.PI * 2) / 3;
      return [Math.cos(a) * 0.86, -0.7, Math.sin(a) * 0.86];
    });
    const bot: Vec3[] = [0, 1, 2].map((k) => {
      const a = (k * Math.PI * 2) / 3 + 0.55;
      return [Math.cos(a) * 0.86, 0.7, Math.sin(a) * 0.86];
    });
    pushPoly(top, pal.facet, pal.secondary, 0.17, 0.48);
    pushPoly(bot, pal.primary, pal.tertiary, 0.13, 0.42);
    for (let k = 0; k < 3; k += 1) pushEdge(top[k], bot[k], pal.tertiary, true);
  }
}
