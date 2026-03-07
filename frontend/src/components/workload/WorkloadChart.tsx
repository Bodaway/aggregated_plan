import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';
import { HALF_DAY_HOURS } from '@/lib/constants';

interface HalfDaySlot {
  readonly date: string;
  readonly halfDay: string;
  readonly consumption: number;
  readonly isFree: boolean;
}

interface WorkloadChartProps {
  readonly halfDays: readonly HalfDaySlot[];
  readonly compact?: boolean;
}

interface ChartDataPoint {
  readonly label: string;
  readonly meetings: number;
  readonly available: number;
}

const DAY_ABBREVIATIONS: Record<number, string> = {
  0: 'Sun',
  1: 'Mon',
  2: 'Tue',
  3: 'Wed',
  4: 'Thu',
  5: 'Fri',
  6: 'Sat',
};

function buildChartData(halfDays: readonly HalfDaySlot[]): readonly ChartDataPoint[] {
  return halfDays.map(slot => {
    const dayOfWeek = new Date(slot.date).getDay();
    const dayLabel = DAY_ABBREVIATIONS[dayOfWeek] ?? slot.date;
    const halfLabel = slot.halfDay === 'AM' ? 'AM' : 'PM';
    const meetingHours = Math.min(slot.consumption, HALF_DAY_HOURS);
    const available = Math.max(HALF_DAY_HOURS - meetingHours, 0);

    return {
      label: `${dayLabel} ${halfLabel}`,
      meetings: parseFloat(meetingHours.toFixed(1)),
      available: parseFloat(available.toFixed(1)),
    };
  });
}

export function WorkloadChart({ halfDays, compact = false }: WorkloadChartProps) {
  const data = buildChartData(halfDays);

  if (data.length === 0) {
    return (
      <p className="text-gray-500 text-sm py-4 text-center">No workload data available</p>
    );
  }

  const chartHeight = compact ? 200 : 300;

  return (
    <ResponsiveContainer width="100%" height={chartHeight}>
      <BarChart data={data} margin={{ top: 8, right: 8, left: 0, bottom: 4 }}>
        <CartesianGrid strokeDasharray="3 3" stroke="#E5E7EB" />
        <XAxis
          dataKey="label"
          tick={{ fontSize: 11, fill: '#6B7280' }}
          tickLine={false}
          axisLine={{ stroke: '#D1D5DB' }}
        />
        <YAxis
          tick={{ fontSize: 11, fill: '#6B7280' }}
          tickLine={false}
          axisLine={{ stroke: '#D1D5DB' }}
          domain={[0, HALF_DAY_HOURS]}
          label={
            compact
              ? undefined
              : {
                  value: 'Hours',
                  angle: -90,
                  position: 'insideLeft',
                  style: { fontSize: 12, fill: '#6B7280' },
                }
          }
        />
        <Tooltip
          contentStyle={{
            backgroundColor: '#FFFFFF',
            border: '1px solid #E5E7EB',
            borderRadius: '8px',
            fontSize: '12px',
          }}
        />
        {!compact && <Legend wrapperStyle={{ fontSize: '12px' }} />}
        <Bar
          dataKey="meetings"
          stackId="workload"
          fill="#3B82F6"
          name="Meetings"
          radius={[0, 0, 0, 0]}
        />
        <Bar
          dataKey="available"
          stackId="workload"
          fill="#10B981"
          name="Available"
          radius={[2, 2, 0, 0]}
        />
      </BarChart>
    </ResponsiveContainer>
  );
}
