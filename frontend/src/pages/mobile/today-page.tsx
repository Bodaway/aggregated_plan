import { useEffect, useMemo, useState } from 'react';
import { useQuery } from 'urql';
import { addDays, formatDate } from '@/lib/date-utils';
import {
  MOBILE_TODAY_QUERY,
  MOBILE_CONFIGURATION_QUERY,
  MOBILE_ACTIVE_TASK_QUERY,
} from '@/graphql/queries/mobile';
import { MobileShell } from './mobile-shell';

/** Rang numérique de l'urgence, pour le tri décroissant (voir `use-dashboard.ts`). */
const URGENCY_RANK: Record<string, number> = { LOW: 1, MEDIUM: 2, HIGH: 3, CRITICAL: 4 };

interface MobileTaskProject {
  readonly name: string;
}

interface MobileTaskNode {
  readonly id: string;
  readonly title: string;
  readonly deadline: string | null;
  readonly urgency: string;
  readonly status: string;
  readonly project: MobileTaskProject | null;
}

interface MobileTasksData {
  readonly tasks: { readonly edges: readonly { readonly node: MobileTaskNode }[] };
}

interface MobileConfigurationData {
  readonly configuration: Record<string, string>;
}

interface MobileActiveTaskData {
  readonly task: { readonly id: string; readonly title: string } | null;
}

type BucketKey = 'overdue' | 'today' | 'tomorrow';

interface Bucketed {
  readonly overdue: readonly MobileTaskNode[];
  readonly today: readonly MobileTaskNode[];
  readonly tomorrow: readonly MobileTaskNode[];
}

/**
 * Seau d'une tâche selon son échéance, ou `null` si elle n'entre dans aucun
 * des trois -- notamment une tâche sans échéance : mieux vaut l'absence
 * silencieuse que de la faire atterrir dans "aujourd'hui" faute de mieux,
 * ce qui masquerait un vrai bug de tri.
 */
function bucketOf(deadline: string | null, todayStr: string, tomorrowStr: string): BucketKey | null {
  if (!deadline) return null;
  if (deadline < todayStr) return 'overdue';
  if (deadline === todayStr) return 'today';
  if (deadline === tomorrowStr) return 'tomorrow';
  return null;
}

/** Échéance croissante, puis urgence décroissante à échéance égale. */
function compareTasks(a: MobileTaskNode, b: MobileTaskNode): number {
  const ad = a.deadline ?? '';
  const bd = b.deadline ?? '';
  if (ad !== bd) return ad < bd ? -1 : 1;
  return (URGENCY_RANK[b.urgency] ?? 0) - (URGENCY_RANK[a.urgency] ?? 0);
}

function bucketize(nodes: readonly MobileTaskNode[], todayStr: string, tomorrowStr: string): Bucketed {
  const overdue: MobileTaskNode[] = [];
  const today: MobileTaskNode[] = [];
  const tomorrow: MobileTaskNode[] = [];
  for (const node of nodes) {
    const key = bucketOf(node.deadline, todayStr, tomorrowStr);
    if (key === 'overdue') overdue.push(node);
    else if (key === 'today') today.push(node);
    else if (key === 'tomorrow') tomorrow.push(node);
  }
  overdue.sort(compareTasks);
  today.sort(compareTasks);
  tomorrow.sort(compareTasks);
  return { overdue, today, tomorrow };
}

function formatSeenAt(date: Date): string {
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function TaskRow({ task }: { readonly task: MobileTaskNode }) {
  return (
    <li className="mobile-task-card">
      <p>{task.title}</p>
      {task.project && <p>{task.project.name}</p>}
    </li>
  );
}

function Bucket({ testId, tasks }: { readonly testId: string; readonly tasks: readonly MobileTaskNode[] }) {
  return (
    <ul className="mobile-bucket" data-testid={testId}>
      {tasks.length === 0 ? (
        <li className="mobile-task-card">Rien à signaler</li>
      ) : (
        tasks.map(t => <TaskRow key={t.id} task={t} />)
      )}
    </ul>
  );
}

/**
 * Plan du jour mobile : trois seaux (en retard, aujourd'hui, demain), triés
 * par échéance croissante puis urgence décroissante. Hors ligne, la dernière
 * réponse connue reste affichée, horodatée -- jamais un écran vide qui
 * laisserait croire qu'il n'y a rien à faire.
 */
export function TodayPage() {
  const todayStr = formatDate(new Date());
  const tomorrowStr = formatDate(addDays(new Date(), 1));

  const [tasksResult] = useQuery<MobileTasksData>({
    query: MOBILE_TODAY_QUERY,
    variables: { until: tomorrowStr },
  });

  // Tâche active : `configuration` (clé `aplan.active_task_id`) d'abord, puis
  // `task(id:)`. Le pointeur est posé par la CLI/HUD ; l'écran mobile se
  // contente de le lire.
  const [configResult] = useQuery<MobileConfigurationData>({ query: MOBILE_CONFIGURATION_QUERY });
  const activeTaskId = configResult.data?.configuration?.['aplan.active_task_id'] || null;
  const [activeTaskResult] = useQuery<MobileActiveTaskData>({
    query: MOBILE_ACTIVE_TASK_QUERY,
    variables: { id: activeTaskId ?? '' },
    pause: !activeTaskId,
  });
  const activeTask = activeTaskId ? activeTaskResult.data?.task ?? null : null;

  // Dernière réponse connue, gardée à côté de l'état urql : sans cette copie,
  // une coupure réseau effacerait le plan du jour au lieu de le montrer
  // périmé -- l'un n'implique pas l'autre.
  const [lastGood, setLastGood] = useState<{ nodes: readonly MobileTaskNode[]; seenAt: Date } | null>(null);

  useEffect(() => {
    if (tasksResult.data?.tasks) {
      setLastGood({
        nodes: tasksResult.data.tasks.edges.map(e => e.node),
        seenAt: new Date(),
      });
    }
  }, [tasksResult.data]);

  const isOffline = Boolean(tasksResult.error) && !tasksResult.data;
  const nodes = tasksResult.data?.tasks
    ? tasksResult.data.tasks.edges.map(e => e.node)
    : lastGood?.nodes ?? [];

  const buckets = useMemo(
    () => bucketize(nodes, todayStr, tomorrowStr),
    [nodes, todayStr, tomorrowStr],
  );

  return (
    <MobileShell>
      {isOffline && lastGood && (
        <p className="mobile-offline-banner">Hors ligne — vu à {formatSeenAt(lastGood.seenAt)}</p>
      )}

      {activeTask && <p className="mobile-active-task">Tâche active : {activeTask.title}</p>}

      <h2>En retard</h2>
      <Bucket testId="bucket-overdue" tasks={buckets.overdue} />

      <h2>Aujourd’hui</h2>
      <Bucket testId="bucket-today" tasks={buckets.today} />

      <h2>Demain</h2>
      <Bucket testId="bucket-tomorrow" tasks={buckets.tomorrow} />
    </MobileShell>
  );
}
