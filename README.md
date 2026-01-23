# Aggregated Plan

Projet TypeScript avec architecture DDD (Domain Driven Design), TDD (Test Driven Development) et paradigme fonctionnel.

## 🏗️ Architecture

Ce projet utilise une architecture monorepo avec pnpm workspace, séparant le frontend et le backend en deux applications distinctes.

### Structure du projet

```
aggregated_plan/
├── frontend/          # Application React avec Vite
├── backend/           # API Hono (programmation fonctionnelle)
├── packages/          # Packages partagés
│   ├── shared-types/  # Types TypeScript partagés
│   └── shared-utils/  # Utilitaires fonctionnels
├── .cursorrules       # Règles de développement Cursor
├── pnpm-workspace.yaml
├── package.json
├── tsconfig.json
├── README.md
├── SPEC_FONCTIONNELLE.md
└── SPEC_TECHNIQUE.md
```

## 🚀 Démarrage rapide

### Prérequis

- Node.js >= 18.0.0
- pnpm >= 8.0.0

### Installation

```bash
pnpm install
```

### Développement

```bash
# Démarrer frontend et backend en parallèle
pnpm dev

# Ou séparément
pnpm --filter frontend dev
pnpm --filter backend dev
```

### Build

```bash
pnpm build
```

### Tests

```bash
# Tous les tests
pnpm test

# En mode watch
pnpm test:watch

# Avec coverage
pnpm --filter frontend test:coverage
pnpm --filter backend test:coverage
```

### Linting

```bash
pnpm lint
```

### Vérification de types

```bash
pnpm type-check
```

## 📋 Principes de développement

### Typage strict

- **JAMAIS** utiliser `any`. Toujours typer explicitement.
- Utiliser `unknown` si le type est vraiment inconnu, puis valider avec type guards.
- Tous les flags strict de TypeScript sont activés.

### Paradigme fonctionnel

- Privilégier les fonctions pures (pas d'effets de bord).
- Utiliser l'immutabilité : ne jamais muter directement les objets/tableaux.
- Préférer `const` à `let`, éviter `var`.
- Utiliser la composition de fonctions plutôt que l'héritage.
- Utiliser `map`, `filter`, `reduce` plutôt que les boucles impératives.

### Types uniquement, pas de classes

- Utiliser `type` et `interface` uniquement.
- Pas de classes, pas de `new`, pas d'héritage.
- Pour les factories, utiliser des fonctions qui retournent des objets typés.

### Test Driven Development (TDD)

- **TOUJOURS** écrire les tests AVANT le code de production.
- Structure : Red → Green → Refactor.
- Coverage minimum : 80%.

### Domain Driven Design (DDD)

- **Domain** : logique métier pure, pas de dépendances externes.
- **Application** : orchestration, use cases.
- **Infrastructure** : implémentations concrètes (DB, HTTP, etc.).
- **Presentation** : UI, API routes.

## 📚 Documentation

- [Spécification fonctionnelle](./SPEC_FONCTIONNELLE.md)
- [Spécification technique](./SPEC_TECHNIQUE.md)

## 🛠️ Technologies

### Frontend

- React 18
- Vite
- TypeScript
- Jest + React Testing Library
- ESLint + Prettier

### Backend

- Hono (framework fonctionnel et rapide)
- TypeScript
- Zod (validation fonctionnelle)
- Jest
- ESLint + Prettier

### Outils

- pnpm (gestionnaire de paquets)
- TypeScript (langage)
- ESLint (linting)
- Prettier (formatage)
- Jest (tests)

## 📝 Scripts disponibles

### Workspace root

- `pnpm dev` : Démarre frontend et backend en parallèle
- `pnpm build` : Build tous les packages
- `pnpm test` : Lance tous les tests
- `pnpm lint` : Lint tous les packages
- `pnpm type-check` : Vérifie les types partout

### Frontend

- `pnpm --filter frontend dev` : Serveur de développement
- `pnpm --filter frontend build` : Build de production
- `pnpm --filter frontend test` : Tests
- `pnpm --filter frontend lint` : Linting

### Backend

- `pnpm --filter backend dev` : Serveur de développement
- `pnpm --filter backend build` : Build de production
- `pnpm --filter backend start` : Serveur de production
- `pnpm --filter backend test` : Tests
- `pnpm --filter backend lint` : Linting

## 🤝 Contribution

1. Écrire les tests en premier (TDD)
2. Respecter l'architecture DDD
3. Utiliser uniquement des types, pas de classes
4. Privilégier le paradigme fonctionnel
5. Maintenir la documentation à jour

## 📄 Licence

[À définir]
