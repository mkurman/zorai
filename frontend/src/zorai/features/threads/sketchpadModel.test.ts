import { describe, expect, it } from "vitest";
import {
  backgroundBounds,
  mergeSelection,
  normalizeBounds,
  shapesInMarquee,
  toggleSelection,
  hitTestHandle,
  resizeShape,
  rotatedShapeBounds,
  rotateShapeToward,
  shapeCorners,
  duplicateShapes,
  imageShapeAt,
  parseSketchClipboard,
  serializeSketchClipboard,
  fitViewToBounds,
  fitViewToShapes,
  unionBounds,
  hitTestShapes,
  pressureWidthFactor,
  shapeBounds,
  sketchFontSize,
  translateShape,
  screenToWorld,
  sketchBounds,
  sketchExportFrame,
  sketchHasInk,
  zoomViewAt,
  type SketchShape,
} from "./sketchpadModel";

function shape(tool: SketchShape["tool"], points: [number, number][], width = 4): SketchShape {
  return { id: `${tool}-${points.length}`, tool, color: "#000", width, points: points.map(([x, y]) => ({ x, y })) };
}

describe("sketch export", () => {
  it("does not send an attachment when only eraser strokes are left", () => {
    expect(sketchHasInk([shape("eraser", [[0, 0], [10, 10]])])).toBe(false);
    expect(sketchBounds([shape("eraser", [[0, 0], [10, 10]])])).toBeNull();
    expect(sketchHasInk([shape("pen", [[0, 0]])])).toBe(true);
  });

  it("crops the PNG to the drawing including stroke thickness, so thick edges are not clipped", () => {
    const bounds = sketchBounds([shape("pen", [[100, 50], [300, 150]], 8), shape("eraser", [[-1000, -1000]])]);

    expect(bounds).toEqual({ minX: 96, minY: 46, maxX: 304, maxY: 154 });
  });

  it("reserves room for the arrow head beyond the arrow's end point", () => {
    const bounds = sketchBounds([shape("arrow", [[0, 0], [100, 0]], 4)]);

    expect(bounds!.maxX).toBeGreaterThanOrEqual(100 + 16);
  });

  it("maps the drawing into the padded image at 2x and caps huge drawings to the max side", () => {
    const frame = sketchExportFrame({ minX: 96, minY: 46, maxX: 304, maxY: 154 });
    expect(frame).toEqual({ width: 512, height: 312, scale: 2, offsetX: (24 - 96) * 2, offsetY: (24 - 46) * 2 });

    const huge = sketchExportFrame({ minX: 0, minY: 0, maxX: 20_000, maxY: 1_000 });
    expect(huge.width).toBeLessThanOrEqual(4096);
    expect(huge.scale).toBeLessThan(1);
  });
});

describe("infinite canvas view", () => {
  it("keeps the point under the cursor fixed while zooming", () => {
    const view = { x: 40, y: -20, scale: 1.5 };
    const anchor = { x: 200, y: 120 };
    const before = screenToWorld(view, anchor);
    const after = screenToWorld(zoomViewAt(view, anchor, 2), anchor);

    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
    expect(zoomViewAt(view, anchor, 1000).scale).toBe(8);
  });

  it("fits a drawing that was panned far away back into the viewport", () => {
    const shapes = [shape("rect", [[5000, 5000], [5200, 5100]], 2)];
    const view = fitViewToShapes(shapes, { width: 600, height: 300 });
    const bounds = sketchBounds(shapes)!;

    expect(bounds.minX * view.scale + view.x).toBeGreaterThanOrEqual(0);
    expect(bounds.maxX * view.scale + view.x).toBeLessThanOrEqual(600);
    expect(bounds.minY * view.scale + view.y).toBeGreaterThanOrEqual(0);
    expect(bounds.maxY * view.scale + view.y).toBeLessThanOrEqual(300);
  });
});

describe("editing shapes after drawing", () => {
  it("selects the shape drawn last when shapes overlap, ignoring eraser strokes on top", () => {
    const below = shape("rect", [[0, 0], [100, 100]]);
    const above = shape("pen", [[10, 50], [90, 50]]);
    const eraser = shape("eraser", [[0, 50], [100, 50]]);

    expect(hitTestShapes([below, above, eraser], { x: 50, y: 51 }, 2)?.id).toBe(above.id);
    expect(hitTestShapes([below, above, eraser], { x: 50, y: 80 }, 2)?.id).toBe(below.id);
    expect(hitTestShapes([below, above, eraser], { x: 500, y: 500 }, 2)).toBeNull();
  });

  it("only picks a line when the pointer is near the stroke, not anywhere in its bounding box", () => {
    const diagonal = shape("line", [[0, 0], [100, 100]]);

    expect(hitTestShapes([diagonal], { x: 52, y: 50 }, 2)?.id).toBe(diagonal.id);
    expect(hitTestShapes([diagonal], { x: 90, y: 10 }, 2)).toBeNull();
  });

  it("moves every point of a shape and keeps its pressure", () => {
    const moved = translateShape({ ...shape("pen", [[1, 2], [3, 4]]), points: [{ x: 1, y: 2, p: 0.8 }, { x: 3, y: 4 }] }, 10, -5);

    expect(moved.points).toEqual([{ x: 11, y: -3, p: 0.8 }, { x: 13, y: -1 }]);
  });

  it("sizes text by its measured width and line count so it is exported and selectable", () => {
    const text = { ...shape("text", [[10, 20]], 4), text: "hello\nworld", textWidth: 60 };
    const lineHeight = sketchFontSize(4) * 1.25;

    expect(shapeBounds(text)).toEqual({ minX: 10, minY: 20, maxX: 70, maxY: 20 + lineHeight * 2 });
    expect(hitTestShapes([text], { x: 65, y: 20 + lineHeight * 1.5 }, 0)?.id).toBe(text.id);
  });
});

describe("pen pressure", () => {
  it("keeps mouse strokes at the chosen width and thins or thickens pen strokes with pressure", () => {
    expect(pressureWidthFactor(undefined)).toBe(1);
    expect(pressureWidthFactor(0.5)).toBe(1);
    expect(pressureWidthFactor(0)).toBeLessThan(0.5);
    expect(pressureWidthFactor(1)).toBeGreaterThan(1.5);
  });

  it("widens the export crop for pressure strokes so a hard press is not clipped", () => {
    const mouse = shapeBounds(shape("pen", [[0, 0]], 8))!;
    const pen = shapeBounds({ ...shape("pen", [[0, 0]], 8), points: [{ x: 0, y: 0, p: 1 }] })!;

    expect(pen.maxX).toBeGreaterThanOrEqual(8 / 2 * pressureWidthFactor(1));
    expect(pen.maxX).toBeGreaterThan(mouse.maxX);
  });
});

describe("image annotation view", () => {
  it("opens a large photo scaled down to fit the pad, never enlarged past 100%", () => {
    const photo = backgroundBounds({ src: "", width: 4000, height: 3000 });
    const view = fitViewToBounds(photo, { width: 800, height: 300 }, 12);

    expect(view.scale).toBeCloseTo(300 / (3000 + 24));
    expect(photo.maxX * view.scale + view.x).toBeLessThanOrEqual(800);
    expect(fitViewToBounds(backgroundBounds({ src: "", width: 100, height: 50 }), { width: 800, height: 300 }).scale).toBe(1);
  });

  it("fits both the image and annotations drawn outside it", () => {
    expect(unionBounds(backgroundBounds({ src: "", width: 100, height: 50 }), { minX: -20, minY: 10, maxX: 40, maxY: 90 }))
      .toEqual({ minX: -20, minY: 0, maxX: 100, maxY: 90 });
    expect(unionBounds(null, { minX: 1, minY: 2, maxX: 3, maxY: 4 })).toEqual({ minX: 1, minY: 2, maxX: 3, maxY: 4 });
  });
});

describe("sketch copy, paste and images", () => {
  const stroke: SketchShape = { id: "s1", tool: "pen", color: "#e5484d", width: 4, points: [{ x: 0, y: 0 }, { x: 10, y: 5 }] };

  it("round-trips copied shapes through clipboard text and ignores ordinary text", () => {
    const text = serializeSketchClipboard([stroke]);

    expect(parseSketchClipboard(text)).toEqual([stroke]);
    expect(parseSketchClipboard("hello")).toBeNull();
    expect(parseSketchClipboard("zorai-sketch:{broken")).toBeNull();
    expect(parseSketchClipboard('zorai-sketch:[{"tool":"pen","points":[{"x":"a"}]}]')).toBeNull();
  });

  it("pastes copies under new ids, offset so they do not hide the original", () => {
    let serial = 0;
    const [copy] = duplicateShapes([stroke], 16, 16, () => `new-${++serial}`);

    expect(copy.id).toBe("new-1");
    expect(copy.points).toEqual([{ x: 16, y: 16 }, { x: 26, y: 21 }]);
    expect(stroke.points[0]).toEqual({ x: 0, y: 0 });
  });

  it("centres an inserted image, shrinks it to fit the view, and lets select hit it anywhere inside", () => {
    const image = imageShapeAt("img", "data:image/png;base64,AAAA", { width: 2000, height: 1000 }, { x: 100, y: 100 }, { width: 400, height: 400 });

    expect(shapeBounds(image)).toEqual({ minX: -100, minY: 0, maxX: 300, maxY: 200 });
    expect(hitTestShapes([image], { x: 100, y: 100 }, 0)?.id).toBe("img");
    expect(sketchHasInk([image])).toBe(true);
    expect(shapeBounds(imageShapeAt("small", "x", { width: 40, height: 20 }, { x: 0, y: 0 }, { width: 400, height: 400 }))).toEqual({ minX: -20, minY: -10, maxX: 20, maxY: 10 });
  });
});

describe("resizing and rotating a selected object", () => {
  const box: SketchShape = { id: "r", tool: "rect", color: "#000", width: 0, points: [{ x: 0, y: 0 }, { x: 100, y: 50 }] };

  it("drags a corner while the opposite corner stays put", () => {
    expect(shapeBounds(resizeShape(box, "se", { x: 200, y: 150 }, false))).toEqual({ minX: 0, minY: 0, maxX: 200, maxY: 150 });
    expect(shapeBounds(resizeShape(box, "nw", { x: 50, y: 25 }, false))).toEqual({ minX: 50, minY: 25, maxX: 100, maxY: 50 });
    expect(shapeBounds(resizeShape(box, "se", { x: 200, y: 60 }, true))).toEqual({ minX: 0, minY: 0, maxX: 200, maxY: 100 });
  });

  it("never collapses or flips an object dragged past its anchor", () => {
    const squashed = shapeBounds(resizeShape(box, "se", { x: -500, y: -500 }, false))!;

    expect(squashed.minX).toBe(0);
    expect(squashed.maxX).toBeGreaterThan(0);
    expect(squashed.maxY).toBeGreaterThan(0);
  });

  it("scales text and images proportionally so they do not distort", () => {
    const text: SketchShape = { id: "t", tool: "text", color: "#000", width: 4, points: [{ x: 0, y: 0 }], text: "hi", textWidth: 40 };
    const bigger = resizeShape(text, "se", { x: 80, y: 1 }, false);

    expect(bigger.textWidth).toBeCloseTo(80);
    expect(sketchFontSize(bigger.width)).toBeCloseTo(sketchFontSize(4) * 2);
  });

  it("rotates around the centre, snaps with Shift, and hit-tests in the rotated frame", () => {
    const turned = rotateShapeToward(box, { x: 150, y: 25 }, false);
    expect(turned.rotation).toBeCloseTo(Math.PI / 2);
    expect(rotateShapeToward(box, { x: 52, y: -100 }, true).rotation).toBe(0);
    expect(rotatedShapeBounds(turned)).toMatchObject({ minX: expect.closeTo(25), maxX: expect.closeTo(75), minY: expect.closeTo(-25), maxY: expect.closeTo(75) });
    expect(hitTestShapes([turned], { x: 50, y: -20 }, 0)?.id).toBe("r");
    expect(hitTestShapes([turned], { x: 95, y: 25 }, 0)).toBeNull();
    expect(hitTestHandle(turned, { x: 50 + 47, y: 25 }, 4, 22)).toBe("rotate");
  });

  it("keeps the anchored corner fixed when resizing a rotated object", () => {
    const turned = { ...box, rotation: Math.PI / 2 };
    const anchorBefore = shapeCorners(turned)!.nw;
    const resized = resizeShape(turned, "se", { x: -50, y: 200 }, false);

    expect(shapeCorners(resized)!.nw.x).toBeCloseTo(anchorBefore.x);
    expect(shapeCorners(resized)!.nw.y).toBeCloseTo(anchorBefore.y);
  });
});

describe("selecting several objects", () => {
  const left: SketchShape = { id: "a", tool: "rect", color: "#000", width: 0, points: [{ x: 0, y: 0 }, { x: 10, y: 10 }] };
  const right: SketchShape = { id: "b", tool: "rect", color: "#000", width: 0, points: [{ x: 100, y: 0 }, { x: 110, y: 10 }] };
  const eraser: SketchShape = { id: "e", tool: "eraser", color: "#000", width: 4, points: [{ x: 5, y: 5 }, { x: 6, y: 6 }] };

  it("box-selects everything the box touches, dragged in any direction, but never eraser strokes", () => {
    expect(shapesInMarquee([left, right, eraser], normalizeBounds({ x: 8, y: 20 }, { x: -5, y: 5 }))).toEqual(["a"]);
    expect(shapesInMarquee([left, right, eraser], normalizeBounds({ x: -5, y: -5 }, { x: 105, y: 5 }))).toEqual(["a", "b"]);
  });

  it("Ctrl+click toggles one object and Ctrl+drag adds to what was already selected", () => {
    expect(toggleSelection(["a"], "b")).toEqual(["a", "b"]);
    expect(toggleSelection(["a", "b"], "a")).toEqual(["b"]);
    expect(mergeSelection(["a"], ["a", "b"])).toEqual(["a", "b"]);
  });
});
