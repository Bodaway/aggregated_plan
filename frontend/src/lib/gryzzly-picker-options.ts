export interface GryzzlyOption {
  gryzzlyTaskId: string;
  name: string;
  projectName: string;
  stale?: boolean;
  /** Owning Gryzzly project's status: 'active' | 'done', or null/undefined when
   *  unknown (a catalog row predating the column). Only 'done' shows a badge. */
  projectStatus?: string | null;
}

export interface AssignedGryzzlyTask {
  gryzzlyTaskId: string;
  name: string | null;
  projectName: string | null;
  stale: boolean;
  projectStatus?: string | null;
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
      // Rebuilt field by field, so this has to be carried explicitly — a closed
      // project must still read as closed when its task is only pinned here.
      projectStatus: assigned.projectStatus ?? null,
    },
  ];
}
