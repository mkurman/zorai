const DAY_MS = 24 * 60 * 60 * 1000;

export function threadDateGroupLabel(updatedAt: number, now: number): string {
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const today = startOfToday.getTime();
  if (updatedAt >= today) return "Today";
  if (updatedAt >= today - DAY_MS) return "Yesterday";
  if (updatedAt >= today - 7 * DAY_MS) return "Previous 7 days";
  if (updatedAt >= today - 30 * DAY_MS) return "Previous 30 days";
  return "Older";
}
