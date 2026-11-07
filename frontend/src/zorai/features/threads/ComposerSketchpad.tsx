import { useCallback, useEffect, useLayoutEffect, useRef, useState, type DragEvent, type KeyboardEvent, type PointerEvent, type Ref } from "react";
import {
  backgroundBounds,
  drawSketchShape,
  duplicateShapes,
  fitViewToBounds,
  hitTestHandle,
  hitTestShapes,
  imageShapeAt,
  isFreehandTool,
  loadSketchImages,
  mergeSelection,
  normalizeBounds,
  parseSketchClipboard,
  resizeShape,
  rotateHandlePosition,
  rotateShapeToward,
  shapeCorners,
  screenToWorld,
  SKETCH_FONT_FAMILY,
  SKETCH_TEXT_LINE_HEIGHT,
  sketchBounds,
  sketchFontSize,
  sketchHasInk,
  sketchTextLines,
  serializeSketchClipboard,
  shapesInMarquee,
  toggleSelection,
  translateShape,
  unionBounds,
  worldToScreen,
  zoomViewAt,
  type SketchBackground,
  type SketchHandle,
  type SketchPoint,
  type SketchShape,
  type SketchTool,
  type SketchView,
} from "./sketchpadModel";
import { annotateImageToPngFile, sketchToPngFile } from "./sketchExport";
import { useComposerPanelHeight } from "./useComposerPanelHeight";

const SKETCH_COLORS = ["ink", "#e5484d", "#f5a524", "#3e9b4f", "#3b82f6", "#a571f0"] as const;
const SKETCH_WIDTHS = [2, 4, 8] as const;
const SKETCH_TOOLS: { tool: SketchTool; label: string; shortcut: string }[] = [
  { tool: "select", label: "Select and move", shortcut: "V" },
  { tool: "pen", label: "Pen", shortcut: "P" },
  { tool: "highlighter", label: "Highlighter", shortcut: "H" },
  { tool: "line", label: "Line", shortcut: "L" },
  { tool: "arrow", label: "Arrow", shortcut: "A" },
  { tool: "rect", label: "Rectangle", shortcut: "R" },
  { tool: "ellipse", label: "Ellipse", shortcut: "O" },
  { tool: "text", label: "Text", shortcut: "T" },
  { tool: "eraser", label: "Eraser", shortcut: "E" },
  { tool: "pan", label: "Pan", shortcut: "Space" },
];

type SketchTextEditor = {
  id: string | null;
  anchor: SketchPoint;
  value: string;
  color: string;
  width: number;
};

type SketchMove = { pointerId: number; origin: SketchPoint; ids: string[]; dx: number; dy: number };
type SketchMarquee = { pointerId: number; start: SketchPoint; end: SketchPoint; base: string[] };
type SketchTransform = { pointerId: number; handle: SketchHandle; shape: SketchShape; preview: SketchShape | null };

const HANDLE_HIT_PX = 8;
const HANDLE_SIZE_PX = 8;
const ROTATE_HANDLE_PX = 22;
const HANDLE_CURSORS: Record<SketchHandle, string> = { nw: "nwse-resize", se: "nwse-resize", ne: "nesw-resize", sw: "nesw-resize", rotate: "grab" };

function marqueeHasArea(marquee: SketchMarquee, scale: number): boolean {
  return Math.hypot(marquee.end.x - marquee.start.x, marquee.end.y - marquee.start.y) * scale > 3;
}

function newShapeId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function readImageFile(file: File): Promise<{ src: string; width: number; height: number } | null> {
  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onerror = () => resolve(null);
    reader.onload = () => {
      const src = String(reader.result);
      const image = new Image();
      image.onload = () => resolve(image.naturalWidth > 0 ? { src, width: image.naturalWidth, height: image.naturalHeight } : null);
      image.onerror = () => resolve(null);
      image.src = src;
    };
    reader.readAsDataURL(file);
  });
}

let measureContext: CanvasRenderingContext2D | null = null;

function measureSketchText(text: string, width: number): number {
  measureContext ??= document.createElement("canvas").getContext("2d");
  if (!measureContext) return 0;
  measureContext.font = `${sketchFontSize(width)}px ${SKETCH_FONT_FAMILY}`;
  return Math.max(...sketchTextLines(text).map((line) => measureContext!.measureText(line).width));
}

type Props = {
  ref?: Ref<HTMLDivElement>;
  shapes: SketchShape[];
  onShapesChange: (shapes: SketchShape[]) => void;
  onClose: () => void;
  background?: SketchBackground;
  primaryLabel: string;
  primaryTitle: string;
  primaryDisabled?: boolean;
  onPrimary: () => void;
  onSaveDraft: () => void;
  busy?: boolean;
};

export function ComposerSketchpad({
  ref,
  shapes,
  onShapesChange,
  onClose,
  background,
  primaryLabel,
  primaryTitle,
  primaryDisabled,
  onPrimary,
  onSaveDraft,
  busy,
}: Props) {
  const surfaceRef = useRef<HTMLDivElement | null>(null);
  const resize = useComposerPanelHeight(320, 180);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [tool, setTool] = useState<SketchTool>("pen");
  const [color, setColor] = useState<string>(SKETCH_COLORS[0]);
  const [width, setWidth] = useState<number>(SKETCH_WIDTHS[1]);
  const [view, setView] = useState<SketchView>({ x: 0, y: 0, scale: 1 });
  const [past, setPast] = useState<SketchShape[][]>([]);
  const [future, setFuture] = useState<SketchShape[][]>([]);
  const [spaceHeld, setSpaceHeld] = useState(false);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [editor, setEditor] = useState<SketchTextEditor | null>(null);
  const draftRef = useRef<SketchShape | null>(null);
  const panRef = useRef<{ pointerId: number; last: SketchPoint } | null>(null);
  const moveRef = useRef<SketchMove | null>(null);
  const marqueeRef = useRef<SketchMarquee | null>(null);
  const transformRef = useRef<SketchTransform | null>(null);
  const [hoverHandle, setHoverHandle] = useState<SketchHandle | null>(null);
  const editorRef = useRef(editor);
  editorRef.current = editor;
  const selectedIdsRef = useRef(selectedIds);
  selectedIdsRef.current = selectedIds;
  const singleSelected = () => (selectedIdsRef.current.length === 1
    ? shapesRef.current.find((shape) => shape.id === selectedIdsRef.current[0]) ?? null
    : null);
  const textInputRef = useRef<HTMLTextAreaElement | null>(null);
  const fittedBackgroundRef = useRef(false);
  const viewRef = useRef(view);
  viewRef.current = view;
  const shapesRef = useRef(shapes);
  shapesRef.current = shapes;
  const imageInputRef = useRef<HTMLInputElement | null>(null);
  const lastPasteRef = useRef<{ text: string; count: number } | null>(null);
  const clipboardRef = useRef<{ copy: (event: ClipboardEvent, cut: boolean) => void; paste: (event: ClipboardEvent) => void } | null>(null);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const current = viewRef.current;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(current.scale * dpr, 0, 0, current.scale * dpr, current.x * dpr, current.y * dpr);
    const move = moveRef.current;
    const transform = transformRef.current;
    const editingId = editorRef.current?.id;
    const marquee = marqueeRef.current;
    const marqueeBounds = marquee && marqueeHasArea(marquee, current.scale) ? normalizeBounds(marquee.start, marquee.end) : null;
    const highlighted = marquee && marqueeBounds
      ? mergeSelection(marquee.base, shapesInMarquee(shapesRef.current, marqueeBounds))
      : selectedIdsRef.current;
    const selectedShapes: SketchShape[] = [];
    for (const stored of shapesRef.current) {
      if (stored.id === editingId) continue;
      const shape = move && move.ids.includes(stored.id)
        ? translateShape(stored, move.dx, move.dy)
        : transform?.preview && stored.id === transform.shape.id ? transform.preview : stored;
      if (highlighted.includes(shape.id)) selectedShapes.push(shape);
      drawSketchShape(ctx, shape);
    }
    if (draftRef.current) drawSketchShape(ctx, draftRef.current);
    const accentColor = canvas.parentElement
      ? getComputedStyle(canvas.parentElement).getPropertyValue("--zorai-accent").trim() || "#3b82f6"
      : "#3b82f6";
    if (selectedShapes.length > 1 || marquee) {
      const unit = 1 / current.scale;
      ctx.save();
      ctx.strokeStyle = accentColor;
      ctx.lineWidth = unit;
      ctx.setLineDash([4 * unit, 3 * unit]);
      for (const shape of selectedShapes) {
        const outline = shapeCorners(shape);
        if (!outline) continue;
        ctx.beginPath();
        ctx.moveTo(outline.nw.x, outline.nw.y);
        ctx.lineTo(outline.ne.x, outline.ne.y);
        ctx.lineTo(outline.se.x, outline.se.y);
        ctx.lineTo(outline.sw.x, outline.sw.y);
        ctx.closePath();
        ctx.stroke();
      }
      if (marqueeBounds) {
        ctx.setLineDash([]);
        ctx.fillStyle = accentColor;
        ctx.globalAlpha = 0.12;
        ctx.fillRect(marqueeBounds.minX, marqueeBounds.minY, marqueeBounds.maxX - marqueeBounds.minX, marqueeBounds.maxY - marqueeBounds.minY);
        ctx.globalAlpha = 1;
        ctx.strokeRect(marqueeBounds.minX, marqueeBounds.minY, marqueeBounds.maxX - marqueeBounds.minX, marqueeBounds.maxY - marqueeBounds.minY);
      }
      ctx.restore();
    }
    const selected = selectedShapes.length === 1 && !marquee ? selectedShapes[0] : null;
    const corners = selected ? shapeCorners(selected) : null;
    const rotateHandle = selected ? rotateHandlePosition(selected, ROTATE_HANDLE_PX / current.scale) : null;
    if (corners && rotateHandle) {
      const accent = accentColor;
      const unit = 1 / current.scale;
      ctx.save();
      ctx.strokeStyle = accent;
      ctx.lineWidth = unit;
      ctx.setLineDash([4 * unit, 3 * unit]);
      ctx.beginPath();
      ctx.moveTo(corners.nw.x, corners.nw.y);
      ctx.lineTo(corners.ne.x, corners.ne.y);
      ctx.lineTo(corners.se.x, corners.se.y);
      ctx.lineTo(corners.sw.x, corners.sw.y);
      ctx.closePath();
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.beginPath();
      ctx.moveTo((corners.nw.x + corners.ne.x) / 2, (corners.nw.y + corners.ne.y) / 2);
      ctx.lineTo(rotateHandle.x, rotateHandle.y);
      ctx.stroke();
      ctx.fillStyle = "#ffffff";
      const half = (HANDLE_SIZE_PX / 2) * unit;
      for (const corner of Object.values(corners)) {
        ctx.fillRect(corner.x - half, corner.y - half, half * 2, half * 2);
        ctx.strokeRect(corner.x - half, corner.y - half, half * 2, half * 2);
      }
      ctx.beginPath();
      ctx.arc(rotateHandle.x, rotateHandle.y, half * 1.2, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      ctx.restore();
    }
  }, []);

  useLayoutEffect(() => {
    const surface = surfaceRef.current;
    const canvas = canvasRef.current;
    if (!surface || !canvas) return;
    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.max(1, Math.round(surface.clientWidth * dpr));
      canvas.height = Math.max(1, Math.round(surface.clientHeight * dpr));
      if (background && !fittedBackgroundRef.current && surface.clientWidth > 0 && surface.clientHeight > 0) {
        fittedBackgroundRef.current = true;
        const fitted = fitViewToBounds(backgroundBounds(background), { width: surface.clientWidth, height: surface.clientHeight }, 12);
        viewRef.current = fitted;
        setView(fitted);
      }
      draw();
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(surface);
    return () => observer.disconnect();
  }, [background, draw]);

  useEffect(() => {
    draw();
  }, [draw, shapes, view, selectedIds, editor]);

  useEffect(() => {
    let live = true;
    void loadSketchImages(shapes).then(() => {
      if (live) draw();
    });
    return () => {
      live = false;
    };
  }, [draw, shapes]);

  useEffect(() => {
    const onCopy = (event: ClipboardEvent) => clipboardRef.current?.copy(event, false);
    const onCut = (event: ClipboardEvent) => clipboardRef.current?.copy(event, true);
    const onPaste = (event: ClipboardEvent) => clipboardRef.current?.paste(event);
    document.addEventListener("copy", onCopy);
    document.addEventListener("cut", onCut);
    document.addEventListener("paste", onPaste);
    return () => {
      document.removeEventListener("copy", onCopy);
      document.removeEventListener("cut", onCut);
      document.removeEventListener("paste", onPaste);
    };
  }, []);

  useEffect(() => {
    if (!editor) return;
    const input = textInputRef.current;
    if (input && document.activeElement !== input) {
      input.focus();
      input.setSelectionRange(input.value.length, input.value.length);
    }
  }, [editor]);

  useEffect(() => {
    const surface = surfaceRef.current;
    if (!surface) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const rect = surface.getBoundingClientRect();
      if (event.ctrlKey || event.metaKey) {
        const anchor = { x: event.clientX - rect.left, y: event.clientY - rect.top };
        setView((current) => zoomViewAt(current, anchor, Math.exp(-event.deltaY * 0.0025)));
      } else {
        setView((current) => ({ ...current, x: current.x - event.deltaX, y: current.y - event.deltaY }));
      }
    };
    surface.addEventListener("wheel", onWheel, { passive: false });
    return () => surface.removeEventListener("wheel", onWheel);
  }, []);

  const commit = (next: SketchShape[]) => {
    setPast((current) => [...current, shapesRef.current]);
    setFuture([]);
    onShapesChange(next);
  };

  const undo = () => {
    if (past.length === 0) return;
    setFuture((current) => [shapesRef.current, ...current]);
    setPast((current) => current.slice(0, -1));
    onShapesChange(past[past.length - 1]);
  };

  const redo = () => {
    if (future.length === 0) return;
    setPast((current) => [...current, shapesRef.current]);
    setFuture((current) => current.slice(1));
    onShapesChange(future[0]);
  };

  const commitEditor = () => {
    const current = editorRef.current;
    if (!current) return;
    editorRef.current = null;
    setEditor(null);
    const text = current.value.replace(/\s+$/, "");
    const existing = current.id ? shapesRef.current.find((shape) => shape.id === current.id) : undefined;
    if (!text) {
      if (existing) commit(shapesRef.current.filter((shape) => shape.id !== existing.id));
      return;
    }
    if (existing?.text === text) return;
    const shape: SketchShape = {
      id: current.id ?? newShapeId(),
      tool: "text",
      color: current.color,
      width: current.width,
      points: [current.anchor],
      text,
      textWidth: measureSketchText(text, current.width),
    };
    commit(existing
      ? shapesRef.current.map((entry) => (entry.id === existing.id ? shape : entry))
      : [...shapesRef.current, shape]);
  };

  const cancelEditor = () => {
    editorRef.current = null;
    setEditor(null);
    surfaceRef.current?.focus();
  };

  const editText = (shape: SketchShape) => {
    setSelectedIds([]);
    setEditor({ id: shape.id, anchor: shape.points[0], value: shape.text ?? "", color: shape.color, width: shape.width });
  };

  const chooseTool = (next: SketchTool) => {
    commitEditor();
    if (next !== "select") setSelectedIds([]);
    setTool(next);
  };

  const deleteSelected = () => {
    if (selectedIds.length === 0) return;
    commit(shapesRef.current.filter((shape) => !selectedIds.includes(shape.id)));
    setSelectedIds([]);
  };

  const addShapes = (added: SketchShape[]) => {
    if (added.length === 0) return;
    commitEditor();
    commit([...shapesRef.current, ...added]);
    setTool("select");
    setSelectedIds(added.map((shape) => shape.id));
    surfaceRef.current?.focus();
  };

  const insertImages = async (files: File[], at?: SketchPoint) => {
    const surface = surfaceRef.current;
    if (!surface) return;
    const loaded = (await Promise.all(files.filter((file) => file.type.startsWith("image/")).map(readImageFile)))
      .filter((image): image is NonNullable<typeof image> => image !== null);
    if (loaded.length === 0) return;
    const current = viewRef.current;
    const center = screenToWorld(current, at ?? { x: surface.clientWidth / 2, y: surface.clientHeight / 2 });
    const maxSize = { width: (surface.clientWidth * 0.6) / current.scale, height: (surface.clientHeight * 0.6) / current.scale };
    const step = 16 / current.scale;
    const added = loaded.map((image, index) => imageShapeAt(newShapeId(), image.src, image, { x: center.x + index * step, y: center.y + index * step }, maxSize));
    await loadSketchImages(added);
    addShapes(added);
  };

  clipboardRef.current = {
    copy: (event, cut) => {
      if (!surfaceRef.current || document.activeElement !== surfaceRef.current || editorRef.current || !event.clipboardData) return;
      const copied = shapesRef.current.filter((entry) => selectedIdsRef.current.includes(entry.id));
      if (copied.length === 0) return;
      event.preventDefault();
      event.clipboardData.setData("text/plain", serializeSketchClipboard(copied));
      lastPasteRef.current = null;
      if (cut) {
        commit(shapesRef.current.filter((entry) => !selectedIdsRef.current.includes(entry.id)));
        setSelectedIds([]);
      }
    },
    paste: (event) => {
      if (!surfaceRef.current || document.activeElement !== surfaceRef.current || editorRef.current || !event.clipboardData) return;
      const text = event.clipboardData.getData("text/plain");
      const copied = parseSketchClipboard(text);
      if (copied) {
        event.preventDefault();
        const count = lastPasteRef.current?.text === text ? lastPasteRef.current.count + 1 : 1;
        lastPasteRef.current = { text, count };
        const offset = (16 * count) / viewRef.current.scale;
        addShapes(duplicateShapes(copied, offset, offset, newShapeId));
        return;
      }
      const files = Array.from(event.clipboardData.items)
        .filter((item) => item.kind === "file" && item.type.startsWith("image/"))
        .map((item) => item.getAsFile())
        .filter((file): file is File => file !== null);
      if (files.length === 0) return;
      event.preventDefault();
      void insertImages(files);
    },
  };

  const [exporting, setExporting] = useState(false);
  const [dropActive, setDropActive] = useState(false);
  const draggingFiles = (event: DragEvent<HTMLDivElement>) => Array.from(event.dataTransfer.items).some((item) => item.kind === "file" && item.type.startsWith("image/"));
  const downloadPng = async () => {
    commitEditor();
    setExporting(true);
    try {
      const panel = resize.panelRef.current;
      const file = background
        ? await annotateImageToPngFile(background, shapesRef.current, "image")
        : await sketchToPngFile(shapesRef.current, panel ? getComputedStyle(panel).backgroundColor : "#ffffff");
      if (!file) return;
      const url = URL.createObjectURL(file);
      const link = document.createElement("a");
      link.href = url;
      link.download = file.name;
      link.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } finally {
      setExporting(false);
    }
  };

  const fitView = () => {
    const surface = surfaceRef.current;
    if (!surface) return;
    const bounds = unionBounds(background ? backgroundBounds(background) : null, sketchBounds(shapes));
    setView(fitViewToBounds(bounds, { width: surface.clientWidth, height: surface.clientHeight }, background ? 12 : 24));
  };

  const localPoint = (event: PointerEvent<HTMLElement>): SketchPoint => {
    const rect = event.currentTarget.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  };

  const resolveColor = () => {
    if (color !== "ink") return color;
    const surface = surfaceRef.current;
    return surface ? getComputedStyle(surface).color : "#000000";
  };

  const pointerWorld = (event: PointerEvent<HTMLDivElement>, point: SketchPoint): SketchPoint => {
    const world = screenToWorld(viewRef.current, point);
    return event.pointerType === "pen" ? { ...world, p: event.pressure } : world;
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (editorRef.current) {
      event.preventDefault();
      commitEditor();
      return;
    }
    event.currentTarget.focus();
    event.currentTarget.setPointerCapture(event.pointerId);
    const point = localPoint(event);
    if (tool === "pan" || spaceHeld || event.button === 1) {
      panRef.current = { pointerId: event.pointerId, last: point };
      return;
    }
    if (event.button !== 0) return;
    const world = screenToWorld(viewRef.current, point);
    const tolerance = 6 / viewRef.current.scale;
    if (tool === "select") {
      const additive = event.ctrlKey || event.metaKey;
      const selectedShape = additive ? null : singleSelected();
      const handle = selectedShape
        ? hitTestHandle(selectedShape, world, HANDLE_HIT_PX / viewRef.current.scale, ROTATE_HANDLE_PX / viewRef.current.scale)
        : null;
      if (selectedShape && handle) {
        transformRef.current = { pointerId: event.pointerId, handle, shape: selectedShape, preview: null };
        return;
      }
      const hit = hitTestShapes(shapesRef.current, world, tolerance);
      if (hit && additive) {
        setSelectedIds((current) => toggleSelection(current, hit.id));
        return;
      }
      if (hit) {
        const ids = selectedIdsRef.current.includes(hit.id) ? selectedIdsRef.current : [hit.id];
        setSelectedIds(ids);
        moveRef.current = { pointerId: event.pointerId, origin: world, ids, dx: 0, dy: 0 };
        return;
      }
      const base = additive ? selectedIdsRef.current : [];
      if (!additive) setSelectedIds([]);
      marqueeRef.current = { pointerId: event.pointerId, start: world, end: world, base };
      return;
    }
    if (tool === "text") {
      event.preventDefault();
      const hit = hitTestShapes(shapesRef.current, world, tolerance);
      if (hit?.tool === "text") editText(hit);
      else setEditor({ id: null, anchor: world, value: "", color: resolveColor(), width: width / viewRef.current.scale });
      return;
    }
    draftRef.current = {
      id: newShapeId(),
      tool,
      color: tool === "eraser" ? "#000000" : resolveColor(),
      width: width / viewRef.current.scale,
      points: isFreehandTool(tool) ? [pointerWorld(event, point)] : [world, world],
    };
    draw();
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const point = localPoint(event);
    const pan = panRef.current;
    if (pan && pan.pointerId === event.pointerId) {
      const dx = point.x - pan.last.x;
      const dy = point.y - pan.last.y;
      pan.last = point;
      setView((current) => ({ ...current, x: current.x + dx, y: current.y + dy }));
      return;
    }
    const transform = transformRef.current;
    if (transform && transform.pointerId === event.pointerId) {
      const world = screenToWorld(viewRef.current, point);
      transform.preview = transform.handle === "rotate"
        ? rotateShapeToward(transform.shape, world, event.shiftKey)
        : resizeShape(transform.shape, transform.handle, world, event.shiftKey);
      draw();
      return;
    }
    const marquee = marqueeRef.current;
    if (marquee && marquee.pointerId === event.pointerId) {
      marquee.end = screenToWorld(viewRef.current, point);
      draw();
      return;
    }
    if (tool === "select" && !moveRef.current && !draftRef.current) {
      const selectedShape = singleSelected();
      const world = screenToWorld(viewRef.current, point);
      const handle = selectedShape
        ? hitTestHandle(selectedShape, world, HANDLE_HIT_PX / viewRef.current.scale, ROTATE_HANDLE_PX / viewRef.current.scale)
        : null;
      if (handle !== hoverHandle) setHoverHandle(handle);
    }
    const move = moveRef.current;
    if (move && move.pointerId === event.pointerId) {
      const world = screenToWorld(viewRef.current, point);
      move.dx = world.x - move.origin.x;
      move.dy = world.y - move.origin.y;
      draw();
      return;
    }
    const draft = draftRef.current;
    if (!draft) return;
    if (isFreehandTool(draft.tool)) {
      const rect = event.currentTarget.getBoundingClientRect();
      const samples = event.nativeEvent.getCoalescedEvents?.() ?? [];
      const added = samples.length > 0
        ? samples.map((sample) => {
          const world = screenToWorld(viewRef.current, { x: sample.clientX - rect.left, y: sample.clientY - rect.top });
          return sample.pointerType === "pen" ? { ...world, p: sample.pressure } : world;
        })
        : [pointerWorld(event, point)];
      draftRef.current = { ...draft, points: [...draft.points, ...added] };
    } else {
      draftRef.current = { ...draft, points: [draft.points[0], screenToWorld(viewRef.current, point)] };
    }
    draw();
  };

  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    if (panRef.current?.pointerId === event.pointerId) {
      panRef.current = null;
      return;
    }
    const transform = transformRef.current;
    if (transform?.pointerId === event.pointerId) {
      transformRef.current = null;
      const changed = transform.preview;
      if (changed) commit(shapesRef.current.map((shape) => (shape.id === changed.id ? changed : shape)));
      else draw();
      return;
    }
    const marquee = marqueeRef.current;
    if (marquee?.pointerId === event.pointerId) {
      marqueeRef.current = null;
      const hits = marqueeHasArea(marquee, viewRef.current.scale) ? shapesInMarquee(shapesRef.current, normalizeBounds(marquee.start, marquee.end)) : [];
      setSelectedIds(mergeSelection(marquee.base, hits));
      draw();
      return;
    }
    const move = moveRef.current;
    if (move?.pointerId === event.pointerId) {
      moveRef.current = null;
      if (move.dx !== 0 || move.dy !== 0) {
        commit(shapesRef.current.map((shape) => (move.ids.includes(shape.id) ? translateShape(shape, move.dx, move.dy) : shape)));
      } else {
        draw();
      }
      return;
    }
    const draft = draftRef.current;
    draftRef.current = null;
    if (draft) commit([...shapesRef.current, draft]);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const mod = event.ctrlKey || event.metaKey;
    if (mod && event.key.toLowerCase() === "z") {
      event.preventDefault();
      if (event.shiftKey) redo();
      else undo();
      return;
    }
    if (mod && event.key.toLowerCase() === "y") {
      event.preventDefault();
      redo();
      return;
    }
    if (event.key === " ") {
      event.preventDefault();
      setSpaceHeld(true);
      return;
    }
    if ((event.key === "Delete" || event.key === "Backspace") && selectedIds.length > 0) {
      event.preventDefault();
      deleteSelected();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      if (selectedIds.length > 0) setSelectedIds([]);
      else onClose();
      return;
    }
    if (mod || event.altKey) return;
    const match = SKETCH_TOOLS.find((entry) => entry.shortcut.toLowerCase() === event.key.toLowerCase());
    if (match) chooseTool(match.tool);
  };

  const panning = tool === "pan" || spaceHeld;
  const surfaceCursor = panning ? "is-panning" : tool === "select" ? "is-selecting" : tool === "text" ? "is-text" : "";

  return (
    <div
      ref={(node) => {
        resize.panelRef.current = node;
        if (typeof ref === "function") ref(node);
        else if (ref) ref.current = node;
      }}
      className="zorai-sketchpad"
      style={{ height: resize.height }}
    >
      <div className="zorai-composer-panel-resize" {...resize.handleProps} />
      <div className="zorai-sketchpad__toolbar" role="toolbar" aria-label="Sketch tools">
        <div className="zorai-sketchpad__group">
          {SKETCH_TOOLS.map((entry) => (
            <button
              key={entry.tool}
              type="button"
              className={["zorai-sketchpad__button", tool === entry.tool ? "is-active" : ""].filter(Boolean).join(" ")}
              title={`${entry.label} (${entry.shortcut})`}
              aria-label={entry.label}
              aria-pressed={tool === entry.tool}
              onClick={() => chooseTool(entry.tool)}
            >
              <SketchToolIcon tool={entry.tool} />
            </button>
          ))}
        </div>
        <div className="zorai-sketchpad__group">
          {SKETCH_COLORS.map((entry) => (
            <button
              key={entry}
              type="button"
              className={["zorai-sketchpad__swatch", color === entry ? "is-active" : "", entry === "ink" ? "is-ink" : ""].filter(Boolean).join(" ")}
              style={entry === "ink" ? undefined : { background: entry }}
              title={entry === "ink" ? "Ink" : entry}
              aria-label={entry === "ink" ? "Ink colour" : `Colour ${entry}`}
              aria-pressed={color === entry}
              onClick={() => setColor(entry)}
            />
          ))}
        </div>
        <div className="zorai-sketchpad__group">
          {SKETCH_WIDTHS.map((entry) => (
            <button
              key={entry}
              type="button"
              className={["zorai-sketchpad__button", width === entry ? "is-active" : ""].filter(Boolean).join(" ")}
              title={`Stroke ${entry}px`}
              aria-label={`Stroke ${entry}px`}
              aria-pressed={width === entry}
              onClick={() => setWidth(entry)}
            >
              <span className="zorai-sketchpad__width" style={{ width: entry + 2, height: entry + 2 }} />
            </button>
          ))}
        </div>
        <div className="zorai-sketchpad__group">
          <input
            ref={imageInputRef}
            type="file"
            accept="image/*"
            multiple
            hidden
            onChange={(event) => {
              void insertImages(Array.from(event.target.files ?? []));
              event.target.value = "";
            }}
          />
          <button type="button" className="zorai-sketchpad__button" title="Insert image (or paste one with Ctrl+V)" aria-label="Insert image" onClick={() => imageInputRef.current?.click()}>
            <SketchActionIcon kind="image" />
          </button>
        </div>
        <div className="zorai-sketchpad__group">
          <button type="button" className="zorai-sketchpad__button" title="Undo (Ctrl+Z)" aria-label="Undo" disabled={past.length === 0} onClick={undo}>
            <SketchActionIcon kind="undo" />
          </button>
          <button type="button" className="zorai-sketchpad__button" title="Redo (Ctrl+Shift+Z)" aria-label="Redo" disabled={future.length === 0} onClick={redo}>
            <SketchActionIcon kind="redo" />
          </button>
          <button type="button" className="zorai-sketchpad__button" title="Fit drawing" aria-label="Fit drawing" onClick={fitView}>
            <SketchActionIcon kind="fit" />
          </button>
          <button type="button" className="zorai-sketchpad__button" title="Clear" aria-label="Clear sketch" disabled={shapes.length === 0} onClick={() => commit([])}>
            <SketchActionIcon kind="clear" />
          </button>
          <button
            type="button"
            className="zorai-sketchpad__button"
            title="Download as PNG"
            aria-label="Download as PNG"
            disabled={exporting || (!background && !sketchHasInk(shapes))}
            onClick={() => void downloadPng()}
          >
            <SketchActionIcon kind="download" />
          </button>
        </div>
        <span className="zorai-sketchpad__zoom">{Math.round(view.scale * 100)}%</span>
        <div className="zorai-sketchpad__actions">
          <button
            type="button"
            className="zorai-ghost-button zorai-ghost-button--compact"
            title="Discard changes since the last save (Esc)"
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="button"
            className="zorai-ghost-button zorai-ghost-button--compact"
            title="Keep this drawing for later without attaching it"
            disabled={busy}
            onClick={() => {
              commitEditor();
              onSaveDraft();
            }}
          >
            Save draft
          </button>
          <button
            type="button"
            className="zorai-ghost-button zorai-ghost-button--compact zorai-sketchpad__primary"
            title={primaryTitle}
            disabled={busy || primaryDisabled}
            onClick={() => {
              commitEditor();
              onPrimary();
            }}
          >
            {busy ? "Saving…" : primaryLabel}
          </button>
        </div>
      </div>
      <div
        ref={surfaceRef}
        className={["zorai-sketchpad__surface", surfaceCursor, dropActive ? "is-drop-target" : ""].filter(Boolean).join(" ")}
        tabIndex={0}
        aria-label="Sketch canvas"
        style={{
          cursor: tool === "select" && hoverHandle && !panning ? HANDLE_CURSORS[hoverHandle] : undefined,
          backgroundSize: `${20 * view.scale}px ${20 * view.scale}px`,
          backgroundPosition: `${view.x}px ${view.y}px`,
        }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onKeyDown={onKeyDown}
        onDragOver={(event) => {
          if (!draggingFiles(event)) return;
          event.preventDefault();
          event.stopPropagation();
          event.dataTransfer.dropEffect = "copy";
          if (!dropActive) setDropActive(true);
        }}
        onDragLeave={(event) => {
          event.stopPropagation();
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropActive(false);
        }}
        onDrop={(event) => {
          if (!draggingFiles(event)) return;
          event.preventDefault();
          event.stopPropagation();
          setDropActive(false);
          const rect = event.currentTarget.getBoundingClientRect();
          void insertImages(Array.from(event.dataTransfer.files), { x: event.clientX - rect.left, y: event.clientY - rect.top });
        }}
        onKeyUp={(event) => {
          if (event.key === " ") setSpaceHeld(false);
        }}
        onBlur={() => setSpaceHeld(false)}
        onDoubleClick={(event) => {
          if (tool !== "select") return;
          const world = screenToWorld(viewRef.current, localPoint(event as unknown as PointerEvent<HTMLDivElement>));
          const hit = hitTestShapes(shapesRef.current, world, 6 / viewRef.current.scale);
          if (hit?.tool === "text") editText(hit);
        }}
      >
        {background ? (
          <img
            className="zorai-sketchpad__background"
            src={background.src}
            alt=""
            draggable={false}
            style={{
              width: background.width,
              height: background.height,
              transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
            }}
          />
        ) : null}
        <canvas ref={canvasRef} />
        {editor ? (
          <textarea
            ref={textInputRef}
            className="zorai-sketchpad__text-input"
            value={editor.value}
            rows={Math.max(1, sketchTextLines(editor.value).length)}
            spellCheck={false}
            aria-label="Sketch text"
            style={{
              left: worldToScreen(view, editor.anchor).x,
              top: worldToScreen(view, editor.anchor).y,
              color: editor.color,
              fontSize: sketchFontSize(editor.width) * view.scale,
              lineHeight: SKETCH_TEXT_LINE_HEIGHT,
              fontFamily: SKETCH_FONT_FAMILY,
            }}
            onChange={(event) => setEditor({ ...editor, value: event.target.value })}
            onPointerDown={(event) => event.stopPropagation()}
            onDoubleClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => {
              event.stopPropagation();
              if (event.key === "Escape") {
                event.preventDefault();
                cancelEditor();
              } else if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                commitEditor();
                surfaceRef.current?.focus();
              }
            }}
            onBlur={commitEditor}
          />
        ) : null}
        {shapes.length === 0 && background ? (
          <span className="zorai-sketchpad__hint">Draw on the image · Save to image replaces the attachment · Esc cancels</span>
        ) : shapes.length === 0 ? (
          <span className="zorai-sketchpad__hint">Draw here · T for text, V to select (drag a box, Ctrl+click to add) · Ctrl+C / Ctrl+V copies shapes or pastes images · Space or middle-drag to pan · Ctrl+scroll to zoom · Save & attach adds it as a PNG</span>
        ) : null}
      </div>
    </div>
  );
}

const iconProps = {
  width: 16,
  height: 16,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  "aria-hidden": true,
};

function SketchToolIcon({ tool }: { tool: SketchTool }) {
  if (tool === "select") return <svg {...iconProps}><path d="M4 3l7 17 2.5-7.5L21 10 4 3z" /></svg>;
  if (tool === "text") return <svg {...iconProps}><path d="M5 6V4h14v2" /><line x1="12" y1="4" x2="12" y2="20" /><line x1="9" y1="20" x2="15" y2="20" /></svg>;
  if (tool === "pen") return <svg {...iconProps}><path d="M12 20h9" /><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" /></svg>;
  if (tool === "highlighter") return <svg {...iconProps}><path d="m9 11-6 6v3h9l3-3" /><path d="m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4" /></svg>;
  if (tool === "line") return <svg {...iconProps}><line x1="5" y1="19" x2="19" y2="5" /></svg>;
  if (tool === "arrow") return <svg {...iconProps}><line x1="5" y1="19" x2="19" y2="5" /><polyline points="9 5 19 5 19 15" /></svg>;
  if (tool === "rect") return <svg {...iconProps}><rect x="4" y="6" width="16" height="12" rx="1" /></svg>;
  if (tool === "ellipse") return <svg {...iconProps}><ellipse cx="12" cy="12" rx="9" ry="6" /></svg>;
  if (tool === "eraser") return <svg {...iconProps}><path d="m7 21-4.3-4.3a1 1 0 0 1 0-1.4l10-10a1 1 0 0 1 1.4 0l5.6 5.6a1 1 0 0 1 0 1.4L13 21" /><path d="M22 21H7" /><path d="m5 11 9 9" /></svg>;
  return <svg {...iconProps}><path d="M18 11V6a2 2 0 0 0-4 0v5" /><path d="M14 10V4a2 2 0 0 0-4 0v6" /><path d="M10 10.5V6a2 2 0 0 0-4 0v8" /><path d="M18 8a2 2 0 1 1 4 0v6a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15" /></svg>;
}

function SketchActionIcon({ kind }: { kind: "undo" | "redo" | "fit" | "clear" | "image" | "download" }) {
  if (kind === "download") return <svg {...iconProps}><path d="M12 4v11" /><path d="m7 10 5 5 5-5" /><path d="M5 20h14" /></svg>;
  if (kind === "image") return <svg {...iconProps}><rect x="3" y="4" width="18" height="16" rx="2" /><circle cx="9" cy="10" r="2" /><path d="m21 16-5-5L5 20" /></svg>;
  if (kind === "undo") return <svg {...iconProps}><path d="M9 14 4 9l5-5" /><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11" /></svg>;
  if (kind === "redo") return <svg {...iconProps}><path d="m15 14 5-5-5-5" /><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13" /></svg>;
  if (kind === "fit") return <svg {...iconProps}><path d="M3 8V5a2 2 0 0 1 2-2h3" /><path d="M16 3h3a2 2 0 0 1 2 2v3" /><path d="M21 16v3a2 2 0 0 1-2 2h-3" /><path d="M8 21H5a2 2 0 0 1-2-2v-3" /></svg>;
  return <svg {...iconProps}><path d="M3 6h18" /><path d="M8 6V4h8v2" /><path d="M19 6l-1 14H6L5 6" /></svg>;
}
