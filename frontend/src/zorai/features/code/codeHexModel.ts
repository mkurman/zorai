export const BYTES_PER_ROW = 16;

export function hexRow(bytes: Uint8Array, offset: number): { address: string; hex: string; ascii: string } {
  const slice = bytes.subarray(0, BYTES_PER_ROW);
  const hex = Array.from({ length: BYTES_PER_ROW }, (_, index) => (index < slice.length ? slice[index].toString(16).padStart(2, "0") : "  "));
  return {
    address: offset.toString(16).padStart(8, "0"),
    hex: `${hex.slice(0, 8).join(" ")}  ${hex.slice(8).join(" ")}`,
    ascii: Array.from(slice, (byte) => (byte >= 0x20 && byte < 0x7f ? String.fromCharCode(byte) : ".")).join(""),
  };
}
