import { useEffect, useRef } from "react";
import { waveformBarLevels } from "./voiceTranscript";

const BAR_COUNT = 28;

export function ComposerVoiceWaveform({ stream }: { stream: MediaStream }) {
  const barsRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const AudioContextCtor = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AudioContextCtor) return;
    const context = new AudioContextCtor();
    const analyser = context.createAnalyser();
    analyser.fftSize = 1024;
    const source = context.createMediaStreamSource(stream);
    source.connect(analyser);
    const samples = new Uint8Array(analyser.fftSize);
    let frame = 0;
    const draw = () => {
      analyser.getByteTimeDomainData(samples);
      const bars = barsRef.current?.children;
      if (bars) {
        waveformBarLevels(samples, bars.length).forEach((level, index) => {
          (bars[index] as HTMLElement).style.transform = `scaleY(${Math.max(0.12, level)})`;
        });
      }
      frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(frame);
      source.disconnect();
      void context.close();
    };
  }, [stream]);

  return (
    <div ref={barsRef} className="zorai-composer-waveform" aria-hidden="true">
      {Array.from({ length: BAR_COUNT }, (_, index) => <span key={index} />)}
    </div>
  );
}
