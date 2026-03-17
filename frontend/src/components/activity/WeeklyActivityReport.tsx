import { useMemo, useState, useCallback } from 'react';
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';
import { useWeeklyActivity } from '@/hooks/use-weekly-activity';
import { formatDate, getWeekStart, getNextWeek, getPrevWeek, addDays } from '@/lib/date-utils';
import { format } from 'date-fns';

const TASK_COLORS = [
  '#3B82F6', // blue
  '#10B981', // emerald
  '#F59E0B', // amber
  '#EF4444', // red
  '#8B5CF6', // violet
  '#EC4899', // pink
  '#06B6D4', // cyan
  '#F97316', // orange
];

const DAY_LABELS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

interface WeeklyActivityReportProps {
  readonly currentDate: Date;
}

export function WeeklyActivityReport({ currentDate }: WeeklyActivityReportProps) {
  const [weekOffset, setWeekOffset] = useState(0);

  const weekStart = useMemo(() => {
    let ws = getWeekStart(currentDate);
    for (let i = 0; i < Math.abs(weekOffset); i++) {
      ws = weekOffset > 0 ? getNextWeek(ws) : getPrevWeek(ws);
    }
    return ws;
  }, [currentDate, weekOffset]);

  const weekStartStr = formatDate(weekStart);
  const { summary, loading, error } = useWeeklyActivity(weekStartStr);

  const goToPrevWeek = useCallback(() => setWeekOffset(p => p - 1), []);
  const goToNextWeek = useCallback(() => setWeekOffset(p => p + 1), []);
  const goToCurrentWeek = useCallback(() => setWeekOffset(0), []);

  const weekLabel = useMemo(() => {
    const end = addDays(weekStart, 6);
    if (weekStart.getMonth() === end.getMonth()) {
      return `${format(weekStart, 'd')} - ${format(end, 'd MMMM yyyy')}`;
    }
    return `${format(weekStart, 'd MMM')} - ${format(end, 'd MMM yyyy')}`;
  }, [weekStart]);

  // Build chart data
  const chartData = useMemo(() => {
    if (!summary) return [];

    return summary.dailyTotals.map((day, i) => {
      const entry: Record<string, string | number> = {
        name: DAY_LABELS[i],
        date: day.date,
      };
      for (const task of summary.taskBreakdown) {
        const key = task.taskTitle ?? 'Unassigned';
        entry[key] = Math.round((task.dailyHours[i] ?? 0) * 100) / 100;
      }
      return entry;
    });
  }, [summary]);

  const taskKeys = useMemo(() => {
    if (!summary) return [];
    return summary.taskBreakdown.map(t => t.taskTitle ?? 'Unassigned');
  }, [summary]);

  const [collapsed, setCollapsed] = useState(false);

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-4">
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <button
          type="button"
          onClick={() => setCollapsed(!collapsed)}
          className="flex items-center gap-2"
        >
          <svg
            className={`w-4 h-4 text-gray-500 transition-transform ${collapsed ? '-rotate-90' : ''}`}
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 8.25l-7.5 7.5-7.5-7.5" />
          </svg>
          <h3 className="text-sm font-semibold text-gray-700 uppercase tracking-wider">
            Weekly Summary
          </h3>
        </button>

        <div className="flex items-center gap-2">
          <button
            onClick={goToPrevWeek}
            className="p-1 rounded-md border border-gray-300 hover:bg-gray-50 transition-colors"
            aria-label="Previous week"
          >
            <svg className="w-3.5 h-3.5 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 19.5L8.25 12l7.5-7.5" />
            </svg>
          </button>
          <span className="text-xs font-medium text-gray-600 min-w-[160px] text-center">
            {weekLabel}
          </span>
          <button
            onClick={goToNextWeek}
            className="p-1 rounded-md border border-gray-300 hover:bg-gray-50 transition-colors"
            aria-label="Next week"
          >
            <svg className="w-3.5 h-3.5 text-gray-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
            </svg>
          </button>
          {weekOffset !== 0 && (
            <button
              onClick={goToCurrentWeek}
              className="px-2 py-0.5 text-xs font-medium text-blue-600 border border-blue-300 rounded-md hover:bg-blue-50 transition-colors"
            >
              This week
            </button>
          )}
        </div>
      </div>

      {!collapsed && (
        <>
          {loading && !summary ? (
            <div className="flex items-center justify-center py-8">
              <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
            </div>
          ) : error ? (
            <p className="text-red-500 text-sm py-4">Failed to load weekly summary</p>
          ) : summary && summary.totalHours === 0 ? (
            <div className="text-center py-6">
              <p className="text-gray-500 text-sm">No activity tracked this week</p>
            </div>
          ) : summary ? (
            <div className="space-y-4">
              {/* Total */}
              <div className="text-right">
                <span className="text-xs text-gray-500">Week total: </span>
                <span className="text-sm font-semibold text-gray-800">
                  {summary.totalHours.toFixed(1)}h
                </span>
              </div>

              {/* Stacked bar chart */}
              {taskKeys.length > 0 && (
                <div className="h-48">
                  <ResponsiveContainer width="100%" height="100%">
                    <BarChart data={chartData} margin={{ top: 0, right: 0, left: -20, bottom: 0 }}>
                      <XAxis
                        dataKey="name"
                        tick={{ fontSize: 11, fill: '#6B7280' }}
                        axisLine={false}
                        tickLine={false}
                      />
                      <YAxis
                        tick={{ fontSize: 11, fill: '#9CA3AF' }}
                        axisLine={false}
                        tickLine={false}
                        unit="h"
                      />
                      <Tooltip
                        contentStyle={{ fontSize: 12, borderRadius: 8, border: '1px solid #E5E7EB' }}
                        // eslint-disable-next-line @typescript-eslint/no-explicit-any
                        formatter={((value: number) => `${value.toFixed(1)}h`) as any}
                      />
                      <Legend
                        wrapperStyle={{ fontSize: 11 }}
                        iconSize={8}
                      />
                      {taskKeys.map((key, i) => (
                        <Bar
                          key={key}
                          dataKey={key}
                          stackId="a"
                          fill={TASK_COLORS[i % TASK_COLORS.length]}
                          radius={i === taskKeys.length - 1 ? [2, 2, 0, 0] : [0, 0, 0, 0]}
                        />
                      ))}
                    </BarChart>
                  </ResponsiveContainer>
                </div>
              )}

              {/* Task breakdown table */}
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-gray-200">
                      <th className="text-left py-1.5 pr-2 font-medium text-gray-600">Task</th>
                      {DAY_LABELS.map(d => (
                        <th key={d} className="text-center py-1.5 px-1 font-medium text-gray-500 w-10">{d}</th>
                      ))}
                      <th className="text-right py-1.5 pl-2 font-semibold text-gray-700">Total</th>
                    </tr>
                  </thead>
                  <tbody>
                    {summary.taskBreakdown.map((task, i) => (
                      <tr key={task.taskId ?? 'unassigned'} className="border-b border-gray-100">
                        <td className="py-1.5 pr-2">
                          <div className="flex items-center gap-1.5">
                            <span
                              className="w-2 h-2 rounded-full flex-shrink-0"
                              style={{ backgroundColor: TASK_COLORS[i % TASK_COLORS.length] }}
                            />
                            <span className="text-gray-800 truncate max-w-[200px]">
                              {task.sourceId && (
                                <span className="font-mono text-blue-600 mr-1">{task.sourceId}</span>
                              )}
                              {task.taskTitle ?? 'Unassigned'}
                            </span>
                          </div>
                        </td>
                        {task.dailyHours.map((h, di) => (
                          <td key={di} className="text-center py-1.5 px-1 text-gray-500">
                            {h > 0 ? h.toFixed(1) : '-'}
                          </td>
                        ))}
                        <td className="text-right py-1.5 pl-2 font-medium text-gray-800">
                          {task.totalHours.toFixed(1)}h
                        </td>
                      </tr>
                    ))}
                  </tbody>
                  <tfoot>
                    <tr className="border-t border-gray-300">
                      <td className="py-1.5 pr-2 font-semibold text-gray-700">Total</td>
                      {summary.dailyTotals.map((d, i) => (
                        <td key={i} className="text-center py-1.5 px-1 font-medium text-gray-700">
                          {d.totalHours > 0 ? d.totalHours.toFixed(1) : '-'}
                        </td>
                      ))}
                      <td className="text-right py-1.5 pl-2 font-bold text-gray-900">
                        {summary.totalHours.toFixed(1)}h
                      </td>
                    </tr>
                  </tfoot>
                </table>
              </div>
            </div>
          ) : null}
        </>
      )}
    </div>
  );
}
