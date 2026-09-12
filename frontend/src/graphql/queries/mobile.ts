/**
 * Requêtes GraphQL du shell mobile (plan du jour + capture).
 *
 * Aucun codegen ici : le reste du dépôt exporte des template literals
 * (`break-rules.ts`) plutôt que des fichiers `.graphql`, et cet écran suit la
 * même convention.
 */

/**
 * `first: 200` est explicite : le défaut serveur est `50` en ordre
 * décroissant, ce qui ferait taire silencieusement les échéances les plus
 * anciennes -- exactement l'inverse de ce qu'un plan du jour doit montrer.
 */
export const MOBILE_TODAY_QUERY = `
  query MobileToday($until: NaiveDate!) {
    tasks(
      filter: {
        deadlineBefore: $until
        status: [TODO, IN_PROGRESS, BLOCKED]
        trackingState: [FOLLOWED]
      }
      first: 200
    ) {
      edges {
        node {
          id
          title
          deadline
          urgency
          status
          project {
            name
          }
        }
      }
    }
  }
`;

/** Lit `aplan.active_task_id` pour retrouver la tâche active (posée par la CLI/HUD). */
export const MOBILE_CONFIGURATION_QUERY = `
  query MobileConfiguration {
    configuration
  }
`;

export const MOBILE_ACTIVE_TASK_QUERY = `
  query MobileActiveTask($id: ID!) {
    task(id: $id) {
      id
      title
    }
  }
`;

/** Alimente le `<select>` projet de l'écran de capture. */
export const MOBILE_PROJECTS_QUERY = `
  query MobileProjects {
    projects {
      id
      name
    }
  }
`;

export const MOBILE_CREATE_TASK_MUTATION = `
  mutation MobileCreateTask($input: CreateTaskInput!) {
    createTask(input: $input) {
      id
    }
  }
`;
