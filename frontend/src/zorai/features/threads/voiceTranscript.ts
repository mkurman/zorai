const BAR_COUNT = 28;

export function joinVoiceTranscript(base: string, transcript: string): string {
  const spoken = transcript.trim();
  if (!spoken) return base;
  return base.trim() ? `${base.trimEnd()} ${spoken}` : spoken;
}

export function waveformBarLevels(samples: Uint8Array, barCount = BAR_COUNT): number[] {
  const perBar = Math.max(1, Math.floor(samples.length / barCount));
  return Array.from({ length: barCount }, (_, bar) => {
    let peak = 0;
    for (let index = bar * perBar; index < Math.min(samples.length, (bar + 1) * perBar); index += 1) {
      peak = Math.max(peak, Math.abs(samples[index] - 128) / 128);
    }
    return Math.min(1, peak * 2.5);
  });
}
