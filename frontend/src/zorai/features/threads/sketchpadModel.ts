export type SketchTool = "select" | "pen" | "highlighter" | "line" | "arrow" | "rect" | "ellipse" | "text" | "eraser" | "pan";
export type SketchShapeTool = Exclude<SketchTool, "select" | "pan"> | "image";
export type SketchPoint = { x: number; y: number; p?: number };
export type SketchShape = {
  id: string;
  tool: SketchShapeTool;
  color: string;
  width: number;
  points: SketchPoint[];
  text?: string;
  textWidth?: number;
  src?: string;
  rotation?: number;
};
export type SketchHandle = "nw" | "ne" | "sw" | "se" | "rotate";
export type SketchView = { x: number; y: number; scale: number };
export type SketchBackground = { src: string; width: number; height: number };
export type SketchBounds = { minX: number; minY: number; maxX: number; maxY: number };
export type SketchExportFrame = { width: number; height: number; scale: number; offsetX: number; offsetY: number };

export const SKETCH_MIN_SCALE = 0.1;
export const SKETCH_MAX_SCALE = 8;
export const SKETCH_FONT_FAMILY = 'system-ui, -apple-system, "Segoe UI", sans-serif';
export const SKETCH_TEXT_LINE_HEIGHT = 1.25;
const HIGHLIGHTER_WIDTH_FACTOR = 3;
const HIGHLIGHTER_ALPHA = 0.35;
const ERASER_WIDTH_FACTOR = 4;
const SKETCH_CLIPBOARD_PREFIX = "zorai-sketch:";
const sketchImages = new Map<string, HTMLImageElement>();

export async function loadSketchImages(shapes: readonly SketchShape[]): Promise<void> {
  await Promise.all(shapes.map(async (shape) => {
    if (shape.tool !== "image" || !shape.src || sketchImages.has(shape.src)) return;
    const image = new Image();
    image.src = shape.src;
    try {
      await image.decode();
      sketchImages.set(shape.src, image);
    } catch {
      return;
    }
  }));
}

export function serializeSketchClipboard(shapes: readonly SketchShape[]): string {
  return SKETCH_CLIPBOARD_PREFIX + JSON.stringify(shapes);
}

export function parseSketchClipboard(text: string | undefined | null): SketchShape[] | null {
  if (!text?.startsWith(SKETCH_CLIPBOARD_PREFIX)) return null;
  try {
    const parsed: unknown = JSON.parse(text.slice(SKETCH_CLIPBOARD_PREFIX.length));
    if (!Array.isArray(parsed)) return null;
    const shapes = parsed.filter((shape): shape is SketchShape => (
      typeof shape === "object" && shape !== null
      && typeof shape.tool === "string" && Array.isArray(shape.points)
      && shape.points.every((point: unknown) => typeof (point as SketchPoint)?.x === "number" && typeof (point as SketchPoint)?.y === "number")
    ));
    return shapes.length > 0 ? shapes : null;
  } catch {
    return null;
  }
}

export function duplicateShapes(shapes: readonly SketchShape[], dx: number, dy: number, newId: () => string): SketchShape[] {
  return shapes.map((shape) => ({ ...translateShape(shape, dx, dy), id: newId() }));
}

export function imageShapeAt(
  id: string,
  src: string,
  natural: { width: number; height: number },
  center: SketchPoint,
  maxSize: { width: number; height: number },
): SketchShape {
  const scale = Math.min(1, maxSize.width / natural.width, maxSize.height / natural.height);
  const width = natural.width * scale;
  const height = natural.height * scale;
  return {
    id,
    tool: "image",
    color: "#000000",
    width: 0,
    points: [{ x: center.x - width / 2, y: center.y - height / 2 }, { x: center.x + width / 2, y: center.y + height / 2 }],
    src,
  };
}

export function isFreehandTool(tool: SketchShapeTool): boolean {
  return tool === "pen" || tool === "highlighter" || tool === "eraser";
}

export function strokeWidthFor(shape: Pick<SketchShape, "tool" | "width">): number {
  if (shape.tool === "highlighter") return shape.width * HIGHLIGHTER_WIDTH_FACTOR;
  if (shape.tool === "eraser") return shape.width * ERASER_WIDTH_FACTOR;
  return shape.width;
}

export function pressureWidthFactor(pressure: number | undefined): number {
  if (pressure == null || !Number.isFinite(pressure)) return 1;
  return 0.25 + Math.min(1, Math.max(0, pressure)) * 1.5;
}

export function sketchFontSize(width: number): number {
  return 8 + width * 3;
}

export function sketchTextLines(text: string | undefined): string[] {
  return (text ?? "").split("\n");
}

function arrowHeadLength(width: number): number {
  return Math.max(10, width * 4);
}

export function sketchHasInk(shapes: readonly SketchShape[]): boolean {
  return shapes.some((shape) => shape.tool !== "eraser" && shape.points.length > 0);
}

export function shapeBounds(shape: SketchShape): SketchBounds | null {
  if (shape.points.length === 0) return null;
  if (shape.tool === "image") {
    const [a, b] = [shape.points[0], shape.points[shape.points.length - 1]];
    return { minX: Math.min(a.x, b.x), minY: Math.min(a.y, b.y), maxX: Math.max(a.x, b.x), maxY: Math.max(a.y, b.y) };
  }
  if (shape.tool === "text") {
    const anchor = shape.points[0];
    const fontSize = sketchFontSize(shape.width);
    return {
      minX: anchor.x,
      minY: anchor.y,
      maxX: anchor.x + (shape.textWidth ?? 0),
      maxY: anchor.y + sketchTextLines(shape.text).length * fontSize * SKETCH_TEXT_LINE_HEIGHT,
    };
  }
  const reach = strokeWidthFor(shape) / 2 * (shape.points.some((point) => point.p != null) ? pressureWidthFactor(1) : 1)
    + (shape.tool === "arrow" ? arrowHeadLength(shape.width) : 0);
  const bounds: SketchBounds = { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity };
  for (const point of shape.points) {
    bounds.minX = Math.min(bounds.minX, point.x - reach);
    bounds.minY = Math.min(bounds.minY, point.y - reach);
    bounds.maxX = Math.max(bounds.maxX, point.x + reach);
    bounds.maxY = Math.max(bounds.maxY, point.y + reach);
  }
  return bounds;
}

export function rotatePoint(point: SketchPoint, center: SketchPoint, angle: number): SketchPoint {
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const dx = point.x - center.x;
  const dy = point.y - center.y;
  return { ...point, x: center.x + dx * cos - dy * sin, y: center.y + dx * sin + dy * cos };
}

export function shapeCenter(shape: SketchShape): SketchPoint | null {
  const bounds = shapeBounds(shape);
  return bounds ? { x: (bounds.minX + bounds.maxX) / 2, y: (bounds.minY + bounds.maxY) / 2 } : null;
}

export function shapeCorners(shape: SketchShape): Record<"nw" | "ne" | "sw" | "se", SketchPoint> | null {
  const bounds = shapeBounds(shape);
  const center = shapeCenter(shape);
  if (!bounds || !center) return null;
  const angle = shape.rotation ?? 0;
  return {
    nw: rotatePoint({ x: bounds.minX, y: bounds.minY }, center, angle),
    ne: rotatePoint({ x: bounds.maxX, y: bounds.minY }, center, angle),
    sw: rotatePoint({ x: bounds.minX, y: bounds.maxY }, center, angle),
    se: rotatePoint({ x: bounds.maxX, y: bounds.maxY }, center, angle),
  };
}

export function rotatedShapeBounds(shape: SketchShape): SketchBounds | null {
  if (!shape.rotation) return shapeBounds(shape);
  const corners = shapeCorners(shape);
  if (!corners) return null;
  const points = Object.values(corners);
  return {
    minX: Math.min(...points.map((point) => point.x)),
    minY: Math.min(...points.map((point) => point.y)),
    maxX: Math.max(...points.map((point) => point.x)),
    maxY: Math.max(...points.map((point) => point.y)),
  };
}

export function rotateHandlePosition(shape: SketchShape, distance: number): SketchPoint | null {
  const bounds = shapeBounds(shape);
  const center = shapeCenter(shape);
  if (!bounds || !center) return null;
  return rotatePoint({ x: center.x, y: bounds.minY - distance }, center, shape.rotation ?? 0);
}

export function hitTestHandle(shape: SketchShape, point: SketchPoint, tolerance: number, rotateDistance: number): SketchHandle | null {
  const rotate = rotateHandlePosition(shape, rotateDistance);
  if (rotate && Math.hypot(point.x - rotate.x, point.y - rotate.y) <= tolerance) return "rotate";
  const corners = shapeCorners(shape);
  if (!corners) return null;
  for (const handle of ["nw", "ne", "sw", "se"] as const) {
    if (Math.hypot(point.x - corners[handle].x, point.y - corners[handle].y) <= tolerance) return handle;
  }
  return null;
}

const OPPOSITE_CORNER = { nw: "se", ne: "sw", sw: "ne", se: "nw" } as const;
const MIN_RESIZE_SIDE = 4;

export function resizeShape(shape: SketchShape, handle: "nw" | "ne" | "sw" | "se", pointer: SketchPoint, keepAspect: boolean): SketchShape {
  const bounds = shapeBounds(shape);
  const center = shapeCenter(shape);
  if (!bounds || !center) return shape;
  const angle = shape.rotation ?? 0;
  const local = rotatePoint(pointer, center, -angle);
  const corner = (name: "nw" | "ne" | "sw" | "se") => ({
    x: name === "nw" || name === "sw" ? bounds.minX : bounds.maxX,
    y: name === "nw" || name === "ne" ? bounds.minY : bounds.maxY,
  });
  const anchor = corner(OPPOSITE_CORNER[handle]);
  const dragged = corner(handle);
  const side = (from: number, to: number, value: number) => {
    const original = to - from;
    if (original === 0) return 1;
    const next = (value - from) * Math.sign(original);
    return Math.max(MIN_RESIZE_SIDE, next) / Math.abs(original);
  };
  let sx = side(anchor.x, dragged.x, local.x);
  let sy = side(anchor.y, dragged.y, local.y);
  if (keepAspect || shape.tool === "text" || shape.tool === "image") {
    const uniform = Math.max(sx, sy);
    sx = uniform;
    sy = uniform;
  }
  const scaled: SketchShape = {
    ...shape,
    points: shape.points.map((point) => ({ ...point, x: anchor.x + (point.x - anchor.x) * sx, y: anchor.y + (point.y - anchor.y) * sy })),
  };
  if (shape.tool === "text") {
    const fontSize = sketchFontSize(shape.width) * sx;
    scaled.width = Math.max(0.1, (fontSize - 8) / 3);
    scaled.textWidth = (shape.textWidth ?? 0) * sx;
  }
  if (!angle) return scaled;
  const before = rotatePoint(anchor, center, angle);
  const scaledCenter = shapeCenter(scaled) ?? center;
  const after = rotatePoint(anchor, scaledCenter, angle);
  return translateShape(scaled, before.x - after.x, before.y - after.y);
}

export function rotateShapeToward(shape: SketchShape, pointer: SketchPoint, snap: boolean): SketchShape {
  const center = shapeCenter(shape);
  if (!center) return shape;
  let angle = Math.atan2(pointer.y - center.y, pointer.x - center.x) + Math.PI / 2;
  if (snap) angle = Math.round(angle / (Math.PI / 12)) * (Math.PI / 12);
  angle = Math.atan2(Math.sin(angle), Math.cos(angle));
  return { ...shape, rotation: Math.abs(angle) < 1e-9 ? 0 : angle };
}

export function sketchBounds(shapes: readonly SketchShape[]): SketchBounds | null {
  const bounds: SketchBounds = { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity };
  for (const shape of shapes) {
    if (shape.tool === "eraser") continue;
    const own = rotatedShapeBounds(shape);
    if (!own) continue;
    bounds.minX = Math.min(bounds.minX, own.minX);
    bounds.minY = Math.min(bounds.minY, own.minY);
    bounds.maxX = Math.max(bounds.maxX, own.maxX);
    bounds.maxY = Math.max(bounds.maxY, own.maxY);
  }
  return Number.isFinite(bounds.minX) ? bounds : null;
}

function distanceToSegment(point: SketchPoint, a: SketchPoint, b: SketchPoint): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const lengthSquared = dx * dx + dy * dy;
  const t = lengthSquared === 0 ? 0 : Math.max(0, Math.min(1, ((point.x - a.x) * dx + (point.y - a.y) * dy) / lengthSquared));
  return Math.hypot(point.x - (a.x + t * dx), point.y - (a.y + t * dy));
}

export function shapeContainsPoint(shape: SketchShape, point: SketchPoint, tolerance: number): boolean {
  if (shape.tool === "eraser") return false;
  if (shape.rotation) {
    const center = shapeCenter(shape);
    if (center) point = rotatePoint(point, center, -shape.rotation);
  }
  if (shape.tool === "text" || shape.tool === "rect" || shape.tool === "ellipse" || shape.tool === "image") {
    const bounds = shapeBounds(shape);
    return Boolean(bounds)
      && point.x >= bounds!.minX - tolerance
      && point.x <= bounds!.maxX + tolerance
      && point.y >= bounds!.minY - tolerance
      && point.y <= bounds!.maxY + tolerance;
  }
  const reach = strokeWidthFor(shape) / 2 + tolerance;
  const points = isFreehandTool(shape.tool) ? shape.points : [shape.points[0], shape.points[shape.points.length - 1]];
  if (points.length === 1) return Math.hypot(point.x - points[0].x, point.y - points[0].y) <= reach;
  for (let index = 1; index < points.length; index += 1) {
    if (distanceToSegment(point, points[index - 1], points[index]) <= reach) return true;
  }
  return false;
}

export function hitTestShapes(shapes: readonly SketchShape[], point: SketchPoint, tolerance: number): SketchShape | null {
  for (let index = shapes.length - 1; index >= 0; index -= 1) {
    if (shapeContainsPoint(shapes[index], point, tolerance)) return shapes[index];
  }
  return null;
}

export function translateShape(shape: SketchShape, dx: number, dy: number): SketchShape {
  return { ...shape, points: shape.points.map((point) => ({ ...point, x: point.x + dx, y: point.y + dy })) };
}

export function sketchExportFrame(
  bounds: SketchBounds,
  options: { padding?: number; pixelRatio?: number; maxSide?: number } = {},
): SketchExportFrame {
  const padding = options.padding ?? 24;
  const pixelRatio = options.pixelRatio ?? 2;
  const maxSide = options.maxSide ?? 4096;
  const worldWidth = bounds.maxX - bounds.minX + padding * 2;
  const worldHeight = bounds.maxY - bounds.minY + padding * 2;
  const scale = Math.min(pixelRatio, maxSide / Math.max(worldWidth, worldHeight));
  return {
    width: Math.max(1, Math.round(worldWidth * scale)),
    height: Math.max(1, Math.round(worldHeight * scale)),
    scale,
    offsetX: (padding - bounds.minX) * scale,
    offsetY: (padding - bounds.minY) * scale,
  };
}

export function screenToWorld(view: SketchView, point: SketchPoint): SketchPoint {
  return { x: (point.x - view.x) / view.scale, y: (point.y - view.y) / view.scale };
}

export function worldToScreen(view: SketchView, point: SketchPoint): SketchPoint {
  return { x: point.x * view.scale + view.x, y: point.y * view.scale + view.y };
}

export function zoomViewAt(view: SketchView, anchor: SketchPoint, factor: number): SketchView {
  const scale = Math.min(SKETCH_MAX_SCALE, Math.max(SKETCH_MIN_SCALE, view.scale * factor));
  const world = screenToWorld(view, anchor);
  return { x: anchor.x - world.x * scale, y: anchor.y - world.y * scale, scale };
}

export function backgroundBounds(background: SketchBackground): SketchBounds {
  return { minX: 0, minY: 0, maxX: background.width, maxY: background.height };
}

export function unionBounds(a: SketchBounds | null, b: SketchBounds | null): SketchBounds | null {
  if (!a || !b) return a ?? b;
  return {
    minX: Math.min(a.minX, b.minX),
    minY: Math.min(a.minY, b.minY),
    maxX: Math.max(a.maxX, b.maxX),
    maxY: Math.max(a.maxY, b.maxY),
  };
}

export function fitViewToShapes(
  shapes: readonly SketchShape[],
  viewport: { width: number; height: number },
  padding = 24,
): SketchView {
  return fitViewToBounds(sketchBounds(shapes), viewport, padding);
}

export function fitViewToBounds(
  bounds: SketchBounds | null,
  viewport: { width: number; height: number },
  padding = 24,
): SketchView {
  if (!bounds || viewport.width <= 0 || viewport.height <= 0) return { x: 0, y: 0, scale: 1 };
  const width = bounds.maxX - bounds.minX + padding * 2;
  const height = bounds.maxY - bounds.minY + padding * 2;
  const scale = Math.min(1, Math.max(SKETCH_MIN_SCALE, Math.min(viewport.width / width, viewport.height / height)));
  return {
    x: (viewport.width - (bounds.maxX - bounds.minX) * scale) / 2 - bounds.minX * scale,
    y: (viewport.height - (bounds.maxY - bounds.minY) * scale) / 2 - bounds.minY * scale,
    scale,
  };
}

function drawFreehand(ctx: CanvasRenderingContext2D, points: SketchPoint[]): void {
  const first = points[0];
  const last = points[points.length - 1];
  if (points.length === 1) {
    ctx.beginPath();
    ctx.arc(first.x, first.y, (ctx.lineWidth * pressureWidthFactor(first.p)) / 2, 0, Math.PI * 2);
    ctx.fill();
    return;
  }
  if (points.every((point) => point.p == null)) {
    ctx.beginPath();
    ctx.moveTo(first.x, first.y);
    for (let index = 1; index < points.length - 1; index += 1) {
      const current = points[index];
      const next = points[index + 1];
      ctx.quadraticCurveTo(current.x, current.y, (current.x + next.x) / 2, (current.y + next.y) / 2);
    }
    ctx.lineTo(last.x, last.y);
    ctx.stroke();
    return;
  }
  const baseWidth = ctx.lineWidth;
  let start: SketchPoint = first;
  for (let index = 1; index < points.length; index += 1) {
    const current = points[index];
    const end = index === points.length - 1
      ? current
      : { x: (current.x + points[index + 1].x) / 2, y: (current.y + points[index + 1].y) / 2 };
    ctx.lineWidth = baseWidth * pressureWidthFactor(current.p);
    ctx.beginPath();
    ctx.moveTo(start.x, start.y);
    if (index === points.length - 1) ctx.lineTo(end.x, end.y);
    else ctx.quadraticCurveTo(current.x, current.y, end.x, end.y);
    ctx.stroke();
    start = end;
  }
  ctx.lineWidth = baseWidth;
}

export function drawSketchShape(ctx: CanvasRenderingContext2D, shape: SketchShape): void {
  const points = shape.points;
  if (points.length === 0) return;
  ctx.save();
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.lineWidth = strokeWidthFor(shape);
  ctx.strokeStyle = shape.color;
  ctx.fillStyle = shape.color;
  if (shape.tool === "highlighter") ctx.globalAlpha = HIGHLIGHTER_ALPHA;
  if (shape.tool === "eraser") ctx.globalCompositeOperation = "destination-out";
  const center = shape.rotation ? shapeCenter(shape) : null;
  if (center) {
    ctx.translate(center.x, center.y);
    ctx.rotate(shape.rotation!);
    ctx.translate(-center.x, -center.y);
  }

  const first = points[0];
  const last = points[points.length - 1];
  if (shape.tool === "image") {
    const image = shape.src ? sketchImages.get(shape.src) : undefined;
    if (image) ctx.drawImage(image, Math.min(first.x, last.x), Math.min(first.y, last.y), Math.abs(last.x - first.x), Math.abs(last.y - first.y));
  } else if (shape.tool === "text") {
    const fontSize = sketchFontSize(shape.width);
    ctx.font = `${fontSize}px ${SKETCH_FONT_FAMILY}`;
    ctx.textBaseline = "top";
    sketchTextLines(shape.text).forEach((line, index) => {
      ctx.fillText(line, first.x, first.y + index * fontSize * SKETCH_TEXT_LINE_HEIGHT + fontSize * (SKETCH_TEXT_LINE_HEIGHT - 1) / 2);
    });
  } else if (isFreehandTool(shape.tool)) {
    drawFreehand(ctx, points);
  } else if (shape.tool === "rect") {
    ctx.strokeRect(first.x, first.y, last.x - first.x, last.y - first.y);
  } else if (shape.tool === "ellipse") {
    ctx.beginPath();
    ctx.ellipse(
      (first.x + last.x) / 2,
      (first.y + last.y) / 2,
      Math.abs(last.x - first.x) / 2,
      Math.abs(last.y - first.y) / 2,
      0,
      0,
      Math.PI * 2,
    );
    ctx.stroke();
  } else {
    ctx.beginPath();
    ctx.moveTo(first.x, first.y);
    ctx.lineTo(last.x, last.y);
    if (shape.tool === "arrow" && (last.x !== first.x || last.y !== first.y)) {
      const angle = Math.atan2(last.y - first.y, last.x - first.x);
      const head = arrowHeadLength(shape.width);
      for (const side of [-1, 1]) {
        ctx.moveTo(last.x, last.y);
        ctx.lineTo(last.x - head * Math.cos(angle + side * Math.PI / 7), last.y - head * Math.sin(angle + side * Math.PI / 7));
      }
    }
    ctx.stroke();
  }
  ctx.restore();
}
