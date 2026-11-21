import { createContext } from "react";

export type MathRendering = { segments: string[]; render: (root: HTMLElement) => void };

export const MathSegmentsContext = createContext<MathRendering>({ segments: [], render: () => {} });

const SUBSCRIPTS: Record<string, string> = { 0: "₀", 1: "₁", 2: "₂", 3: "₃", 4: "₄", 5: "₅", 6: "₆", 7: "₇", 8: "₈", 9: "₉", "+": "₊", "-": "₋" };
const SUPERSCRIPTS: Record<string, string> = { 0: "⁰", 1: "¹", 2: "²", 3: "³", 4: "⁴", 5: "⁵", 6: "⁶", 7: "⁷", 8: "⁸", 9: "⁹", "+": "⁺", "-": "⁻" };

function scripted(text: string, map: Record<string, string>, marker: string): string {
  return [...text].every((char) => map[char]) ? [...text].map((char) => map[char]).join("") : `${marker}${text}`;
}

export function readableMath(segment: string): string {
  return segment
    .replace(/^\$\$|\$\$$|^\$|\$$|^\\\(|\\\)$|^\\\[|\\\]$/g, "")
    .replace(/_\{([^}]*)\}|_(\w)/g, (_, group, single) => scripted(group ?? single, SUBSCRIPTS, "_"))
    .replace(/\^\{([^}]*)\}|\^(\w)/g, (_, group, single) => scripted(group ?? single, SUPERSCRIPTS, "^"))
    .replace(/\\(?:mathrm|text|mathbf|operatorname)\{([^}]*)\}/g, "$1")
    .replace(/\\pm/g, "±")
    .replace(/\\times/g, "×")
    .replace(/\\to|\\rightarrow/g, "→")
    .replace(/\\le(?:q)?\b/g, "≤")
    .replace(/\\ge(?:q)?\b/g, "≥")
    .replace(/[{}]/g, "")
    .trim();
}

export function restoreMathText(text: string, segments: string[]): string {
  if (segments.length === 0) return text;
  return text.replace(/(\d+)/g, (token, index) => {
    const segment = segments[Number(index)];
    return segment === undefined ? token : readableMath(segment);
  });
}
