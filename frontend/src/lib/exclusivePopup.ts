import { useEffect, useRef } from "react";

export type ExclusivePopupGroup = "menu" | "panel";

const current: Partial<Record<ExclusivePopupGroup, { token: symbol; close: () => void }>> = {};

export function claimExclusivePopup(close: () => void, group: ExclusivePopupGroup = "menu"): () => void {
  const token = Symbol(group);
  const previous = current[group];
  current[group] = { token, close };
  if (previous) previous.close();
  return () => {
    if (current[group]?.token === token) delete current[group];
  };
}

export function useExclusivePopup(onClose: () => void, group: ExclusivePopupGroup = "menu"): void {
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  useEffect(() => claimExclusivePopup(() => onCloseRef.current(), group), [group]);
}
