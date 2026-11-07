import { useRef, useState, type Dispatch, type ReactNode, type SetStateAction } from "react";
import type { ComposerAttachment } from "@/components/agent-chat-panel/chat-view/types";
import { readComposerAttachment } from "@/components/agent-chat-panel/chat-view/composerMedia";
import { ComposerSketchpad } from "./ComposerSketchpad";
import { annotateImageToPngFile, sketchToPngFile } from "./sketchExport";
import { sketchHasInk, type SketchBackground, type SketchShape } from "./sketchpadModel";

type ComposerAnnotation = { background: SketchBackground; shapes: SketchShape[]; applied: boolean };

function annotationHasDraft(annotation: ComposerAnnotation | undefined): boolean {
  return Boolean(annotation && !annotation.applied && sketchHasInk(annotation.shapes));
}

export function useComposerSketch({
  attachments,
  setAttachments,
  onClosed,
}: {
  attachments: ComposerAttachment[];
  setAttachments: Dispatch<SetStateAction<ComposerAttachment[]>>;
  onClosed: () => void;
}): {
  pad: ReactNode;
  sketchOpen: boolean;
  hasDraft: boolean;
  annotating: boolean;
  toggleSketch: () => void;
  hideSketch: () => void;
  openAnnotation: (attachmentId: string) => void;
  tileBadge: (attachmentId: string) => string | undefined;
  forgetAttachment: (attachmentId: string) => void;
  forgetAll: () => void;
} {
  const [sketchOpen, setSketchOpen] = useState(false);
  const [sketchShapes, setSketchShapes] = useState<SketchShape[]>([]);
  const [sketchDraft, setSketchDraft] = useState<SketchShape[]>([]);
  const sketchRef = useRef<HTMLDivElement | null>(null);
  const [annotations, setAnnotations] = useState<Record<string, ComposerAnnotation>>({});
  const [annotating, setAnnotating] = useState<{ attachmentId: string; shapes: SketchShape[] } | null>(null);
  const [saving, setSaving] = useState(false);

  const openSketch = () => {
    setSketchShapes(sketchDraft);
    setSketchOpen(true);
  };

  const closeSketch = () => {
    setSketchOpen(false);
    onClosed();
  };

  const saveSketchDraft = () => {
    setSketchDraft(sketchShapes);
    closeSketch();
  };

  const attachSketch = async () => {
    setSaving(true);
    try {
      const background = sketchRef.current ? getComputedStyle(sketchRef.current).backgroundColor : "#ffffff";
      const file = await sketchToPngFile(sketchShapes, background);
      const sketch = file ? await readComposerAttachment(file) : null;
      if (!sketch) return;
      setAttachments((current) => [...current, sketch]);
      setSketchShapes([]);
      setSketchDraft([]);
      closeSketch();
    } finally {
      setSaving(false);
    }
  };

  const openAnnotation = (attachmentId: string) => {
    const existing = annotations[attachmentId];
    if (existing) {
      setSketchOpen(false);
      setAnnotating({ attachmentId, shapes: existing.shapes });
      return;
    }
    const attachment = attachments.find((item) => item.id === attachmentId);
    const src = attachment?.kind === "image" ? attachment.dataUrl : undefined;
    if (!src) return;
    const image = new Image();
    image.onload = () => {
      setSketchOpen(false);
      setAnnotations((current) => ({
        ...current,
        [attachmentId]: { background: { src, width: image.naturalWidth, height: image.naturalHeight }, shapes: [], applied: false },
      }));
      setAnnotating({ attachmentId, shapes: [] });
    };
    image.src = src;
  };

  const closeAnnotation = () => {
    setAnnotating(null);
    onClosed();
  };

  const saveAnnotationDraft = () => {
    if (!annotating) return;
    const record = annotations[annotating.attachmentId];
    if (record) {
      setAnnotations((current) => ({
        ...current,
        [annotating.attachmentId]: { ...record, shapes: annotating.shapes, applied: record.applied && record.shapes === annotating.shapes },
      }));
    }
    closeAnnotation();
  };

  const applyAnnotation = async () => {
    if (!annotating) return;
    const record = annotations[annotating.attachmentId];
    const original = attachments.find((item) => item.id === annotating.attachmentId);
    if (!record || !original) return;
    setSaving(true);
    try {
      const file = await annotateImageToPngFile(record.background, annotating.shapes, original.name.replace(/-annotated(?=\.png$)/, ""));
      const annotated = file ? await readComposerAttachment(file) : null;
      if (!annotated) return;
      setAttachments((current) => current.map((item) => (item.id === original.id ? annotated : item)));
      setAnnotations((current) => {
        const { [original.id]: _replaced, ...rest } = current;
        return { ...rest, [annotated.id]: { ...record, shapes: annotating.shapes, applied: true } };
      });
      closeAnnotation();
    } finally {
      setSaving(false);
    }
  };

  const annotationRecord = annotating ? annotations[annotating.attachmentId] : undefined;
  const pad = annotating && annotationRecord ? (
    <ComposerSketchpad
      key={`annotate:${annotating.attachmentId}`}
      shapes={annotating.shapes}
      onShapesChange={(shapes) => setAnnotating((current) => (current ? { ...current, shapes } : current))}
      background={annotationRecord.background}
      primaryLabel="Save to image"
      primaryTitle="Replace the attachment with the annotated image"
      primaryDisabled={!sketchHasInk(annotating.shapes) && !annotationRecord.applied}
      onPrimary={() => void applyAnnotation()}
      onSaveDraft={saveAnnotationDraft}
      busy={saving}
      onClose={closeAnnotation}
    />
  ) : sketchOpen ? (
    <ComposerSketchpad
      ref={sketchRef}
      shapes={sketchShapes}
      onShapesChange={setSketchShapes}
      primaryLabel="Save & attach"
      primaryTitle="Add this drawing to the message as a PNG"
      primaryDisabled={!sketchHasInk(sketchShapes)}
      onPrimary={() => void attachSketch()}
      onSaveDraft={saveSketchDraft}
      busy={saving}
      onClose={closeSketch}
    />
  ) : null;

  return {
    pad,
    sketchOpen,
    hasDraft: sketchHasInk(sketchDraft),
    annotating: Boolean(annotating),
    toggleSketch: () => (sketchOpen ? saveSketchDraft() : openSketch()),
    hideSketch: () => {
      if (sketchOpen) saveSketchDraft();
    },
    openAnnotation,
    tileBadge: (attachmentId) => (annotationHasDraft(annotations[attachmentId]) ? "draft" : undefined),
    forgetAttachment: (attachmentId) => {
      setAnnotations(({ [attachmentId]: _removed, ...rest }) => rest);
      if (annotating?.attachmentId === attachmentId) setAnnotating(null);
    },
    forgetAll: () => {
      setAnnotations({});
      setAnnotating(null);
    },
  };
}
