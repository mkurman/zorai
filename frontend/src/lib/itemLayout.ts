export type ItemLayout = { order: string[]; hidden: string[] };

export const EMPTY_ITEM_LAYOUT: ItemLayout = { order: [], hidden: [] };

export function orderedItemIds<T extends string>(defaults: readonly T[], layout: ItemLayout | undefined): T[] {
  const known = new Set<string>(defaults);
  const saved = (layout?.order ?? []).filter((id): id is T => known.has(id));
  const ordered = [...new Set(saved), ...defaults.filter((id) => !saved.includes(id))];
  return ordered;
}

export function visibleItemIds<T extends string>(defaults: readonly T[], layout: ItemLayout | undefined): T[] {
  const hidden = new Set(layout?.hidden ?? []);
  return orderedItemIds(defaults, layout).filter((id) => !hidden.has(id));
}

export function moveItem<T extends string>(defaults: readonly T[], layout: ItemLayout | undefined, id: T, offset: -1 | 1): ItemLayout {
  const order = orderedItemIds(defaults, layout);
  const index = order.indexOf(id);
  const target = index + offset;
  if (index < 0 || target < 0 || target >= order.length) return { order, hidden: layout?.hidden ?? [] };
  [order[index], order[target]] = [order[target], order[index]];
  return { order, hidden: layout?.hidden ?? [] };
}

export function setItemHidden(layout: ItemLayout | undefined, id: string, hidden: boolean): ItemLayout {
  const current = new Set(layout?.hidden ?? []);
  if (hidden) current.add(id);
  else current.delete(id);
  return { order: layout?.order ?? [], hidden: [...current] };
}
