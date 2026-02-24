# Spécification Technique

## Vue d'ensemble

Ce document décrit les spécifications techniques du projet **Aggregated Plan**.

> **Note** : Ce document doit être maintenu à jour au fur et à mesure de l'évolution du projet.

## Architecture générale

### Monorepo avec pnpm workspace

Le projet est organisé en monorepo utilisant pnpm workspace pour gérer les dépendances et les scripts de manière centralisée.

### Structure des applications

#### Frontend

- **Framework** : React 18 avec Vite
- **Architecture** : DDD (Domain Driven Design)
- **Layers** :
  - `domain/` : Logique métier pure
  - `application/` : Use cases et orchestration
  - `infrastructure/` : Adaptateurs externes (API, storage)
  - `presentation/` : Composants React et UI

#### Backend

- **Framework** : Hono
- **Architecture** : DDD (Domain Driven Design)
- **Layers** :
  - `domain/` : Logique métier pure
  - `application/` : Use cases et orchestration
  - `infrastructure/` : Adaptateurs (DB, HTTP clients)

##### Implémentation MVP (en cours)

- **Persistance** : Référentiels PostgreSQL (Drizzle ORM) avec sélection par configuration
  (`PERSISTENCE_KIND=in-memory|postgres`). Les référentiels en mémoire restent disponibles
  pour les tests rapides.
- **API HTTP** :
  - `GET /projects`, `POST /projects`, `PUT /projects/:id`, `DELETE /projects/:id`
  - `GET /developers`, `POST /developers`
  - `GET /assignments`, `POST /assignments`
  - `POST /allocations`
  - `GET /conflicts`
  - `GET /availabilities`, `POST /availabilities`
- **Validation** : Parsing des payloads avec Zod avant passage aux use cases.

#### Packages partagés

- `@aggregated-plan/shared-types` : Types TypeScript partagés
- `@aggregated-plan/shared-utils` : Utilitaires fonctionnels

## Stack technique

### Langages et outils

- **TypeScript** : 5.3.3
- **Node.js** : >= 18.0.0
- **pnpm** : >= 8.0.0

### Frontend

- React 18.2.0
- Vite 5.0.8
- Jest 29.7.0
- React Testing Library 14.1.2
- ESLint 8.56.0
- Prettier

### Backend

- Hono 3.12.0
- Drizzle ORM (PostgreSQL)
- Zod 3.22.4 (validation)
- Jest 29.7.0
- ESLint 8.56.0
- Prettier

## Principes de développement

### Typage strict

- Configuration TypeScript avec tous les flags strict activés
- Interdiction explicite de `any`
- Utilisation de `unknown` avec type guards si nécessaire

### Paradigme fonctionnel

- Fonctions pures privilégiées
- Immutabilité
- Composition de fonctions
- Pas de mutations directes

### Types uniquement

- Utilisation exclusive de `type` et `interface`
- Pas de classes
- Pas d'héritage

### Test Driven Development (TDD)

- Tests écrits avant le code
- Coverage minimum : 80%
- Tests unitaires et d'intégration

### Domain Driven Design (DDD)

- Séparation claire des couches
- Domain isolé des dépendances externes
- Application pour l'orchestration
- Infrastructure pour les implémentations concrètes

## Configuration

### TypeScript

Configuration de base partagée dans `tsconfig.json` à la racine, étendue par chaque package.

### ESLint

Règles strictes :
- `@typescript-eslint/no-explicit-any: error`
- `@typescript-eslint/no-unsafe-*: error`
- `prefer-const: error`
- `no-var: error`

### Prettier

Configuration uniforme pour tous les packages.

### Jest

- Coverage threshold : 80% pour branches, functions, lines, statements
- Configuration par package avec paths mapping

### Configuration multi-environnements

- Variables disponibles : `APP_ENV`, `PERSISTENCE_KIND`, `DB_HOST`, `DB_PORT`,
  `DB_NAME`, `DB_USER`, `DB_PASSWORD`, `DB_SSL`, `DB_MAX_CONNECTIONS`,
  `DATABASE_URL` (optionnel).
- Fichiers d’exemple : `config/env/example.env`, `config/env/local.env`,
  `config/env/dev.env`, `config/env/test.env`.

## Dépendances partagées

Les dépendances communes sont définies au niveau root pour éviter les duplications :
- `typescript`
- `@types/node`

## Scripts

### Workspace

- `dev` : Démarre frontend et backend
- `build` : Build tous les packages
- `test` : Tests récursifs
- `lint` : Linting récursif
- `type-check` : Vérification de types récursive

## Design Patterns utilisés

- **Factory** : Création d'objets complexes
- **Repository** : Abstraction de la persistance
- **Strategy** : Algorithmes interchangeables
- **Adapter** : Adaptation d'interfaces externes

## Sécurité

[À compléter avec les mesures de sécurité]

## Performance

[À compléter avec les optimisations de performance]

## Déploiement

### Infra locale (Docker/Podman)

- Compose compatible Docker/Podman : `infra/compose/compose.yaml`
- Démarrage base locale :
  - `docker compose --env-file config/env/local.env -f infra/compose/compose.yaml up -d db`
- Migrations :
  - `pnpm --filter backend db:migrate`

### Migrations

- SQL versionné dans `backend/drizzle/`
- Génération via Drizzle Kit (`pnpm --filter backend db:generate`)

## Historique des modifications

| Date | Version | Auteur | Description |
|------|---------|--------|-------------|
| [Date] | 1.0.0 | [Auteur] | Création du document |
