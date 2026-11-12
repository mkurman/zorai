import { beforeEach, describe, expect, it, vi } from "vitest";
import { trackPointerDrag } from "./pointerDrag";

function pointer(type: string, init: { clientX?: number; buttons?: number; pointerId?: number }) {
  const event = Object.assign(new Event(type), { clientX: init.clientX ?? 0, buttons: init.buttons ?? 0, pointerId: init.pointerId ?? 1 });
  window.dispatchEvent(event);
}

describe("trackPointerDrag", () => {
  beforeEach(() => vi.stubGlobal("window", new EventTarget()));

  it("follows moves whose pointer id differs from the press, as XWayland sends them, so panels resize on Ubuntu", () => {
    const onMove = vi.fn();
    trackPointerDrag((event) => onMove(event.clientX));
    pointer("pointermove", { clientX: 40, buttons: 1, pointerId: 7 });
    pointer("pointermove", { clientX: 60, buttons: 1, pointerId: 9 });
    pointer("pointerup", { pointerId: 1 });
    pointer("pointermove", { clientX: 80, buttons: 1 });
    expect(onMove.mock.calls.map(([x]) => x)).toEqual([40, 60]);
  });

  it("ends a drag whose release was missed once a move arrives with no button held", () => {
    const onMove = vi.fn();
    const onEnd = vi.fn();
    trackPointerDrag(onMove, onEnd);
    pointer("pointermove", { clientX: 10, buttons: 0 });
    pointer("pointermove", { clientX: 20, buttons: 1 });
    expect(onMove).not.toHaveBeenCalled();
    expect(onEnd).toHaveBeenCalledTimes(1);
  });
});
