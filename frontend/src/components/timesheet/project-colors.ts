// Stable project→colour mapping (hash the id so a project keeps its colour across days).
const SLOT_COLORS = [
  'bg-blue-400', 'bg-green-400', 'bg-purple-400', 'bg-amber-400',
  'bg-rose-400', 'bg-cyan-400', 'bg-orange-400', 'bg-teal-400',
] as const;

export function projectColor(projectId: string | null): string {
  if (!projectId) return 'bg-gray-300'; // unattributed
  let hash = 0;
  for (let i = 0; i < projectId.length; i += 1) {
    hash = (hash * 31 + projectId.charCodeAt(i)) | 0;
  }
  return SLOT_COLORS[Math.abs(hash) % SLOT_COLORS.length];
}
