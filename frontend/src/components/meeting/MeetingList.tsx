import { MeetingCard } from './MeetingCard';

interface Meeting {
  readonly id: string;
  readonly title: string;
  readonly startTime: string;
  readonly endTime: string;
  readonly location?: string | null;
  readonly durationHours: number;
}

interface MeetingListProps {
  readonly meetings: readonly Meeting[];
  readonly emptyMessage?: string;
}

export function MeetingList({
  meetings,
  emptyMessage = 'No meetings scheduled',
}: MeetingListProps) {
  if (meetings.length === 0) {
    return (
      <p className="text-gray-500 text-sm py-4 text-center">{emptyMessage}</p>
    );
  }

  const sorted = [...meetings].sort(
    (a, b) => new Date(a.startTime).getTime() - new Date(b.startTime).getTime()
  );

  return (
    <div className="space-y-2">
      {sorted.map(meeting => (
        <MeetingCard
          key={meeting.id}
          title={meeting.title}
          startTime={meeting.startTime}
          endTime={meeting.endTime}
          location={meeting.location}
          durationHours={meeting.durationHours}
        />
      ))}
    </div>
  );
}
