import type { EntityId } from '@aggregated-plan/shared-types';
import type {
  AllocationRepository,
  AssignmentRepository,
  AvailabilityRepository,
  DeveloperRepository,
  ProjectRepository,
} from '@application/index';
import type { InMemoryStore } from './in-memory-store';

const findById = <T extends { readonly id: EntityId }>(
  items: readonly T[],
  id: EntityId,
): T | null => items.find((item) => item.id === id) ?? null;

export const createProjectRepository = (store: InMemoryStore): ProjectRepository => ({
  list: async () => [...store.projects],
  getById: async (id) => findById(store.projects, id),
  getByName: async (name) =>
    store.projects.find(
      (project) => project.name.toLowerCase() === name.trim().toLowerCase(),
    ) ?? null,
  save: async (project) => {
    store.projects = [...store.projects, project];
    return project;
  },
  update: async (project) => {
    store.projects = store.projects.map((item) => (item.id === project.id ? project : item));
    return project;
  },
  remove: async (id) => {
    store.projects = store.projects.filter((item) => item.id !== id);
  },
});

export const createAssignmentRepository = (store: InMemoryStore): AssignmentRepository => ({
  list: async () => [...store.assignments],
  listByDeveloper: async (developerId) =>
    store.assignments.filter((assignment) => assignment.developerId === developerId),
  save: async (assignment) => {
    store.assignments = [...store.assignments, assignment];
    return assignment;
  },
  saveMany: async (assignments) => {
    store.assignments = [...store.assignments, ...assignments];
    return assignments;
  },
});

export const createAllocationRepository = (store: InMemoryStore): AllocationRepository => ({
  list: async () => [...store.allocations],
  listByDeveloper: async (developerId) =>
    store.allocations.filter((allocation) => allocation.developerId === developerId),
  save: async (allocation) => {
    store.allocations = [...store.allocations, allocation];
    return allocation;
  },
});

export const createAvailabilityRepository = (
  store: InMemoryStore,
): AvailabilityRepository => ({
  list: async () => [...store.availabilities],
  listByDeveloper: async (developerId) =>
    store.availabilities.filter((availability) => availability.developerId === developerId),
  save: async (availability) => {
    store.availabilities = [...store.availabilities, availability];
    return availability;
  },
});

export const createDeveloperRepository = (store: InMemoryStore): DeveloperRepository => ({
  list: async () => [...store.developers],
  getById: async (id) => findById(store.developers, id),
  save: async (developer) => {
    store.developers = [...store.developers, developer];
    return developer;
  },
});

export type InMemoryRepositories = {
  readonly projectRepository: ProjectRepository;
  readonly assignmentRepository: AssignmentRepository;
  readonly allocationRepository: AllocationRepository;
  readonly availabilityRepository: AvailabilityRepository;
  readonly developerRepository: DeveloperRepository;
};

export const createInMemoryRepositories = (store: InMemoryStore): InMemoryRepositories => ({
  projectRepository: createProjectRepository(store),
  assignmentRepository: createAssignmentRepository(store),
  allocationRepository: createAllocationRepository(store),
  availabilityRepository: createAvailabilityRepository(store),
  developerRepository: createDeveloperRepository(store),
});
