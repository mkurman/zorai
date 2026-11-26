export function footerModelLabel(model: string): string {
  const name = model.split("/").pop() ?? model;
  return name.length > 28 ? `${name.slice(0, 27)}…` : name || "Choose model";
}

export function effortLabel(effort: string): string {
  return effort === "xhigh" ? "Extra high" : effort ? effort.charAt(0).toUpperCase() + effort.slice(1) : "None";
}
