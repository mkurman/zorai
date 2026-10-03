export type StatisticsSessionPage = {
  sessions: ZoraiSessionStatisticsRow[];
  session_total: number;
  session_limit: number;
  session_offset: number;
};

export function mergeStatisticsSessionPage<T extends StatisticsSessionPage>(
  current: T,
  page: StatisticsSessionPage,
): T {
  return {
    ...current,
    sessions: page.sessions,
    session_total: page.session_total,
    session_limit: page.session_limit,
    session_offset: page.session_offset,
  };
}

export function statisticsSessionPageMatches(
  page: { session_offset?: number; session_limit?: number },
  offset: number,
  limit: number,
): boolean {
  return (page.session_offset ?? 0) === offset && (page.session_limit ?? limit) === limit;
}
