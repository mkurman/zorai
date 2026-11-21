import { createContext } from "react";

const CANVAS_WIDTH = 1200;
const MIN_CANVAS_HEIGHT = 628;
const MARGIN = 48;
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
  const widths = fitColumnWidths(natural, CANVAS_WIDTH - MARGIN * 2);
  const laidOut = [columns, ...rows].map((cells, rowIndex) => {
    const wrapped = widths.map((width, index) => wrapText(cells[index] ?? "", width - PAD_X * 2, (text) => measure(text, rowIndex === 0)));
    return { cells: wrapped, height: Math.max(...wrapped.map((lines) => lines.length)) * LINE_HEIGHT + PAD_Y * 2 };
  });
  const tableHeight = laidOut.reduce((sum, row) => sum + row.height, 0);
  return {
    widths,
    rows: laidOut,
    width: CANVAS_WIDTH,
    height: Math.max(MIN_CANVAS_HEIGHT, tableHeight + MARGIN * 2),
  };
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
  const originX = Math.round((layout.width - tableWidth) / 2);
  const originY = Math.round((layout.height - tableHeight) / 2);
  canvas.width = layout.width * SCALE;
  canvas.height = layout.height * SCALE;
  context.scale(SCALE, SCALE);
  context.fillStyle = token("--bg-primary", "#111318");
  context.fillRect(0, 0, layout.width, layout.height);
  context.fillStyle = token("--bg-secondary", "#1a1d24");
  context.fillRect(originX, originY, tableWidth, layout.rows[0]?.height ?? 0);
  context.strokeStyle = token("--border-strong", "#2a2f38");
  context.lineWidth = 1;
  context.textBaseline = "top";
  let top = originY;
  layout.rows.forEach((row, rowIndex) => {
    if (rowIndex > 0) {
      context.beginPath();
      context.moveTo(originX, top + 0.5);
      context.lineTo(originX + tableWidth, top + 0.5);
      context.stroke();
    }
    context.font = font(rowIndex === 0);
    context.fillStyle = token(rowIndex === 0 ? "--text-primary" : "--text-secondary", "#d6d6d6");
    let left = originX;
    row.cells.forEach((lines, columnIndex) => {
      lines.forEach((line, lineIndex) => context.fillText(line, left + PAD_X, top + PAD_Y + lineIndex * LINE_HEIGHT + (LINE_HEIGHT - FONT_SIZE) / 2));
      left += layout.widths[columnIndex];
    });
    top += row.height;
  });
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
