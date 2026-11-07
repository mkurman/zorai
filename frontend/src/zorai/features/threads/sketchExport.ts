import {
  drawSketchShape,
  loadSketchImages,
  sketchBounds,
  sketchExportFrame,
  sketchHasInk,
  type SketchBackground,
  type SketchShape,
} from "./sketchpadModel";

export async function sketchToPngFile(shapes: readonly SketchShape[], background: string): Promise<File | null> {
  const bounds = sketchBounds(shapes);
  if (!bounds || !sketchHasInk(shapes)) return null;
  await loadSketchImages(shapes);
  const frame = sketchExportFrame(bounds);
  const ink = document.createElement("canvas");
  ink.width = frame.width;
  ink.height = frame.height;
  const inkCtx = ink.getContext("2d");
  const canvas = document.createElement("canvas");
  canvas.width = frame.width;
  canvas.height = frame.height;
  const ctx = canvas.getContext("2d");
  if (!inkCtx || !ctx) return null;
  inkCtx.setTransform(frame.scale, 0, 0, frame.scale, frame.offsetX, frame.offsetY);
  for (const shape of shapes) drawSketchShape(inkCtx, shape);
  ctx.fillStyle = background;
  ctx.fillRect(0, 0, frame.width, frame.height);
  ctx.drawImage(ink, 0, 0);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) return null;
  return new File([blob], `sketch-${Date.now()}.png`, { type: "image/png" });
}

export async function annotateImageToPngFile(
  background: SketchBackground,
  shapes: readonly SketchShape[],
  name: string,
): Promise<File | null> {
  const image = new Image();
  image.src = background.src;
  try {
    await image.decode();
  } catch {
    return null;
  }
  await loadSketchImages(shapes);
  const ink = document.createElement("canvas");
  ink.width = background.width;
  ink.height = background.height;
  const inkCtx = ink.getContext("2d");
  const canvas = document.createElement("canvas");
  canvas.width = background.width;
  canvas.height = background.height;
  const ctx = canvas.getContext("2d");
  if (!inkCtx || !ctx) return null;
  for (const shape of shapes) drawSketchShape(inkCtx, shape);
  ctx.drawImage(image, 0, 0, background.width, background.height);
  ctx.drawImage(ink, 0, 0);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) return null;
  const baseName = name.replace(/\.[^./\\]+$/, "") || "image";
  return new File([blob], `${baseName}-annotated.png`, { type: "image/png" });
}
