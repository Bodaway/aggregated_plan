/** Marks a Gryzzly project that is closed (`status: "done"`).
 *
 * Muted grey on purpose: it is context, not a warning. The amber `stale` badge in
 * GryzzlyTaskPicker means something is wrong (the catalog row is gone or disabled);
 * a terminated project is merely finished, and its tasks stay selectable because a
 * project routinely closes with time declarations still owed on it.
 *
 * One component rather than an inline span per surface, so the picker and the task
 * edit sheet cannot drift apart. */
export function TerminatedBadge({ small = false }: { readonly small?: boolean }) {
  return (
    <span
      className={`inline-flex items-center rounded font-medium bg-gray-200 text-gray-600 flex-shrink-0 ${
        small ? 'px-1 py-0.5 text-[9px]' : 'px-1.5 py-0.5 text-[10px]'
      }`}
    >
      terminé
    </span>
  );
}
