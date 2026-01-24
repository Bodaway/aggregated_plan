import React, { useEffect, useState } from 'react';
import type {
  Assignment,
  Conflict,
  Developer,
  HalfDay,
  IsoDateString,
  Milestone,
  MilestoneType,
  Project,
} from '@domain/index';
import {
  loadAssignments,
  loadConflicts,
  loadDevelopers,
  loadMilestones,
  loadProjects,
  editDeveloper,
  submitAssignment,
  submitDeveloper,
  submitMilestone,
  submitProject,
} from '@application/index';
import { Timeline } from './timeline';

type ProjectFormState = {
  readonly name: string;
  readonly description: string;
  readonly startDate: string;
  readonly endDate: string;
  readonly createdBy: string;
};

type AssignmentFormState = {
  readonly projectId: string;
  readonly developerId: string;
  readonly date: string;
  readonly halfDay: HalfDay;
};

type DeveloperFormState = {
  readonly displayName: string;
  readonly email: string;
  readonly capacityHalfDaysPerWeek: string;
};

type DeveloperEditFormState = {
  readonly id: string;
  readonly displayName: string;
  readonly email: string;
  readonly capacityHalfDaysPerWeek: string;
};

type MilestoneFormState = {
  readonly projectId: string;
  readonly name: string;
  readonly date: string;
  readonly type: MilestoneType;
};

type ViewTab = 'portfolio' | 'timeline';

const DEFAULT_PROJECT_FORM: ProjectFormState = {
  name: '',
  description: '',
  startDate: '',
  endDate: '',
  createdBy: 'admin',
};

const DEFAULT_ASSIGNMENT_FORM: AssignmentFormState = {
  projectId: '',
  developerId: '',
  date: '',
  halfDay: 'morning',
};

const DEFAULT_DEVELOPER_FORM: DeveloperFormState = {
  displayName: '',
  email: '',
  capacityHalfDaysPerWeek: '',
};

const DEFAULT_DEVELOPER_EDIT_FORM: DeveloperEditFormState = {
  id: '',
  displayName: '',
  email: '',
  capacityHalfDaysPerWeek: '',
};

const DEFAULT_MILESTONE_FORM: MilestoneFormState = {
  projectId: '',
  name: '',
  date: '',
  type: 'delivery',
};

const getErrorMessage = (error: unknown): string => {
  if (error instanceof Error) {
    return error.message;
  }
  return 'Unexpected error';
};

export const App: React.FC = () => {
  const [projects, setProjects] = useState<readonly Project[]>([]);
  const [milestones, setMilestones] = useState<readonly Milestone[]>([]);
  const [developers, setDevelopers] = useState<readonly Developer[]>([]);
  const [assignments, setAssignments] = useState<readonly Assignment[]>([]);
  const [conflicts, setConflicts] = useState<readonly Conflict[]>([]);
  const [projectForm, setProjectForm] = useState<ProjectFormState>(DEFAULT_PROJECT_FORM);
  const [assignmentForm, setAssignmentForm] = useState<AssignmentFormState>(DEFAULT_ASSIGNMENT_FORM);
  const [developerForm, setDeveloperForm] = useState<DeveloperFormState>(DEFAULT_DEVELOPER_FORM);
  const [developerEditForm, setDeveloperEditForm] = useState<DeveloperEditFormState>(
    DEFAULT_DEVELOPER_EDIT_FORM,
  );
  const [milestoneForm, setMilestoneForm] = useState<MilestoneFormState>(DEFAULT_MILESTONE_FORM);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<ViewTab>('portfolio');

  const refreshData = async (): Promise<void> => {
    setIsLoading(true);
    setErrorMessage(null);
    try {
      const [projectsData, milestonesData, developersData, conflictsData, assignmentsData] = await Promise.all([
        loadProjects(),
        loadMilestones(),
        loadDevelopers(),
        loadConflicts(),
        loadAssignments(),
      ]);
      setProjects(projectsData);
      setMilestones(milestonesData);
      setDevelopers(developersData);
      setConflicts(conflictsData);
      setAssignments(assignmentsData);
    } catch (error) {
      setErrorMessage(getErrorMessage(error));
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.fetch !== 'function') {
      setIsLoading(false);
      return;
    }
    void refreshData();
  }, []);

  const updateProjectForm = (field: keyof ProjectFormState, value: string): void => {
    setProjectForm((prev) => ({ ...prev, [field]: value }));
  };

  const updateAssignmentForm = (
    field: keyof AssignmentFormState,
    value: string,
  ): void => {
    if (field === 'halfDay' && (value === 'morning' || value === 'afternoon')) {
      setAssignmentForm((prev) => ({ ...prev, halfDay: value }));
      return;
    }
    setAssignmentForm((prev) => ({ ...prev, [field]: value }));
  };

  const updateDeveloperForm = (
    field: keyof DeveloperFormState,
    value: string,
  ): void => {
    setDeveloperForm((prev) => ({ ...prev, [field]: value }));
  };

  const updateDeveloperEditForm = (
    field: keyof DeveloperEditFormState,
    value: string,
  ): void => {
    setDeveloperEditForm((prev) => ({ ...prev, [field]: value }));
  };

  const selectDeveloperForEdit = (developerId: string): void => {
    const developer = developers.find((item) => item.id === developerId);
    if (!developer) {
      setDeveloperEditForm(DEFAULT_DEVELOPER_EDIT_FORM);
      return;
    }
    setDeveloperEditForm({
      id: developer.id,
      displayName: developer.displayName,
      email: developer.email,
      capacityHalfDaysPerWeek: developer.capacityHalfDaysPerWeek.toString(),
    });
  };

  const updateMilestoneForm = (
    field: keyof MilestoneFormState,
    value: string,
  ): void => {
    if (
      field === 'type' &&
      (value === 'delivery' || value === 'review' || value === 'demo' || value === 'other')
    ) {
      setMilestoneForm((prev) => ({ ...prev, type: value }));
      return;
    }
    setMilestoneForm((prev) => ({ ...prev, [field]: value }));
  };

  const getDeveloperLabel = (developerId: string): string =>
    developers.find((developer) => developer.id === developerId)?.displayName ?? developerId;

  const getProjectLabel = (projectId: string): string =>
    projects.find((project) => project.id === projectId)?.name ?? projectId;

  const formatConflictDetails = (conflict: Conflict): string => {
    const developerLabel = getDeveloperLabel(conflict.developerId);
    const projectLabels = conflict.projectIds?.map(getProjectLabel).join(', ');
    if (conflict.type === 'capacity') {
      const weekLabel = conflict.weekStart ? `Week of ${conflict.weekStart}` : 'Capacity';
      return `${developerLabel} - ${weekLabel} (${conflict.assignedHalfDays ?? 0}/${conflict.capacityHalfDays ?? 0} half-days)`;
    }
    const dateLabel = conflict.dates.join(', ');
    const halfDayLabel = conflict.halfDay ? ` (${conflict.halfDay})` : '';
    if (projectLabels) {
      return `${developerLabel} - ${projectLabels} on ${dateLabel}${halfDayLabel}`;
    }
    return `${developerLabel} - ${dateLabel}${halfDayLabel}`;
  };

  const handleCreateProject = async (
    event: React.FormEvent<HTMLFormElement>,
  ): Promise<void> => {
    event.preventDefault();
    if (!projectForm.name || !projectForm.startDate || !projectForm.endDate || !projectForm.createdBy) {
      setErrorMessage('Please fill in all required project fields.');
      return;
    }
    try {
      await submitProject({
        name: projectForm.name,
        description: projectForm.description || undefined,
        startDate: projectForm.startDate as IsoDateString,
        endDate: projectForm.endDate as IsoDateString,
        createdBy: projectForm.createdBy,
      });
      setProjectForm(DEFAULT_PROJECT_FORM);
      await refreshData();
    } catch (error) {
      setErrorMessage(getErrorMessage(error));
    }
  };

  const handleCreateAssignment = async (
    event: React.FormEvent<HTMLFormElement>,
  ): Promise<void> => {
    event.preventDefault();
    if (!assignmentForm.projectId || !assignmentForm.developerId || !assignmentForm.date) {
      setErrorMessage('Please select a project, developer, and date.');
      return;
    }
    try {
      await submitAssignment({
        projectId: assignmentForm.projectId,
        developerId: assignmentForm.developerId,
        date: assignmentForm.date as IsoDateString,
        halfDay: assignmentForm.halfDay,
      });
      setAssignmentForm(DEFAULT_ASSIGNMENT_FORM);
      await refreshData();
    } catch (error) {
      setErrorMessage(getErrorMessage(error));
    }
  };

  const handleCreateDeveloper = async (
    event: React.FormEvent<HTMLFormElement>,
  ): Promise<void> => {
    event.preventDefault();
    if (!developerForm.displayName || !developerForm.email) {
      setErrorMessage('Please provide a developer name and email.');
      return;
    }
    const capacityValue = developerForm.capacityHalfDaysPerWeek.trim();
    const capacity =
      capacityValue.length > 0 ? Number(capacityValue) : undefined;
    if (capacityValue.length > 0 && Number.isNaN(capacity)) {
      setErrorMessage('Capacity must be a number.');
      return;
    }
    try {
      await submitDeveloper({
        displayName: developerForm.displayName,
        email: developerForm.email,
        capacityHalfDaysPerWeek: capacity,
      });
      setDeveloperForm(DEFAULT_DEVELOPER_FORM);
      await refreshData();
    } catch (error) {
      setErrorMessage(getErrorMessage(error));
    }
  };

  const handleUpdateDeveloper = async (
    event: React.FormEvent<HTMLFormElement>,
  ): Promise<void> => {
    event.preventDefault();
    if (!developerEditForm.id) {
      setErrorMessage('Please select a developer to update.');
      return;
    }
    if (!developerEditForm.displayName || !developerEditForm.email) {
      setErrorMessage('Please provide a developer name and email.');
      return;
    }
    const capacityValue = developerEditForm.capacityHalfDaysPerWeek.trim();
    const capacity =
      capacityValue.length > 0 ? Number(capacityValue) : undefined;
    if (capacityValue.length > 0 && Number.isNaN(capacity)) {
      setErrorMessage('Capacity must be a number.');
      return;
    }
    try {
      await editDeveloper(developerEditForm.id, {
        displayName: developerEditForm.displayName,
        email: developerEditForm.email,
        capacityHalfDaysPerWeek: capacity,
      });
      setDeveloperEditForm(DEFAULT_DEVELOPER_EDIT_FORM);
      await refreshData();
    } catch (error) {
      setErrorMessage(getErrorMessage(error));
    }
  };

  const handleCreateMilestone = async (
    event: React.FormEvent<HTMLFormElement>,
  ): Promise<void> => {
    event.preventDefault();
    if (!milestoneForm.projectId || !milestoneForm.name || !milestoneForm.date) {
      setErrorMessage('Please select a project, milestone name, and date.');
      return;
    }
    try {
      await submitMilestone({
        projectId: milestoneForm.projectId,
        name: milestoneForm.name,
        date: milestoneForm.date as IsoDateString,
        type: milestoneForm.type,
      });
      setMilestoneForm(DEFAULT_MILESTONE_FORM);
      await refreshData();
    } catch (error) {
      setErrorMessage(getErrorMessage(error));
    }
  };

  return (
    <main>
      <h1>Aggregated Plan</h1>
      {errorMessage ? <p role="alert">{errorMessage}</p> : null}
      {isLoading ? <p>Loading data...</p> : null}

      <div className="tabs">
        <button
          type="button"
          className={`tab-button ${activeTab === 'portfolio' ? 'active' : ''}`}
          onClick={() => setActiveTab('portfolio')}
        >
          Portfolio
        </button>
        <button
          type="button"
          className={`tab-button ${activeTab === 'timeline' ? 'active' : ''}`}
          onClick={() => setActiveTab('timeline')}
        >
          Timeline
        </button>
      </div>

      {activeTab === 'portfolio' ? (
        <>
          <section>
            <h2>Portfolio</h2>
            <form onSubmit={handleCreateProject}>
              <label>
                Name
                <input
                  type="text"
                  value={projectForm.name}
                  onChange={(event) => updateProjectForm('name', event.target.value)}
                  required
                />
              </label>
              <label>
                Description
                <input
                  type="text"
                  value={projectForm.description}
                  onChange={(event) => updateProjectForm('description', event.target.value)}
                />
              </label>
              <label>
                Start date
                <input
                  type="date"
                  value={projectForm.startDate}
                  onChange={(event) => updateProjectForm('startDate', event.target.value)}
                  required
                />
              </label>
              <label>
                End date
                <input
                  type="date"
                  value={projectForm.endDate}
                  onChange={(event) => updateProjectForm('endDate', event.target.value)}
                  required
                />
              </label>
              <label>
                Created by
                <input
                  type="text"
                  value={projectForm.createdBy}
                  onChange={(event) => updateProjectForm('createdBy', event.target.value)}
                  required
                />
              </label>
              <button type="submit">Create project</button>
            </form>

            <ul>
              {projects.map((project) => (
                <li key={project.id}>
                  <strong>{project.name}</strong> ({project.status}) {project.startDate} →{' '}
                  {project.endDate}
                </li>
              ))}
            </ul>

            <h3>Milestones</h3>
            <form onSubmit={handleCreateMilestone}>
              <label>
                Project
                <select
                  value={milestoneForm.projectId}
                  onChange={(event) => updateMilestoneForm('projectId', event.target.value)}
                  required
                >
                  <option value="">Select</option>
                  {projects.map((project) => (
                    <option key={project.id} value={project.id}>
                      {project.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Name
                <input
                  type="text"
                  value={milestoneForm.name}
                  onChange={(event) => updateMilestoneForm('name', event.target.value)}
                  required
                />
              </label>
              <label>
                Date
                <input
                  type="date"
                  value={milestoneForm.date}
                  onChange={(event) => updateMilestoneForm('date', event.target.value)}
                  required
                />
              </label>
              <label>
                Type
                <select
                  value={milestoneForm.type}
                  onChange={(event) => updateMilestoneForm('type', event.target.value)}
                >
                  <option value="delivery">Delivery</option>
                  <option value="review">Review</option>
                  <option value="demo">Demo</option>
                  <option value="other">Other</option>
                </select>
              </label>
              <button type="submit">Add milestone</button>
            </form>
            <ul>
              {milestones.map((milestone) => (
                <li key={milestone.id}>
                  <strong>{milestone.name}</strong> ({milestone.type}) {milestone.date}
                </li>
              ))}
            </ul>
          </section>

          <section>
            <h2>Staffing</h2>
            <form onSubmit={handleCreateDeveloper}>
              <label>
                Developer name
                <input
                  type="text"
                  value={developerForm.displayName}
                  onChange={(event) => updateDeveloperForm('displayName', event.target.value)}
                  required
                />
              </label>
              <label>
                Email
                <input
                  type="email"
                  value={developerForm.email}
                  onChange={(event) => updateDeveloperForm('email', event.target.value)}
                  required
                />
              </label>
              <label>
                Capacity (half-days/week)
                <input
                  type="number"
                  min={1}
                  max={10}
                  value={developerForm.capacityHalfDaysPerWeek}
                  onChange={(event) =>
                    updateDeveloperForm('capacityHalfDaysPerWeek', event.target.value)
                  }
                />
              </label>
              <button type="submit">Add developer</button>
            </form>

            <form onSubmit={handleUpdateDeveloper}>
              <label>
                Developer
                <select
                  value={developerEditForm.id}
                  onChange={(event) => selectDeveloperForEdit(event.target.value)}
                  required
                >
                  <option value="">Select</option>
                  {developers.map((developer) => (
                    <option key={developer.id} value={developer.id}>
                      {developer.displayName}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Name
                <input
                  type="text"
                  value={developerEditForm.displayName}
                  onChange={(event) => updateDeveloperEditForm('displayName', event.target.value)}
                  required
                />
              </label>
              <label>
                Email
                <input
                  type="email"
                  value={developerEditForm.email}
                  onChange={(event) => updateDeveloperEditForm('email', event.target.value)}
                  required
                />
              </label>
              <label>
                Capacity (half-days/week)
                <input
                  type="number"
                  min={1}
                  max={10}
                  value={developerEditForm.capacityHalfDaysPerWeek}
                  onChange={(event) =>
                    updateDeveloperEditForm('capacityHalfDaysPerWeek', event.target.value)
                  }
                />
              </label>
              <button type="submit">Update developer</button>
            </form>
            <form onSubmit={handleCreateAssignment}>
              <label>
                Project
                <select
                  value={assignmentForm.projectId}
                  onChange={(event) => updateAssignmentForm('projectId', event.target.value)}
                  required
                >
                  <option value="">Select</option>
                  {projects.map((project) => (
                    <option key={project.id} value={project.id}>
                      {project.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Developer
                <select
                  value={assignmentForm.developerId}
                  onChange={(event) => updateAssignmentForm('developerId', event.target.value)}
                  required
                >
                  <option value="">Select</option>
                  {developers.map((developer) => (
                    <option key={developer.id} value={developer.id}>
                      {developer.displayName}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Date
                <input
                  type="date"
                  value={assignmentForm.date}
                  onChange={(event) => updateAssignmentForm('date', event.target.value)}
                  required
                />
              </label>
              <label>
                Half-day
                <select
                  value={assignmentForm.halfDay}
                  onChange={(event) => updateAssignmentForm('halfDay', event.target.value)}
                >
                  <option value="morning">Morning</option>
                  <option value="afternoon">Afternoon</option>
                </select>
              </label>
              <button type="submit">Assign developer</button>
            </form>

            <h3>Developers</h3>
            <ul>
              {developers.map((developer) => (
                <li key={developer.id}>
                  {developer.displayName} - Capacity: {developer.capacityHalfDaysPerWeek} half-days/week
                </li>
              ))}
            </ul>
          </section>

          <section>
            <h2>Conflicts</h2>
            {conflicts.length === 0 ? (
              <p>No conflicts detected.</p>
            ) : (
              <ul>
                {conflicts.map((conflict, index) => (
                  <li key={`${conflict.type}-${index}`}>
                    <strong>{conflict.type}</strong> - {conflict.message} ({formatConflictDetails(conflict)})
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      ) : (
        <Timeline
          projects={projects}
          assignments={assignments}
          developers={developers}
          milestones={milestones}
          conflicts={conflicts}
        />
      )}
    </main>
  );
};
