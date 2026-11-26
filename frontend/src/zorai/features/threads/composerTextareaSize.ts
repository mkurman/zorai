export const COMPOSER_TEXTAREA_MIN_ROWS = 1;
export const COMPOSER_TEXTAREA_MAX_ROWS = 10;

export function composerTextareaHeightPx(
  scrollHeight: number,
  lineHeight: number,
  paddingY: number,
): number {
  const minHeight = lineHeight * COMPOSER_TEXTAREA_MIN_ROWS + paddingY;
  const maxHeight = lineHeight * COMPOSER_TEXTAREA_MAX_ROWS + paddingY;
  return Math.min(maxHeight, Math.max(minHeight, scrollHeight));
}

export function applyComposerTextareaSize(el: HTMLTextAreaElement): void {
  const styles = window.getComputedStyle(el);
  const lineHeight = Number.parseFloat(styles.lineHeight);
  const paddingY = Number.parseFloat(styles.paddingTop) + Number.parseFloat(styles.paddingBottom);
  const safeLineHeight = Number.isFinite(lineHeight) && lineHeight > 0 ? lineHeight : 21;
  const safePadding = Number.isFinite(paddingY) ? paddingY : 18;
  el.style.height = "0px";
  const measurePlaceholder = el.value === "" && el.placeholder !== "";
  if (measurePlaceholder) el.value = el.placeholder;
  const scrollHeight = el.scrollHeight;
  if (measurePlaceholder) el.value = "";
  el.style.height = `${composerTextareaHeightPx(scrollHeight, safeLineHeight, safePadding)}px`;
}
