export function codeTiming(label: string): void {
  console.info(`[code-timing] ${label} t=${Math.round(performance.now())}`);
}
