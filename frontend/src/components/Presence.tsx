import { useRef, type ReactNode } from "react";
import { usePresence } from "@/hooks/usePresence";

export function Presence<T>({ value, children }: { value: T | null | undefined | false; children: (value: T, closing: boolean) => ReactNode }) {
  const present = value !== null && value !== undefined && value !== false;
  const { closing } = usePresence(present);
  const lastRef = useRef<T | null>(null);
  if (present) lastRef.current = value as T;
  const shown = present ? (value as T) : closing ? lastRef.current : null;
  if (shown === null) return null;
  return (
    <div className="zorai-motion-exit" data-closing={closing || undefined}>
      {children(shown, closing)}
    </div>
  );
}
