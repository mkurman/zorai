import { useEffect, useRef } from "react";
import { useUserThemesStore } from "./userThemesStore";

export function ThemeBackdropVideo() {
  const video = useUserThemesStore((state) => state.backdropVideo);
  const ref = useRef<HTMLVideoElement | null>(null);

  useEffect(() => {
    const element = ref.current;
    if (!element || typeof window === "undefined") return;
    const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    let focused = document.hasFocus();
    const sync = (event?: Event) => {
      if (event?.type === "blur") focused = false;
      if (event?.type === "focus") focused = true;
      const shouldPlay = focused && document.visibilityState === "visible" && !reducedMotion?.matches;
      if (shouldPlay) void element.play().catch(() => undefined);
      else element.pause();
    };
    sync();
    window.addEventListener("focus", sync);
    window.addEventListener("blur", sync);
    document.addEventListener("visibilitychange", sync);
    reducedMotion?.addEventListener?.("change", sync);
    return () => {
      window.removeEventListener("focus", sync);
      window.removeEventListener("blur", sync);
      document.removeEventListener("visibilitychange", sync);
      reducedMotion?.removeEventListener?.("change", sync);
    };
  }, [video?.src]);

  if (!video) return null;
  return (
    <video
      ref={ref}
      className="zorai-backdrop-video"
      src={video.src}
      style={{
        filter: `blur(${video.blur}px)`,
        top: -2 * video.blur,
        left: -2 * video.blur,
        width: `calc(100vw + ${4 * video.blur}px)`,
        height: `calc(100vh + ${4 * video.blur}px)`,
      }}
      muted
      loop
      playsInline
      preload="auto"
      aria-hidden="true"
    />
  );
}
