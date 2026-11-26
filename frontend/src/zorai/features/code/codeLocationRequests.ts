export type CodeLocationRequest = { path: string; line: number; column?: number };

const EVENT = "zorai-open-code-location";
let pending: CodeLocationRequest[] = [];

export function requestCodeLocation(request: CodeLocationRequest) {
  pending.push(request);
  window.dispatchEvent(new CustomEvent(EVENT));
}

export function subscribeCodeLocations(open: (request: CodeLocationRequest) => void): () => void {
  const drain = () => {
    const requests = pending;
    pending = [];
    requests.forEach(open);
  };
  window.addEventListener(EVENT, drain);
  drain();
  return () => window.removeEventListener(EVENT, drain);
}
