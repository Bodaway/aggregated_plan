import { buildJql, toIsoDate } from '@infrastructure/jira-http-client';

describe('jira-http-client', () => {
  describe('buildJql', () => {
    it('builds JQL from project key only', () => {
      const jql = buildJql({ projectKey: 'PROJ' });
      expect(jql).toBe('project = "PROJ"');
    });

    it('builds JQL with issue types filter', () => {
      const jql = buildJql({
        projectKey: 'PROJ',
        issueTypes: ['Story', 'Bug'],
      });
      expect(jql).toBe('project = "PROJ" AND issuetype in ("Story", "Bug")');
    });

    it('builds JQL with statuses filter', () => {
      const jql = buildJql({
        projectKey: 'PROJ',
        statuses: ['To Do', 'In Progress'],
      });
      expect(jql).toBe('project = "PROJ" AND status in ("To Do", "In Progress")');
    });

    it('builds JQL with all filters combined', () => {
      const jql = buildJql({
        projectKey: 'PROJ',
        issueTypes: ['Story'],
        statuses: ['Done'],
      });
      expect(jql).toBe('project = "PROJ" AND issuetype in ("Story") AND status in ("Done")');
    });

    it('uses raw JQL when provided', () => {
      const jql = buildJql({
        projectKey: 'PROJ',
        jql: 'project = PROJ AND sprint in openSprints()',
        issueTypes: ['Story'],
      });
      expect(jql).toBe('project = PROJ AND sprint in openSprints()');
    });

    it('ignores empty issue types array', () => {
      const jql = buildJql({
        projectKey: 'PROJ',
        issueTypes: [],
      });
      expect(jql).toBe('project = "PROJ"');
    });

    it('ignores empty statuses array', () => {
      const jql = buildJql({
        projectKey: 'PROJ',
        statuses: [],
      });
      expect(jql).toBe('project = "PROJ"');
    });
  });

  describe('toIsoDate', () => {
    it('converts ISO datetime to ISO date string', () => {
      expect(toIsoDate('2024-03-15T10:30:00.000+0000')).toBe('2024-03-15');
    });

    it('handles already short dates', () => {
      expect(toIsoDate('2024-03-15')).toBe('2024-03-15');
    });
  });
});
