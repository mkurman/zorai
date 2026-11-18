function parseSvgElement(svg: string): SVGSVGElement | null {
  return new DOMParser().parseFromString(svg, "text/html").querySelector("svg");
}

export function svgViewBoxSize(svg: string): { width: number; height: number } | null {
  const viewBox = parseSvgElement(svg)?.getAttribute("viewBox")?.trim().split(/[\s,]+/).map(Number);
  if (!viewBox || viewBox.length !== 4 || !(viewBox[2] > 0) || !(viewBox[3] > 0)) return null;
  return { width: viewBox[2], height: viewBox[3] };
}

export function standaloneSvg(svg: string): string {
  const root = parseSvgElement(svg);
  return root ? new XMLSerializer().serializeToString(root) : svg;
}

export function svgWithIntrinsicSize(svg: string): string {
  const root = parseSvgElement(svg);
  const size = svgViewBoxSize(svg);
  if (!root || !size) return standaloneSvg(svg);
  root.setAttribute("width", String(size.width));
  root.setAttribute("height", String(size.height));
  root.style.removeProperty("max-width");
  return new XMLSerializer().serializeToString(root);
}
