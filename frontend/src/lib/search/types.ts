export interface SearchableTask {
  readonly id: string;
  readonly title: string;
  readonly sourceId: string | null;
  readonly source: 'JIRA' | 'EXCEL' | 'OBSIDIAN' | 'PERSONAL' | 'OUTLOOK';
  readonly assignee: string | null;
  readonly projectName: string | null;
  readonly tags: readonly string[];
  readonly description: string | null;
  readonly status: 'TODO' | 'IN_PROGRESS' | 'DONE' | 'BLOCKED';
}
