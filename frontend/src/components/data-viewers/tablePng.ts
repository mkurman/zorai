import { createContext } from "react";

const CANVAS_WIDTH = 1200;
const MIN_CANVAS_HEIGHT = 628;
const MARGIN = 48;
const CARD_PAD = 20;
const CARD_RADIUS = 16;
const ROW_RADIUS = 8;
const PAD_X = 14;
const PAD_Y = 10;
const LINE_HEIGHT = 20;
const SCALE = 2;
const FONT_SIZE = 15;

export const MessageExportTimeContext = createContext<number | null>(null);

export function timestampedFileName(prefix: string, time: number | null): string {
  const date = new Date(time ?? Date.now());
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${prefix}-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}-${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`;
}

type Measure = (text: string, header: boolean) => number;

export type TableLayout = {
  widths: number[];
  rows: Array<{ cells: string[][]; height: number }>;
  width: number;
  height: number;
};

export function fitColumnWidths(natural: number[], available: number): number[] {
  const widths = natural.map(() => 0);
  let open = natural.map((_, index) => index);
  let remaining = available;
  while (open.length > 0) {
    const share = remaining / open.length;
    const fitting = open.filter((index) => natural[index] <= share);
    if (fitting.length === 0) {
      open.forEach((index) => { widths[index] = Math.floor(share); });
      break;
    }
    fitting.forEach((index) => {
      widths[index] = natural[index];
      remaining -= natural[index];
    });
    open = open.filter((index) => natural[index] > share);
  }
  return widths;
}

export function wrapText(text: string, maxWidth: number, measure: (text: string) => number): string[] {
  const lines: string[] = [];
  let line = "";
  for (const word of text.split(/\s+/).filter(Boolean)) {
    const candidate = line ? `${line} ${word}` : word;
    if (measure(candidate) <= maxWidth) {
      line = candidate;
      continue;
    }
    if (line) lines.push(line);
    line = "";
    for (const char of word) {
      if (line && measure(line + char) > maxWidth) {
        lines.push(line);
        line = "";
      }
      line += char;
    }
  }
  if (line || lines.length === 0) lines.push(line);
  return lines;
}

export function layoutTable(columns: string[], rows: string[][], measure: Measure): TableLayout {
  const natural = columns.map((column, index) =>
    Math.ceil(Math.max(measure(column, true), ...rows.map((row) => measure(row[index] ?? "", false)))) + PAD_X * 2,
  );
  const widths = fitColumnWidths(natural, CANVAS_WIDTH - MARGIN * 2 - CARD_PAD * 2);
  const laidOut = [columns, ...rows].map((cells, rowIndex) => {
    const wrapped = widths.map((width, index) => wrapText(cells[index] ?? "", width - PAD_X * 2, (text) => measure(text, rowIndex === 0)));
    return { cells: wrapped, height: Math.max(...wrapped.map((lines) => lines.length)) * LINE_HEIGHT + PAD_Y * 2 };
  });
  const tableHeight = laidOut.reduce((sum, row) => sum + row.height, 0);
  return {
    widths,
    rows: laidOut,
    width: CANVAS_WIDTH,
    height: Math.max(MIN_CANVAS_HEIGHT, tableHeight + MARGIN * 2 + CARD_PAD * 2),
  };
}

export function isNumericCell(text: string): boolean {
  return /^[-+−~≈]?\s*[$€£]?\d[\d,.\s]*(?:[%×x]|\s?[a-zA-Zµ]{1,4})?$/.test(text.trim());
}

function roundedRect(context: CanvasRenderingContext2D, x: number, y: number, width: number, height: number, radius: number) {
  context.beginPath();
  context.roundRect(x, y, width, height, radius);
}

export function opaqueColor(color: string): string {
  const rgba = /^rgba\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(color.trim());
  return rgba ? `rgb(${rgba[1]}, ${rgba[2]}, ${rgba[3]})` : color;
}

export function exportTablePng(columns: string[], rows: string[][], fileName: string) {
  const style = getComputedStyle(document.documentElement);
  const token = (name: string, fallback: string) => opaqueColor(style.getPropertyValue(name).trim() || fallback);
  const family = style.getPropertyValue("--font-ui").trim() || "sans-serif";
  const font = (header: boolean) => `${header ? 600 : 400} ${FONT_SIZE}px ${family}`;
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  if (!context) return;
  const layout = layoutTable(columns, rows, (text, header) => {
    context.font = font(header);
    return context.measureText(text).width;
  });
  const tableWidth = layout.widths.reduce((sum, value) => sum + value, 0);
  const tableHeight = layout.rows.reduce((sum, row) => sum + row.height, 0);
  const cardWidth = tableWidth + CARD_PAD * 2;
  const cardHeight = tableHeight + CARD_PAD * 2;
  const cardX = Math.round((layout.width - cardWidth) / 2);
  const cardY = Math.round((layout.height - cardHeight) / 2);
  const originX = cardX + CARD_PAD;
  const originY = cardY + CARD_PAD;
  const accent = token("--accent", "#d9a548");
  canvas.width = layout.width * SCALE;
  canvas.height = layout.height * SCALE;
  context.scale(SCALE, SCALE);

  context.fillStyle = token("--bg-primary", "#111318");
  context.fillRect(0, 0, layout.width, layout.height);
  const glow = context.createRadialGradient(layout.width * 0.15, 0, 0, layout.width * 0.15, 0, layout.width * 0.9);
  glow.addColorStop(0, accent);
  glow.addColorStop(1, "transparent");
  context.globalAlpha = 0.14;
  context.fillStyle = glow;
  context.fillRect(0, 0, layout.width, layout.height);
  context.globalAlpha = 1;

  context.save();
  context.shadowColor = "rgba(0, 0, 0, 0.35)";
  context.shadowBlur = 40;
  context.shadowOffsetY = 12;
  roundedRect(context, cardX, cardY, cardWidth, cardHeight, CARD_RADIUS);
  context.fillStyle = token("--bg-tertiary", "#1c1f26");
  context.fill();
  context.restore();
  roundedRect(context, cardX + 0.5, cardY + 0.5, cardWidth - 1, cardHeight - 1, CARD_RADIUS);
  context.strokeStyle = token("--border-strong", "#2a2f38");
  context.lineWidth = 1;
  context.stroke();

  context.textBaseline = "top";
  let top = originY;
  layout.rows.forEach((row, rowIndex) => {
    const header = rowIndex === 0;
    if (header || rowIndex % 2 === 0) {
      roundedRect(context, originX, top, tableWidth, row.height, ROW_RADIUS);
      context.globalAlpha = header ? 0.16 : 0.045;
      context.fillStyle = header ? accent : token("--text-primary", "#ffffff");
      context.fill();
      context.globalAlpha = 1;
    }
    context.font = font(header);
    context.fillStyle = header ? token("--text-primary", "#f2f2f2") : token("--text-secondary", "#c8c8c8");
    let left = originX;
    row.cells.forEach((lines, columnIndex) => {
      const width = layout.widths[columnIndex];
      const alignRight = !header && lines.length === 1 && isNumericCell(lines[0]);
      context.textAlign = alignRight ? "right" : "left";
      lines.forEach((line, lineIndex) => {
        const y = top + PAD_Y + lineIndex * LINE_HEIGHT + (LINE_HEIGHT - FONT_SIZE) / 2;
        context.fillText(line, alignRight ? left + width - PAD_X : left + PAD_X, y);
      });
      left += width;
    });
    top += row.height;
  });
  context.textAlign = "left";
  canvas.toBlob((blob) => {
    if (!blob) return;
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = fileName;
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  }, "image/png");
}
