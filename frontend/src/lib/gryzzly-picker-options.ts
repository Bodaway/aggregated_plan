export interface GryzzlyOption {
  gryzzlyTaskId: string;
  name: string;
  projectName: string;
  stale?: boolean;
}

export interface AssignedGryzzlyTask {
  gryzzlyTaskId: string;
  name: string | null;
  projectName: string | null;
  stale: boolean;
}

/** Active options sorted by project then name, plus the currently-assigned task
 *  (even if inactive/missing from the active list) so the user can see & clear it. */
export function buildPickerOptions(
  active: GryzzlyOption[],
  assigned: AssignedGryzzlyTask | null,
): GryzzlyOption[] {
  const sorted = [...active].sort(
    (a, b) => a.projectName.localeCompare(b.projectName) || a.name.localeCompare(b.name),
  );
  if (!assigned) return sorted;
  if (sorted.some((o) => o.gryzzlyTaskId === assigned.gryzzlyTaskId)) return sorted;
  return [
    ...sorted,
    {
      gryzzlyTaskId: assigned.gryzzlyTaskId,
      name: assigned.name ?? "(unknown Gryzzly task)",
      projectName: assigned.projectName ?? "(archived)",
      stale: true,
    },
  ];
}
