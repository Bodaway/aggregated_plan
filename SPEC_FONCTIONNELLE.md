# Spécification Fonctionnelle — Aggregated Plan v2

## Table des matières

1. [Contexte et enjeux](#1-contexte-et-enjeux)
2. [Objectifs du projet](#2-objectifs-du-projet)
3. [Périmètre](#3-périmètre)
4. [Utilisateurs et rôles](#4-utilisateurs-et-rôles)
5. [Parcours utilisateurs principaux](#5-parcours-utilisateurs-principaux)
6. [User stories / Besoins détaillés](#6-user-stories--besoins-détaillés)
7. [Règles métier](#7-règles-métier)
8. [Données et informations manipulées](#8-données-et-informations-manipulées)
9. [Cas particuliers / Cas limites](#9-cas-particuliers--cas-limites)
10. [Exigences non fonctionnelles](#10-exigences-non-fonctionnelles)
11. [Hypothèses et points ouverts](#11-hypothèses-et-points-ouverts)
12. [Glossaire](#12-glossaire)

---

## 1. Contexte et enjeux

### 1.1 Description du contexte

**Aggregated Plan** est un cockpit personnel destiné à un Tech Lead qui gère simultanément 4 à 8 projets de développement logiciel avec un périmètre de 5 à 15 personnes.

Au quotidien, le Tech Lead utilise 5 outils différents pour gérer son activité et celle de son équipe :

| Outil | Usage actuel | Type de données |
|-------|-------------|-----------------|
| **Jira** | Gestion de tickets / tâches de développement | Tâches (personnelles + équipe) |
| **Microsoft Outlook / Exchange** | Calendrier, réunions, indisponibilités | Événements, créneaux occupés |
| **Excel sur SharePoint** | Project log, planning projets, tâches équipe | Planning, tâches, affectations |
| **Obsidian** | Notes de suivi projet, tâches personnelles | Notes markdown, tâches |
| **Microsoft Teams** | Communication d'équipe | Contexte, échanges |

### 1.2 Problèmes actuels

1. **Dispersion des informations** : les tâches, plannings et informations projet sont éparpillés dans 5 outils distincts sans vue consolidée. Le Tech Lead doit naviguer entre tous ces outils pour reconstituer une image complète.

2. **Priorisation difficile** : sans vue unifiée, il est impossible de déterminer rapidement quelles tâches sont les plus importantes ou les plus urgentes parmi toutes les sources.

3. **Charge de travail opaque** : le Tech Lead n'a pas de visibilité claire sur sa propre charge de travail. Il ne sait pas facilement s'il est surchargé ou s'il a de la marge.

4. **Suivi d'équipe fragmenté** : les informations sur les affectations et l'avancement des membres de l'équipe sont réparties entre Jira et Excel, sans vue consolidée.

5. **Redondance entre sources** : certaines tâches apparaissent à la fois dans Jira et dans l'Excel SharePoint (avec un lien inconstant — parfois le numéro de ticket Jira est présent dans l'Excel, parfois non), créant confusion et risque de double-comptage.

6. **Reporting chronophage** : reconstituer ce qui a été fait pendant la semaine nécessite de croiser manuellement les informations de plusieurs sources.

### 1.3 Enjeux

- **Efficacité personnelle** : le Tech Lead perd un temps significatif à naviguer entre les outils et à reconstituer l'information
- **Qualité de décision** : sans vue d'ensemble, les décisions de priorisation et de staffing sont prises avec une information incomplète
- **Prévention de la surcharge** : les dépassements de charge ne sont détectés qu'après coup
- **Traçabilité de l'activité** : pas de journal d'activité automatique permettant un reporting facile

---

## 2. Objectifs du projet

### 2.1 Objectifs fonctionnels

| # | Objectif | Description |
|---|----------|-------------|
| O1 | **Agréger automatiquement** | Centraliser les tâches et le planning depuis Jira, Outlook, Excel SharePoint dans une vue unique |
| O2 | **Offrir un dashboard quotidien** | Chaque matin, présenter une vue claire des tâches du jour, des réunions, de la charge de la semaine et des alertes |
| O3 | **Permettre la priorisation** | Matrice impact/urgence avec calcul hybride (auto-calculé depuis les échéances + ajustement manuel) |
| O4 | **Suivre l'activité en temps réel** | Journal d'activité alimenté par des micro-interactions tout au long de la journée |
| O5 | **Détecter proactivement les problèmes** | Alertes automatiques pour les conflits, surcharges, retards et deadlines proches |
| O6 | **Suivre l'activité de l'équipe** | Vue consolidée des affectations, avancement et disponibilités de l'équipe |
| O7 | **Offrir une vue par projet** | Regrouper toutes les informations d'un projet (tâches, planning, notes, réunions) dans une seule vue |

### 2.2 Indicateurs de succès

| Indicateur | Cible |
|-----------|-------|
| Nombre d'outils à consulter pour avoir une vue complète de la journée | 1 (au lieu de 5) |
| Temps pour identifier la tâche prioritaire | < 30 secondes |
| Délai de détection d'une surcharge | Proactif (alerte avant que ça arrive) |
| Temps de génération d'un résumé hebdomadaire | Automatique (0 effort manuel) |
| Fréquence de consultation de l'outil | Plusieurs fois par jour (outil central) |

---

## 3. Périmètre

### 3.1 Dans le périmètre — MVP v1

| Fonctionnalité | Description |
|----------------|-------------|
| Agrégation Jira | Import automatique des tâches via API Jira (les siennes + celles de l'équipe) |
| Agrégation Outlook | Import automatique du calendrier via Microsoft Graph API |
| Agrégation Excel | Lecture du fichier Excel sur SharePoint via Microsoft Graph API |
| Vue quotidienne | Dashboard du matin : tâches du jour, réunions, charge semaine, alertes |
| Priorisation hybride | Matrice impact/urgence avec calcul auto + ajustement manuel |
| Tâches personnelles | Création et gestion de tâches propres (actions, rappels, follow-ups, technique hors Jira) |
| Notes markdown locales | Champ `notes` markdown attaché à chaque tâche, indépendant de la description Jira et **préservé à chaque synchronisation** |
| Alertes intelligentes | Détection de deadlines proches, surcharge, conflits de planning, retards |
| Dédoublonnage | Réconciliation des tâches présentes dans Jira ET Excel |
| Suivi d'activité | Journal d'activité par micro-interactions (sélection de la tâche en cours) |
| Note rapide depuis le timer | Quand une activité est en cours et liée à une tâche, un champ de saisie sous le timer permet d'ajouter en un Entrée une ligne horodatée aux `notes` de la tâche |
| Interface ligne de commande | Binaire `aplan` (clavier-first) qui s'adresse à l'API GraphQL locale et expose toutes les actions du cockpit, avec un accent particulier sur le parcours rapide : `aplan start <tâche>`, `aplan note "..."`, `aplan status in_progress`, `aplan done`, `aplan stop`, `aplan current`. Une *skill* Claude Code dédiée (`.claude/skills/aplan/SKILL.md`) permet à un assistant Claude de piloter le cockpit via cette CLI plutôt qu'en formulant des requêtes GraphQL. |
| Persistance hybride | Données propres en base locale + cache synchronisé pour les données agrégées |

### 3.2 Dans le périmètre — v2

| Fonctionnalité | Description |
|----------------|-------------|
| Suivi équipe | Vue consolidée : qui fait quoi, avancement, disponibilités |
| Vue projet | Toutes les infos d'un projet regroupées (tâches Jira, planning Excel, notes Obsidian, réunions) |
| Intégration Obsidian | Parsing des fichiers .md locaux avec convention de tags pour extraire les tâches |
| Tableau de bord charge/projet | Tâches ouvertes, charge restante, ratio capacité/charge par projet |
| Rétrospective hebdomadaire | Résumé automatique : fait, reste, évolution de la charge |
| Tags transverses | Catégorisation personnalisée des tâches pour analyser la répartition du temps |

### 3.3 Hors périmètre

| Élément | Justification |
|---------|---------------|
| Multi-utilisateurs | L'outil est personnel, un seul utilisateur |
| Authentification utilisateur (Azure AD / SSO Teams) | Pas nécessaire pour un utilisateur unique en mode local (le flux OAuth Outlook est uniquement pour l'accès à l'API Microsoft Graph, pas pour authentifier l'utilisateur de l'outil) |
| Rôles et permissions | Un seul utilisateur = accès total |
| Intégration Teams (bot, tab) | Pas prioritaire, l'outil est un dashboard web personnel |
| Export PDF/Excel/CSV | Reporté, le reporting est d'abord pour consultation personnelle |
| Écriture vers les sources | L'outil est en lecture seule vis-à-vis de Jira, Outlook, Excel (pas de sync bidirectionnelle) |
| Application mobile | L'outil est consulté sur desktop principalement |

---

## 4. Utilisateurs et rôles

### 4.1 Utilisateur unique — Tech Lead

| Attribut | Description |
|----------|-------------|
| **Profil** | Tech Lead d'une organisation de développement logiciel |
| **Périmètre** | 4 à 8 projets simultanés, 5 à 15 personnes |
| **Outils quotidiens** | Jira, Outlook, Excel (SharePoint), Obsidian, Teams |
| **Besoins principaux** | Vue consolidée, priorisation, suivi de charge, suivi d'équipe |
| **Fréquence d'utilisation** | Plusieurs fois par jour |
| **Support** | Desktop (navigateur web) |

### 4.2 Permissions

L'utilisateur unique a accès à toutes les fonctionnalités sans restriction. Il n'y a pas de système de permissions à implémenter.

---

## 5. Parcours utilisateurs principaux

### 5.1 Parcours « Début de journée »

**Description** : Chaque matin, le Tech Lead ouvre l'outil pour avoir une vue claire de sa journée.

1. L'utilisateur ouvre l'application
2. L'outil synchronise automatiquement les données depuis les sources (Jira, Outlook, Excel)
3. Le dashboard quotidien s'affiche avec :
   - Les tâches du jour (toutes sources agrégées, dédoublonnées)
   - Les réunions de la journée (depuis Outlook)
   - La charge de la semaine en cours (visualisation graphique)
   - Les alertes actives (deadlines proches, surcharges, conflits)
4. L'utilisateur parcourt ses tâches et ajuste les priorités si nécessaire (matrice impact/urgence)
5. L'utilisateur sélectionne sa première tâche de la journée (début du suivi d'activité)

### 5.2 Parcours « Changement de contexte »

**Description** : Au cours de la journée, l'outil détecte un changement de contexte et propose au Tech Lead de mettre à jour son activité.

1. Un événement déclencheur se produit :
   - Fin d'une réunion Outlook, **ou**
   - Rappel périodique (configurable), **ou**
   - L'utilisateur clique manuellement sur "Changer de tâche"
2. L'outil affiche la liste des tâches en cours (agrégées de toutes les sources)
3. L'utilisateur sélectionne la tâche sur laquelle il va travailler
4. Le journal d'activité est mis à jour automatiquement (créneau précédent fermé, nouveau créneau ouvert)

### 5.3 Parcours « Priorisation »

**Description** : Le Tech Lead souhaite reprioriser ses tâches en utilisant la matrice impact/urgence.

1. L'utilisateur accède à la vue de priorisation (matrice impact/urgence)
2. L'outil affiche toutes ses tâches positionnées sur la matrice :
   - **Urgence** : auto-calculée depuis les échéances (Jira, milestones...), ajustable manuellement
   - **Impact** : qualifié manuellement par l'utilisateur
3. L'utilisateur ajuste le positionnement de certaines tâches si nécessaire
4. Les tâches sont automatiquement classées par quadrant : Urgent+Important → Important → Urgent → Ni l'un ni l'autre
5. La vue quotidienne reflète le nouvel ordre de priorité

### 5.4 Parcours « Consultation de la charge »

**Description** : Le Tech Lead veut savoir s'il est surchargé ou s'il a de la marge.

1. L'utilisateur accède à la vue de charge
2. L'outil affiche :
   - La charge de la semaine en cours en demi-journées (tâches planifiées vs capacité)
   - Les demi-journées libres vs occupées
   - Les réunions vs le temps de travail effectif
3. L'utilisateur peut naviguer entre les semaines pour voir l'évolution
4. Les alertes de surcharge sont mises en évidence visuellement

### 5.5 Parcours « Triage des tâches »

**Description** : À la réception de nouvelles tâches synchronisées depuis les sources externes, le Tech Lead décide lesquelles suivre activement dans l'outil.

1. L'utilisateur accède à la vue Triage
2. L'outil affiche deux colonnes :
   - **Boîte de réception (Inbox)** : toutes les tâches nouvellement synchronisées, non encore triées
   - **Suivies (Following)** : les tâches que l'utilisateur a choisi de suivre
3. L'utilisateur peut :
   - Glisser-déposer une tâche de la boîte de réception vers la colonne « Suivies » pour la suivre
   - Cliquer sur le bouton « Rejeter » (×) pour écarter une tâche de la boîte de réception
   - Utiliser le bouton « Tout suivre » pour suivre toutes les tâches de la boîte de réception d'un coup
   - Glisser-déposer une tâche suivie vers la boîte de réception pour annuler le suivi
4. Seules les tâches suivies apparaissent dans le dashboard quotidien et les vues de priorisation
5. Les tâches créées manuellement sont automatiquement marquées comme « suivies »

### 5.6 Parcours « Gestion d'une tâche personnelle »

**Description** : Le Tech Lead crée une tâche qui n'existe dans aucune source externe.

1. L'utilisateur clique sur "Nouvelle tâche"
2. Il renseigne :
   - Titre (obligatoire)
   - Description (optionnel)
   - Projet associé (optionnel, sélectionné parmi les projets connus)
   - Échéance (optionnel)
   - Impact et urgence (optionnel, valeurs par défaut)
   - Tags/catégories (optionnel)
3. La tâche apparaît dans la vue quotidienne et dans la matrice de priorisation
4. La tâche est persistée localement

### 5.7 Parcours « Suivi d'équipe » (v2)

**Description** : Le Tech Lead veut savoir qui fait quoi et repérer les problèmes de staffing.

1. L'utilisateur accède à la vue équipe
2. L'outil affiche une matrice développeur × projet avec les affectations (données Jira + Excel, dédoublonnées)
3. L'utilisateur peut filtrer par projet ou par personne
4. Les surcharges et les indisponibilités sont signalées visuellement
5. L'utilisateur peut cliquer sur un développeur pour voir le détail de ses tâches et sa charge

### 5.8 Parcours « Rétrospective hebdomadaire » (v2)

**Description** : En fin de semaine, le Tech Lead consulte le bilan automatique.

1. L'utilisateur accède à la rétrospective hebdomadaire
2. L'outil affiche un résumé généré automatiquement depuis le journal d'activité :
   - Temps passé par projet (déduit du suivi d'activité)
   - Temps passé par catégorie/tag
   - Tâches complétées vs tâches restantes
   - Évolution de la charge sur la semaine
3. L'utilisateur peut consulter le détail jour par jour
4. Le résumé est disponible pour copie/partage si nécessaire

---

## 6. User stories / Besoins détaillés

### 6.1 Agrégation des sources

#### US-001 : Import automatique depuis Jira

> En tant que Tech Lead, je veux que mes tâches Jira soient automatiquement importées dans l'outil afin de ne pas avoir à les ressaisir.

**Critères d'acceptation :**
- Les tâches assignées à l'utilisateur sont importées automatiquement
- Les tâches assignées aux membres de l'équipe sont également importées
- Les champs importés incluent : titre, description, statut, assigné, échéance, priorité Jira, projet, numéro de ticket
- La synchronisation est déclenchée à l'ouverture de l'application et périodiquement
- Les modifications dans Jira sont reflétées dans l'outil après synchronisation

**Priorité** : Must (MVP v1)

---

#### US-002 : Import automatique depuis Outlook

> En tant que Tech Lead, je veux que mon calendrier Outlook soit automatiquement importé afin de voir mes réunions et indisponibilités dans la même vue que mes tâches.

**Critères d'acceptation :**
- Les événements du calendrier de l'utilisateur sont importés via Microsoft Graph API
- Les informations importées incluent : titre, date/heure début, date/heure fin, lieu/lien Teams, participants
- Les événements sont positionnés sur le planning de la journée
- Les créneaux occupés par des réunions réduisent la capacité de travail disponible
- Les annulations et modifications sont reflétées après synchronisation

**Priorité** : Must (MVP v1)

> **Note — liste d'exclusion :** L'utilisateur peut exclure des réunions de la synchronisation en listant des titres (une entrée par ligne) dans les paramètres Outlook. La correspondance est insensible à la casse et s'effectue par sous-chaîne : une réunion est ignorée si son titre contient l'un des motifs saisis. Pratique pour les réunions récurrentes parasites (ex. « pause midi »).

---

#### US-002b : Porte d'authentification Microsoft (sign-in gate)

> En tant que Tech Lead, je veux m'authentifier avec mon compte Microsoft au démarrage de l'application afin que la synchronisation du calendrier Outlook et des fichiers Excel/SharePoint fonctionne sans intervention manuelle par la suite.

**Critères d'acceptation :**
- Au démarrage, l'application affiche une porte d'authentification (`AuthGate`) bloquant l'accès tant qu'aucune session Microsoft valide n'est détectée.
- L'écran de connexion propose un bouton « Se connecter avec Microsoft ». Un clic ouvre le flux OAuth Microsoft dans le navigateur (consentement administrateur accordé — aucune invite de consentement supplémentaire). Après authentification, l'utilisateur est redirigé automatiquement vers l'application, qui s'affiche normalement.
- **Une seule authentification couvre les deux connecteurs** : Outlook (calendrier) et Excel/SharePoint utilisent le même jeton Microsoft Graph.
- Une fois connecté, l'en-tête de l'application affiche l'adresse email du compte et un bouton « Se déconnecter ».
- Le jeton d'accès est renouvelé automatiquement en arrière-plan sans aucune action de l'utilisateur.
- Si le renouvellement échoue avec `invalid_grant` (token révoqué, mot de passe changé, etc.), les jetons stockés sont effacés et l'application retourne à l'écran de connexion (porte d'authentification affichée de nouveau).
- Cliquer sur « Se déconnecter » (mutation `signOut`) efface les jetons stockés et ramène l'utilisateur à la porte d'authentification.

**Priorité** : Must (MVP v1)

---

#### US-003 : Import automatique depuis Excel SharePoint

> En tant que Tech Lead, je veux que le fichier Excel de planning sur SharePoint soit lu automatiquement afin d'intégrer les tâches et plannings gérés par d'autres dans ma vue consolidée.

**Critères d'acceptation :**
- L'outil accède au fichier Excel sur SharePoint via Microsoft Graph API
- Le chemin et la structure du fichier sont configurables (colonnes, onglets)
- Les tâches extraites incluent au minimum : titre, assigné, projet, date/période
- L'import est en lecture seule (l'outil ne modifie jamais le fichier Excel)
- Les modifications dans l'Excel sont reflétées après synchronisation

**Priorité** : Must (MVP v1)

---

#### US-004 : Dédoublonnage Jira / Excel

> En tant que Tech Lead, je veux que les tâches présentes à la fois dans Jira et dans l'Excel soient dédoublonnées afin d'avoir une image fidèle de la charge sans double-comptage.

**Critères d'acceptation :**
- Quand un numéro de ticket Jira est présent dans l'Excel, la tâche est automatiquement fusionnée (les données Jira font foi, enrichies par les données Excel si nécessaire)
- Quand il n'y a pas de clé commune, l'outil propose un rapprochement basé sur la similarité (titre, assigné, projet) avec confirmation manuelle
- L'utilisateur peut forcer la liaison ou la séparation de deux tâches manuellement
- Le statut de dédoublonnage est visible (tâche fusionnée, tâche source unique, tâche à réconcilier)

**Priorité** : Must (MVP v1)

---

#### US-005 : Intégration Obsidian (v2)

> En tant que Tech Lead, je veux que mes tâches dans Obsidian soient extraites automatiquement afin de centraliser même mes notes personnelles.

**Critères d'acceptation :**
- L'outil parse les fichiers markdown d'un vault Obsidian configuré
- Une convention de tags est définie (ex : `#task`, `#todo`) pour identifier les tâches
- Les tâches extraites incluent : titre, fichier source, tags, statut (fait/à faire)
- L'utilisateur peut configurer les dossiers et patterns à scanner
- Le parsing respecte le format standard des checkboxes markdown : `- [ ]` et `- [x]`

**Priorité** : Should (v2)

---

#### US-006 : Synchronisation du catalogue Gryzzly (lecture seule)

> En tant que Tech Lead, je veux que le catalogue Gryzzly (projets actifs et tâches) soit synchronisé automatiquement afin de pouvoir associer mon activité à la bonne tâche Gryzzly lors de la déclaration de temps.

**Critères d'acceptation :**
- Le catalogue Gryzzly est importé en lecture seule : projets actifs et leurs tâches (l'outil ne modifie jamais Gryzzly).
- Pour chaque tâche du catalogue, sont conservés : nom de la tâche, projet associé et client (`customer_name`).
- La synchronisation est déclenchée par `forceSync` (comme les autres sources) si une clé d'API Gryzzly est configurée ; sinon la source est marquée « non configurée ».
- Le catalogue alimente la sélection d'une tâche Gryzzly lors de la déclaration d'activité (US-030) — il ne crée pas de tâches aplan.
- Une tâche Gryzzly disparue d'une synchronisation est désactivée mais jamais supprimée : une activité déjà associée à cette tâche reste résoluble.
- **Robustesse** : si une synchronisation ne retourne aucune tâche (incident transitoire de l'API), le catalogue existant est conservé tel quel (aucune désactivation en masse).

**Priorité** : Should

---

#### US-007 : Assignation d'une tâche aplan à une tâche Gryzzly

> En tant que Tech Lead, je veux associer une tâche aplan à une tâche Gryzzly du catalogue afin de préparer une future déclaration de temps.

**Critères d'acceptation :**
- L'utilisateur peut assigner une tâche aplan à une tâche Gryzzly issue du catalogue synchronisé (US-006), via la mutation `assignGryzzlyTask(taskId, gryzzlyTaskId)`.
- Lors de l'assignation, le `gryzzly_project_id` est snapshoté dans la tâche depuis le catalogue — une déclaration d'heures future n'a pas besoin que le catalogue soit à jour.
- Si la tâche Gryzzly est inconnue du catalogue, l'assignation est refusée (erreur de validation).
- L'utilisateur peut effacer l'assignation en passant `gryzzlyTaskId: null` ; les deux champs (`gryzzly_task_id` et `gryzzly_project_id`) sont mis à `null`.

**Priorité** : Should

---

### 6.2 Vue quotidienne (Dashboard)

#### US-010 : Dashboard du matin

> En tant que Tech Lead, je veux voir chaque matin un tableau de bord synthétique de ma journée afin de démarrer avec une vision claire.

**Critères d'acceptation :**
- Le dashboard s'affiche comme vue par défaut à l'ouverture de l'application
- Il contient 4 zones :
  1. **Tâches du jour** : liste des tâches à traiter, classées par priorité, toutes sources confondues
  2. **Réunions** : agenda de la journée (depuis Outlook)
  3. **Charge de la semaine** : visualisation graphique (demi-journées libres vs occupées)
  4. **Alertes** : notifications de deadlines proches, surcharges, conflits
- Les données sont synchronisées automatiquement à l'ouverture
- Un indicateur montre la date/heure de dernière synchronisation
- L'utilisateur peut forcer une re-synchronisation manuellement

**Priorité** : Must (MVP v1)

---

#### US-011 : Navigation temporelle

> En tant que Tech Lead, je veux pouvoir naviguer entre les jours et les semaines afin de planifier et anticiper.

**Critères d'acceptation :**
- L'utilisateur peut passer du jour courant à n'importe quel autre jour
- Une vue semaine est disponible montrant l'ensemble des demi-journées de la semaine
- Les données agrégées sont disponibles pour les jours futurs (tâches avec échéance, réunions planifiées)
- Le retour à "aujourd'hui" est accessible en un clic

**Priorité** : Must (MVP v1)

---

### 6.3 Priorisation

#### US-020 : Matrice impact/urgence

> En tant que Tech Lead, je veux positionner mes tâches sur une matrice impact/urgence afin de voir immédiatement ce qui est le plus important.

**Critères d'acceptation :**
- La matrice est un quadrant 2×2 : (Urgent + Important) / (Important) / (Urgent) / (Ni l'un ni l'autre)
- Chaque tâche a deux valeurs : urgence (1-4) et impact (1-4)
- L'urgence est auto-calculée selon les règles R10 à R13 (voir section Règles métier) et ajustable manuellement
- L'impact est qualifié manuellement par l'utilisateur (valeur par défaut : 2 — moyen)
- L'utilisateur peut repositionner une tâche par glisser-déposer ou par édition directe
- Les surcharges manuelles sont mémorisées et persistent lors des synchronisations suivantes

**Priorité** : Must (MVP v1)

---

#### US-021 : Tri automatique des tâches

> En tant que Tech Lead, je veux que mes tâches soient automatiquement classées par priorité afin de toujours voir en premier ce qui est le plus critique.

**Critères d'acceptation :**
- L'ordre de tri par défaut est : Urgent+Important > Important > Urgent > Autre
- À l'intérieur d'un même quadrant, le tri secondaire est par échéance la plus proche
- L'utilisateur peut modifier l'ordre manuellement (le tri manuel prévaut sur l'auto)
- Le tri s'applique dans la vue quotidienne et dans la liste de tâches

**Priorité** : Must (MVP v1)

---

### 6.4 Suivi d'activité

#### US-030 : Déclaration de l'activité en cours

> En tant que Tech Lead, je veux indiquer régulièrement sur quoi je travaille afin de construire automatiquement mon journal d'activité.

**Critères d'acceptation :**
- L'outil propose les tâches via un **sélecteur recherchable** : à l'ouverture (champ vide), il affiche le périmètre de travail suivi (tâches planifiées/échues le jour même en tête) ; dès la saisie (≥ 2 caractères), il recherche par titre sur l'ensemble des tâches (filtre `titleContains`). Évite la troncature aux N tâches les plus récemment créées.
- L'utilisateur sélectionne la tâche active en un ou deux clics
- Le journal enregistre : tâche sélectionnée, heure de début, heure de fin (quand il change)
- L'interaction doit être rapide et non-intrusive (popup léger ou barre latérale)
- Lors de la sélection, l'utilisateur peut associer la tâche à une **tâche Gryzzly** issue du catalogue synchronisé (US-006), en vue d'une future déclaration de temps.

**Priorité** : Must (MVP v1)

---

#### US-031 : Déclenchement du suivi d'activité

> En tant que Tech Lead, je veux être sollicité automatiquement au bon moment afin de ne pas oublier de mettre à jour mon activité.

**Critères d'acceptation :**
- Trois types de déclencheurs, combinés :
  1. **Après chaque réunion Outlook** : l'outil détecte la fin d'une réunion et demande sur quoi l'utilisateur travaille ensuite
  2. **Rappel périodique** : notification à intervalle configurable (par défaut : toutes les 2 heures)
  3. **Bouton manuel** : l'utilisateur peut à tout moment cliquer sur "Je change de tâche"
- Les déclencheurs sont configurables (activer/désactiver chaque type, ajuster la fréquence)
- La notification est discrète (pas de popup bloquant)

**Priorité** : Must (MVP v1)

---

#### US-032 : Journal d'activité

> En tant que Tech Lead, je veux consulter mon journal d'activité afin de savoir comment j'ai passé mon temps.

**Critères d'acceptation :**
- Le journal affiche une timeline de la journée avec les créneaux par tâche
- Chaque entrée montre : tâche, projet associé, durée, demi-journée (matin/après-midi)
- L'utilisateur peut corriger le journal a posteriori (modifier une entrée, ajouter un créneau oublié)
- Le journal est persisté localement
- Les données du journal alimentent la rétrospective hebdomadaire (US-060)

**Priorité** : Must (MVP v1)

---

#### US-033 : Création manuelle d'un créneau d'activité

> En tant que Tech Lead, je veux créer manuellement un créneau d'activité afin de rattraper un oubli ou de saisir une activité passée.

**Critères d'acceptation :**
- L'utilisateur peut créer un créneau en saisissant :
  - Date (obligatoire)
  - Heure de début (obligatoire)
  - Heure de fin (obligatoire)
  - Tâche associée (optionnel) — sélectionnée via un **sélecteur recherchable** : à l'ouverture (champ vide), il propose le périmètre de travail (tâches suivies actives, celles planifiées/échues le jour même en tête) ; dès la saisie (≥ 2 caractères), il lance une recherche par titre sur l'ensemble des tâches (filtre `titleContains`, débounce ~250 ms). Évite la troncature aux N tâches les plus récemment créées.
- L'heure de fin doit être postérieure à l'heure de début (validation bloquante)
- Le créneau créé apparaît immédiatement dans le journal d'activité

**Priorité** : Must (MVP v1)

---

#### US-034 : Modification d'un créneau d'activité existant

> En tant que Tech Lead, je veux modifier un créneau d'activité existant afin de corriger une erreur ou d'affiner les données saisies.

**Critères d'acceptation :**
- L'utilisateur peut modifier :
  - La tâche associée (peut être effacée pour dissocier la tâche)
  - L'heure de début
  - L'heure de fin
- La date est en lecture seule lors de la modification d'un créneau existant
- L'heure de fin doit être postérieure à l'heure de début (validation bloquante)
- Les modifications sont persistées et immédiatement visibles dans le journal

**Priorité** : Must (MVP v1)

---

### 6.4bis Journal de bord (Worklog)

#### US-WL : Consigner et consulter un journal de bord horodaté

> En tant que Tech Lead, je veux saisir des notes horodatées pendant que j'avance sur une tâche, et pouvoir relire l'ensemble des entrées d'une journée ou d'une tâche donnée, afin de reconstituer ce que j'ai fait et pourquoi.

**Critères d'acceptation :**
- **R-WL-01** : une entrée de worklog est toujours attachée à une tâche (pas d'entrée orpheline).
- **R-WL-02** : l'horodatage (`loggedAt`) est fixé automatiquement à l'instant de création. Il reste modifiable via une action secondaire (menu kebab → « Edit timestamp »).
- **R-WL-03** : le corps d'une entrée est en markdown, non vide après trim, et ne dépasse pas 10 000 caractères.
- **R-WL-04** : la page `/worklog` est filtrable par plage de dates (presets : Aujourd'hui, 7 derniers jours, Cette semaine, Ce mois, Personnalisé) et par tâche/projet. Les entrées sont regroupées par jour, ordre anti-chronologique.
- **R-WL-05** : supprimer une tâche supprime toutes ses entrées de worklog (cascade FK).
- **R-WL-06** : arrêter le timer d'activité avec une note rapide crée une entrée de worklog associée à la tâche courante (et n'écrit plus dans le champ `notes`).
- **R-WL-07** : dans `TaskEditSheet`, la section Worklog apparaît juste sous le champ `notes`, avec un champ d'ajout (Ctrl/Cmd+Enter pour soumettre) et la liste des dernières entrées de la tâche.
- **R-WL-08** : Claude Code journalise via le worklog horodaté (`aplan log "<texte>"`). Chaque appel crée une entrée distincte et atomique — ne pas regrouper plusieurs découvertes en un seul appel.
- **R-WL-09** : le temps est enregistré en créneaux **fermés** (granularité demi-journée) dérivés des horodatages des entrées de worklog, jamais via un créneau ouvert. Les créneaux sont matérialisés à `aplan stop`, `aplan done`, ou en fin de session (hook `SessionEnd`).
- **R-WL-10** : le fuseau horaire `aplan.timezone` (défaut `Europe/Paris`) définit les bornes de journée et de demi-journée utilisées pour dériver les créneaux à partir des horodatages UTC des entrées.
- **R-WL-11** : le lien session→tâche est le pointeur de configuration `aplan.active_task_id` (défini par `aplan start`, effacé par `aplan stop`/`aplan done`). Il n'existe aucun créneau d'activité ouvert associé à ce pointeur.
- **R-WL-12** : chaque entrée porte un **filigrane de consolidation** (`consolidatedAt`, nul par défaut) qui dit si la consolidation mémoire l'a déjà lue (US-096, R59). Ce marqueur n'est ni saisi ni affiché par l'utilisateur : il n'appartient qu'au dispositif de mémoire, et le supprimer ferait re-proposer chaque soir l'intégralité du journal.

**Priorité** : Must (MVP v1)

---

### 6.4ter Délégation de tâche

#### US-DEL : Indiquer la personne à qui une tâche est déléguée

> En tant que Tech Lead, je veux pouvoir noter à qui je délègue une tâche afin de garder une trace visible directement sur la carte sans changer d'outil.

**Critères d'acceptation :**
- **R-DEL-01** : un champ texte libre « Delegated to » est disponible dans le panneau d'édition de la tâche (`TaskEditSheet`). Vider le champ retire la délégation.
- **R-DEL-02** : le champ est purement informatif — il n'a aucune incidence sur la charge, les alertes ou la matrice de priorités.
- **R-DEL-03** : les noms déjà saisis sur les tâches de l'utilisateur alimentent automatiquement une liste de suggestions (auto-complétion). Aucune gestion de liste dans les paramètres ; tout nouveau nom saisi enrichit la liste.
- **R-DEL-04** : le champ est local et distinct de l'assigné Jira (`assignee`, lecture seule). La valeur survit aux synchronisations Jira/Excel, au même titre que `notes`.
- **R-DEL-05** : quand une tâche est déléguée, le nom du délégué est affiché sur les cartes de tâche (matrice de priorités, vue Triage), préfixé d'une flèche « → ».
- **R-DEL-06** : pour les tâches récurrentes, la délégation est par occurrence — elle n'est pas propagée au modèle de récurrence.

**Priorité** : Must (MVP v1)

---

### 6.5 Tâches personnelles

#### US-040 : Créer une tâche personnelle

> En tant que Tech Lead, je veux créer des tâches qui n'existent dans aucune source externe afin de centraliser toutes mes actions dans un seul outil.

**Critères d'acceptation :**
- L'utilisateur peut créer une tâche avec :
  - Titre (obligatoire)
  - Description (optionnel)
  - Projet associé (optionnel, sélectionné parmi les projets connus)
  - Échéance (optionnel, format date ou date+heure)
  - Impact (optionnel, défaut : 2)
  - Urgence (optionnel, défaut : auto-calculée si échéance fournie)
  - Tags/catégories (optionnel)
- La tâche apparaît dans la vue quotidienne et dans la matrice de priorisation
- La tâche est persistée localement

**Priorité** : Must (MVP v1)

---

#### US-041 : Gérer les tâches personnelles

> En tant que Tech Lead, je veux modifier, compléter ou supprimer mes tâches personnelles afin de garder ma liste à jour.

**Critères d'acceptation :**
- L'utilisateur peut modifier tous les champs d'une tâche personnelle
- L'utilisateur peut marquer une tâche comme terminée (elle disparaît de la vue quotidienne mais reste dans l'historique)
- L'utilisateur peut supprimer une tâche définitivement
- Les tâches terminées sont comptabilisées dans la rétrospective

**Priorité** : Must (MVP v1)

---

#### US-042 : Triage des tâches synchronisées

> En tant que Tech Lead, je veux trier les tâches importées depuis les sources externes afin de ne suivre activement que celles qui me concernent.

**Critères d'acceptation :**
- Une page « Triage » affiche deux colonnes : Boîte de réception (Inbox) et Suivies (Following)
- Les tâches nouvellement synchronisées arrivent en Boîte de réception par défaut
- L'utilisateur peut glisser-déposer une tâche vers la colonne Suivies (drag & drop via @dnd-kit)
- L'utilisateur peut rejeter une tâche de la boîte de réception (bouton ×, état « dismissed »)
- L'utilisateur peut annuler le suivi d'une tâche suivie (retour en boîte de réception)
- Un bouton « Tout suivre » permet de suivre toutes les tâches de la boîte de réception en une action
- Le dashboard quotidien n'affiche que les tâches suivies (état « followed »)
- Les tâches créées manuellement sont automatiquement en état « followed »
- Chaque carte de tâche affiche : clé Jira, titre, statut, assigné, échéance (si présente)

**Priorité** : Must (MVP v1)

#### US-043 : Édition de tâche via panneau latéral

> En tant que Tech Lead, je veux pouvoir éditer les champs locaux d'une tâche depuis n'importe quel écran afin de ne pas avoir à changer de contexte pour ajuster mes priorités.

**Critères d'acceptation :**
- Un clic sur n'importe quelle carte de tâche ouvre un panneau latéral (sheet) à droite
- Le panneau affiche les informations synchonisées en lecture seule (titre, statut, assigné, échéance, statut Jira)
- Les champs urgence, impact, description, heures estimées/restantes sont éditables
- Pour les tâches Jira/Excel : les champs temps Jira sont affichés en lecture seule, l'utilisateur peut définir des surcharges locales (remaining override, estimated override)
- Pour les tâches personnelles : le champ « heures estimées » est directement éditable
- Le panneau se ferme via bouton ×, touche Escape ou clic sur le backdrop
- Le glisser-déposer reste fonctionnel : un clic ouvre le panneau, un drag (>8px) initie le déplacement

**Priorité** : Must (MVP v1)

#### US-044 : Affichage du suivi temporel Jira avec surcharge locale

> En tant que Tech Lead, je veux voir les heures estimées, restantes et consommées de Jira sur chaque carte de tâche afin de suivre l'avancement temporel.

**Critères d'acceptation :**
- Chaque carte de tâche affiche une ligne de suivi temporel (heures restantes / consommées / estimées) avec barre de progression
- Les données proviennent des champs Jira `timeestimate`, `timespent`, `timeoriginalestimate`
- L'utilisateur peut surcharger localement les heures restantes et estimées via le panneau d'édition
- La surcharge locale prend priorité sur les valeurs Jira pour le calcul effectif
- Les cartes en mode compact (matrice de priorité) affichent uniquement les heures restantes effectives

**Priorité** : Must (MVP v1)

---

### 6.6 Alertes et détection

#### US-050 : Alertes de deadline

> En tant que Tech Lead, je veux être alerté quand une tâche approche de son échéance afin de ne rien laisser passer.

**Critères d'acceptation :**
- L'alerte se déclenche quand l'échéance est à J-2 ou moins (configurable)
- L'alerte est visible dans la zone "Alertes" du dashboard quotidien
- Le niveau d'alerte varie : avertissement (J-2), critique (J-0), en retard (dépassé)
- Les tâches de toutes les sources sont concernées (Jira, Excel, personnelles)

**Priorité** : Must (MVP v1)

---

#### US-051 : Alertes de surcharge

> En tant que Tech Lead, je veux être alerté quand ma charge dépasse ma capacité afin de réagir avant la surcharge.

**Critères d'acceptation :**
- L'alerte se déclenche quand le total des heures planifiées (tâches + réunions) dépasse la capacité hebdomadaire en heures
- La capacité par défaut est de 10 demi-journées par semaine (configurable)
- Les réunions Outlook comptent dans la charge
- L'alerte indique le dépassement en nombre de demi-journées

**Priorité** : Must (MVP v1)

---

#### US-052 : Alertes de conflits de planning

> En tant que Tech Lead, je veux être alerté quand une tâche planifiée entre en conflit avec une réunion afin de réorganiser mon planning.

**Critères d'acceptation :**
- Un conflit est détecté quand le créneau horaire d'une tâche chevauche celui d'une réunion Outlook
- Un conflit est détecté quand les créneaux horaires de deux tâches se chevauchent
- L'alerte indique les deux éléments en conflit et le créneau concerné
- L'utilisateur peut résoudre le conflit en déplaçant l'une des tâches

**Priorité** : Must (MVP v1)

---

### 6.7 Suivi d'équipe (v2)

#### US-060 : Vue équipe

> En tant que Tech Lead, je veux voir une vue consolidée de l'activité de mon équipe afin de repérer les problèmes de staffing.

**Critères d'acceptation :**
- La vue affiche une matrice développeur × projet avec les affectations
- Les données proviennent de Jira (tickets assignés) et Excel (planning), dédoublonnées
- L'utilisateur peut filtrer par projet, par personne, par période
- Les surcharges et indisponibilités sont signalées visuellement
- Le clic sur un développeur affiche le détail de ses tâches et sa charge

**Priorité** : Should (v2)

---

#### US-061 : Vue projet consolidée

> En tant que Tech Lead, je veux voir toutes les informations d'un projet regroupées afin d'avoir un contexte complet en un seul endroit.

**Critères d'acceptation :**
- La vue projet affiche :
  - Les tâches du projet (Jira + Excel, dédoublonnées)
  - Les réunions associées au projet (détection automatique depuis le titre de la réunion Outlook, modifiable manuellement par l'utilisateur)
  - Les notes Obsidian liées au projet (v2, si intégration Obsidian active)
  - La charge par développeur sur ce projet
- L'utilisateur peut naviguer entre les projets

**Priorité** : Should (v2)

---

#### US-062 : Rétrospective hebdomadaire

> En tant que Tech Lead, je veux consulter un bilan automatique de ma semaine afin de savoir comment j'ai réparti mon temps.

**Critères d'acceptation :**
- Le bilan est généré automatiquement à partir du journal d'activité
- Il affiche :
  - Temps passé par projet (en demi-journées et en %)
  - Temps passé par catégorie/tag
  - Tâches complétées vs tâches restantes
  - Évolution de la charge sur la semaine
- L'utilisateur peut consulter le détail jour par jour
- Les données sont disponibles pour les semaines précédentes (historique)

**Priorité** : Should (v2)

---

#### US-063 : Tableau de bord charge par projet

> En tant que Tech Lead, je veux voir la charge et l'avancement par projet afin d'identifier les projets en difficulté.

**Critères d'acceptation :**
- Pour chaque projet, l'outil affiche :
  - Nombre de tâches ouvertes
  - Charge estimée restante (en demi-journées, si l'estimation est disponible dans Jira)
  - Ratio capacité/charge (si les affectations sont connues)
- Les projets en alerte (retard, surcharge) sont mis en évidence
- Le tableau est triable par charge, par nombre de tâches, par projet

**Priorité** : Should (v2)

---

#### US-064 : Tags et catégories transverses

> En tant que Tech Lead, je veux taguer mes tâches avec des catégories personnalisées afin d'analyser la répartition de mon temps.

**Critères d'acceptation :**
- L'utilisateur peut créer des catégories/tags personnalisés (ex : #revue-code, #architecture, #support, #réunion, #admin)
- Un tag peut être associé à n'importe quelle tâche (importée ou personnelle)
- Les tags sont utilisés dans la rétrospective hebdomadaire pour ventiler le temps
- Les tags sont persistés localement

**Priorité** : Must (MVP v1)

### 6.8 Recherche globale

#### US-070 : Barre de recherche globale

> En tant que Tech Lead, je veux pouvoir rechercher une tâche instantanément depuis n'importe quel écran afin de l'ouvrir ou de la localiser sans changer de vue.

**Critères d'acceptation :**
- Une barre de recherche est visible en permanence dans le Header de l'application.
- Les raccourcis `/` et `Cmd/Ctrl+K` permettent de mettre le focus sur la barre de recherche depuis n'importe où dans l'interface. La touche `Esc` efface la saisie et retire le focus.
- À partir de 2 caractères saisis, les tâches correspondantes sur l'écran courant sont mises en évidence (anneau bleu) et les tâches non correspondantes sont atténuées visuellement.
- Un menu déroulant affiche les meilleures correspondances ; cliquer sur une suggestion ouvre le panneau d'édition de la tâche sans navigation d'écran.
- La recherche est floue (fuzzy) et porte sur : le titre, la clé Jira (`sourceId`), les tags, le projet, l'assigné et la description.
- Les tâches écartées (état `dismissed`) sont exclues des résultats.

**Priorité** : Must (MVP v1)

### 6.9 Feuille de temps (Timesheet) et déclaration d'activité pour Gryzzly

#### US-080 : Consultation et réconciliation de la feuille de temps

> En tant que Tech Lead, je veux consulter et réconcilier ma feuille de temps journalière (reconstruction à partir du journal d'activité) avant de la déclarer à Gryzzly afin de m'assurer que le temps est correctement affecté par projet.

**Critères d'acceptation :**
- L'utilisateur peut afficher la feuille de temps d'un jour donné via `aplan timesheet [--date YYYY-MM-DD]` (par défaut : aujourd'hui).
- La feuille de temps affiche :
  - Un **tableau heures×projet** listant chaque projet Gryzzly avec les heures affectées (heures épinglées depuis le journal d'activité et heures non attribuées)
  - Une **chronologie ASCII** montrant les blocs d'activité de la journée (réunions, créneaux d'activité, créneaux non trackés)
  - Un **récapitulatif** : heures totales, heures non attribuées, confiance globale (HIGH/MEDIUM/LOW)
  - Les **blocs non résolus** : créneaux sans affectation projet clair, avec suggestion basée sur les apprentissages précédents
- Les sous-commandes permettent d'éditer la feuille de temps **sans REPL interactif** (voir ci-dessous) :
  - `aplan timesheet set <projet> <heures>` — épingle des heures pour un projet (surcharge le calcul auto)
  - `aplan timesheet validate [--date YYYY-MM-DD]` — valide la feuille de temps
  - `aplan timesheet off [--am|--pm] [--date YYYY-MM-DD]` — marque une demi-journée (ou la journée complète) comme indisponible
- La feuille de temps est sauvegardée automatiquement lors des modifications.

**Décision d'architecture — CLI flag-driven (pas de REPL) :**

La CLI de timesheet est **flag-driven** : chaque édition se fait via une sous-commande explicite et synchrone (`set`, `off`, `validate`), avec résultat immédiat. Il n'existe **aucun mode REPL ou interactif** pour éditer la feuille de temps en ligne.

*Justification :* L'édition interactive riche (glisser-déposer de blocs, suggestions en temps réel, interface timeline visuelle) relève de l'écran React dédiée au timesheet (Surface B, Plan 3). La CLI sert les cas d'usage non-interactifs : automatisation, scripting Claude, appels par workflows externes. Les deux interfaces opèrent sur le même modèle de données GraphQL.

**Priorité** : Should (v2)

#### US-081 : Apprentissage de règles de mappage (Mapping)

> En tant que Tech Lead, je veux enseigner au système à associer automatiquement certains contextes (dépôt git, branche, organisateur de réunion, etc.) à un projet Gryzzly spécifique afin d'accélérer la déclaration de temps.

**Critères d'acceptation :**
- L'utilisateur peut créer une règle de mappage via `aplan map add <kind> <pattern> [--branch <pattern>] <gryzzly-project-id>` où :
  - `<kind>` : type de sélecteur (`repository`, `subject`, `organizer`, `internal_project`)
  - `<pattern>` : motif à matcher (regex ou substring selon le kind)
  - `--branch <pattern>` : motif de branche (pour kind=`repository` uniquement)
  - `<gryzzly-project-id>` : identifiant Gryzzly cible
- L'utilisateur peut consulter les règles existantes via `aplan map list [--kind <kind>]`
- Les règles apprises sont persistées et utilisées pour suggérer des affectations lors de la reconstruction de feuille de temps.
- Lors du mappage, les signaux sont priorisés dans cet ordre (premier match gagne) : (1) dépôt (`repository`), (2) sujet de réunion (`subject`), (3) organisateur (`organizer`), (4) projet interne/aplan (`internal_project`). **Note importante** : `aplan map add` ne rejette pas encore de sélecteurs simultanés — cette logique de priorité sera affinée dans une future itération.

**Priorité** : Should (v2)

---

#### US-082 : Reconstruction automatique en fin de journée

> En tant que Tech Lead, je veux que ma feuille de temps soit automatiquement reconstruite à partir de mon journal d'activité (worklog) en fin de journée afin de lancer facilement ma déclaration d'heures.

**Critères d'acceptation :**
- **Déclenchement :** chaque jour à l'heure configurée (défaut : 18h00, fuseau horaire `aplan.timezone`), le système reconstruit automatiquement le brouillon de feuille de temps du jour.
- **Garde contre les écrasements :** si la feuille de temps du jour a déjà un statut `Validée` ou `Soumise`, aucune reconstruction n'a lieu (le brouillon finalisé n'est jamais écrasé).
- **Reconstruction idempotente :** le système utilise un watermark (`aplan.timesheet.last_auto_run`) pour éviter les exécutions redondantes. Un jour ne peut être reconstruit qu'une seule fois automatiquement.
- **Fenêtre de rattrapage :** si le watermark est absent (premier lancement) ou remonte à plus de 7 jours, la reconstruction s'exécute pour les 7 derniers jours (bouchon configurable à 7 jours, non dépassé).
- **Alerte passive :** une fois la reconstruction complétée, une alerte passive « Brouillon de feuille de temps prêt » (`TimesheetReady`, sévérité `Information`) est levée. Elle apparaît dans la zone des alertes du dashboard et est accessible via la requête `alerts`. Il n'existe aucune notification OS/push — l'alerte est consultée in-app via l'écran `/timesheet` ou le dashboard.
- **Heure de déclenchement configurable :** accessible via la clé de configuration `workday.auto_reconstruct_hour` (0–23, défaut 18).

**Priorité** : Should

---

#### US-083 : Écran interactif de revue et édition de feuille de temps

> En tant que Tech Lead, je veux revoir visuellement ma feuille de temps du jour (répartition des heures par projet, calendrier des réunions et blocs de travail) et la modifier avant de la valider, afin de maîtriser exactement comment je déclare mon temps à Gryzzly.

**Critères d'acceptation :**
- L'écran `/timesheet` est accessible depuis la navigation principale et affiche la revue d'une journée (sélectionnable via navigation temporelle avec boutons « jour précédent / aujourd'hui / jour suivant »).
- **Zone 1 — Chronologie (Timeline) :**
  - Affiche les événements de la journée sur un axe temporel horizontal, divisé en deux demi-journées (matin 08:00–12:00, après-midi 13:00–17:00).
  - Les réunions Outlook (importées) sont affichées en grisé hachuré (pattern visuel indicant « verrouillé »), non éditables.
  - Les blocs de travail (créneaux d'activité du journal) sont affichés en rectangles colorés, la couleur déterminée par le projet Gryzzly associé (palette stable, même code couleur qu'ailleurs dans l'outil).
  - Les créneaux non affectés à un projet sont grisés (« hors bureau »).
  - La timeline est **en lecture seule** : aucune édition par glisser-déposer, aucun clic pour modifier un bloc. Les modifications se font exclusivement via le sidebar.
  - Les espaces libres dans la timeline sont visuellement apparents (arrière-plan blanc/clair).

- **Zone 2 — Sidebar « Heures par projet » (Hours by Project) :**
  - Liste chaque projet Gryzzly avec ses heures affectées ce jour, plus une ligne « Non attribué » en bas (surlignée en amber).
  - Chaque ligne affiche : pastille de couleur (code projet) + **sélecteur de projet Gryzzly** (liste déroulante) + champ numérique d'heures **directement éditable en ligne** (sans bouton « Éditer ») + indicateur ▲ si confiance basse.
  - Le champ numérique accepte les décimales (pas de valeur négative).
  - **Sélecteur de projet (réaffectation) :** chaque ligne — en particulier la ligne « Non attribué » — porte une liste déroulante permettant de réaffecter ses heures à un projet Gryzzly du catalogue (option « — Non attribué — » pour repasser en non-attribué). Les options proviennent du catalogue Gryzzly (`gryzzlyTasks`, dédoublonné par projet, libellé « projet — client »). Réaffecter une ligne la marque comme éditée : à l'enregistrement, elle est **épinglée** (`isPinned = true`). Lors de l'enregistrement, les lignes visant le même projet sont **fusionnées** (heures additionnées), et toutes les lignes non-attribuées sont regroupées en une seule.
  - Sous les lignes de projets, affichage du **total des heures** vs **cible configurable** (défaut : 7,5 h) :
    - Badge « ✓ balanced » en vert si le total égale la cible (à ±0.01 h près).
    - Badge montrant l'écart signé (ex. « +1.50h », « -0.50h ») en orange sinon.
  - Affichage du **statut** (DRAFT, VALIDATED, SUBMITTED, DAY_OFF) en petit badge en haut du sidebar.

- **Actions disponibles (Boutons en bas du sidebar, masqués si jour validé/soumis) :**

  1. **Enregistrer** (« Save »)
     - Enregistre les valeurs d'heures saisies dans le sidebar pour ce jour.
     - Les lignes modifiées (heures **ou** projet réaffecté) reçoivent le marqueur « épinglé » (`isPinned = true`) : elles conservent les valeurs manuelles lors d'une reconstruction future.
     - Les changements ne sont persistés que lors d'un clic sur « Save » — **il n'y a pas de sauvegarde automatique**.
     - **Erreurs d'enregistrement affichées :** si le backend refuse l'enregistrement (ex. total des heures épinglées > cible journalière — message « pinned hours (X) exceed the daily target (Y) »), le message est affiché en rouge dans le sidebar et **aucune donnée n'est écrasée** (les saisies locales sont conservées). Auparavant, ces erreurs étaient avalées silencieusement.

  2. **Valider & Verrouiller** (« Validate & lock »)
     - Valide la feuille de temps du jour (statut → `VALIDATED`) et la verrouille.
     - Une fois verrouillée, la timeline et le sidebar deviennent inéditables (tous les champs désactivés).
     - Les boutons « Save », « Refresh from signals » et « Day off » sont masqués.
     - Un badge « VALIDATED » apparaît en vert dans le header du sidebar.
     - Note limitée : une journée verrouillée ne peut pas être réouverte depuis l'UI actuelle (futur : mutation `reopenTimesheet`).

  3. **Reconstruire depuis les signaux** (« Refresh from signals »)
     - Demande une confirmation : « Reconstruct from signals? This overwrites unsaved manual edits for this day. »
     - Re-exécute la logique de reconstruction : relit le journal d'activité et les règles de mappage, réaffecte les heures aux projets Gryzzly.
     - Déverrouille les heures épinglées et remplit les affectations depuis la logique par défaut.
     - Utile après avoir ajouté des règles de mappage ou pour recommencer après des éditions incorrectes.
     - Non disponible si le jour est validé.

  4. **Jour off** (« Day off »)
     - Marque la journée entière comme indisponible (statut → `DAY_OFF`, scope `FULL`).
     - Le statut passe à `DAY_OFF` (badge dans le header) et la journée est enregistrée à 0 h : timeline vide, aucune ligne projet. La timeline et le sidebar restent affichés (pas de masquage dédié dans cette version).
     - Note limitée : `scope` est toujours `FULL` (demi-journée AM/PM non encore supportées).

- **Comportement de navigation et édition :**
  - Les modifications saisies dans le sidebar (champs numérique) restent **en mémoire locale** jusqu'à un clic sur « Save ». Aucune sauvegarde implicite n'a lieu.
  - La navigation temporelle (jour précédent/suivant) **ne sauvegarde pas** les éditions en cours : changer de jour sans avoir cliqué « Save » **perd les modifications locales**.
  - Après un clic sur « Save », les données sont persistées et les marqueurs d'édition sont nettoyés.

- **En-tête du jour :**
  - Affiche la date sélectionnée, boutons de navigation, et un bouton de rafraîchissement.
  - Affiche également le statut du jour (DRAFT, VALIDATED, SUBMITTED, DAY_OFF) sous forme de badge.

- **Intégration avec le CLI `aplan timesheet` (Surface A) :**
  - La représentation visuelle est le pendant interactif du CLI. Les commandes `aplan timesheet set <projet> <heures>` et `aplan timesheet off` modifient la même base de données.
  - Une édition via le CLI est immédiatement reflétée si l'écran `/timesheet` était ouvert (optionnel : polling ou SSE abonnement).

**Limitations actuelles (documentées pour v2+) :**
- **Scope demi-journée non appliqué** : le bouton « Jour off » marque la journée *entière* comme indisponible (`FULL`), pas la possibilité de marquer juste le matin ou l'après-midi. Voir R-11.4.
- **Pas de ré-ouverture depuis l'UI** : une journée validée (`VALIDATED`) ne peut être rouverte que via une future mutation `reopenTimesheet` (en dehors du périmètre Plan 3). Un utilisateur qui valide par erreur doit utiliser la CLI ou la base de données pour annuler.
- **Pas d'édition par glisser-déposer** : l'assignation des heures se fait exclusivement via édition numérique inline dans le sidebar. La timeline est en lecture seule.
- **Réaffectation non durable entre les jours** : le sélecteur de projet mémorise le choix dans le **brouillon du jour** uniquement (persistant aux rechargements, mais pas aux jours suivants ni à un « Refresh from signals »). Rendre l'attribution durable exige de rattacher le projet à la **tâche** source ; or la ligne « Non attribué » ne conserve pas ses `source_refs`, donc l'écran ne peut pas remonter aux tâches. Hors périmètre de cette itération.
- **Rappel — un signal de journal (worklog) ne s'attribue à un projet que via le `gryzzly_project_id` de sa tâche** (les règles de mappage `aplan map` ne s'appliquent qu'aux commits et réunions). Une journée dont les tâches n'ont pas de projet Gryzzly est donc entièrement « Non attribué » jusqu'à réaffectation manuelle via le sélecteur.

**Priorité** : Must (Plan 3)

### 6.10 Mémoire sémantique (souvenirs)

Là où `tasks` répond à « qu'est-ce que je dois **faire** ? » et `worklog_entries` à
« qu'est-ce qui **s'est passé** ? », les **souvenirs** (`memories`) répondent à
« qu'est-ce que je dois **savoir** ? » : décisions prises, engagements pris, faits appris,
préférences exprimées. Un souvenir ne porte **jamais** d'échéance ni de statut d'avancement —
ces informations vivent sur la tâche, sous peine d'avoir deux moteurs de rappel divergents.

Un souvenir est **bi-temporel** : `occurredAt` dit quand la chose est devenue vraie,
`invalidatedAt` quand elle a cessé de l'être, et `supersededBy` par quoi elle a été remplacée.
Une décision annulée n'est pas une ligne supprimée : c'est une décision avec une fin de
validité et un successeur. Rappeler une décision périmée est le pire mode d'échec du système,
d'où un **filtre dur** par défaut : seuls les souvenirs `ACTIVE` et non invalidés sont rappelés.

#### US-090 : Enregistrer un souvenir

> En tant que Tech Lead, je veux enregistrer en une commande une décision, un engagement, un fait
> ou une préférence, afin que Claude puisse me le rappeler plus tard avec son contexte.

**Critères d'acceptation :**
- L'utilisateur enregistre un souvenir via
  `aplan remember "<titre>" [--kind decision|commitment|fact|preference] [--why "<contexte>"] [--project <projet>] [--to <personne>] [--task <tâche>] [--source-ref <réf>] [--confirm]`.
  `--kind` vaut `fact` par défaut ; `--to` est répétable.
- `--source-ref` note **d'où vient** le souvenir : un identifiant d'entrée de journal, un
  identifiant de session. Champ libre, sans clé étrangère (les entrées de journal disparaissent
  en cascade avec leur tâche, une provenance pendante est acceptée explicitement). C'est la
  consolidation planifiée (US-096) qui s'en sert : sans lui, un candidat étrange ne peut pas être
  rapproché de l'entrée de journal qui l'a produit.
- `--project` accepte un UUID **ou** un nom de projet (correspondance exacte puis
  sous-chaîne, insensible à la casse). `--task` accepte les mêmes formes que les autres
  commandes : UUID, clé Jira (`AP-123`), titre approximatif, ou `@current`.
- Aucune tâche n'est requise : une discussion d'architecture sans tâche associée produit
  quand même un souvenir (`projectId` et `taskId` sont facultatifs).
- Le souvenir atterrit dans la **file de validation** (`status = PENDING`) sauf si
  `--confirm` est passé, qui l'enregistre directement en `ACTIVE`.
- Un titre vide ou uniquement composé d'espaces est refusé. Le titre est limité à
  500 caractères, le contexte (`--why`) à 10 000.
- Le souvenir est **immédiatement retrouvable** par recherche : l'index plein texte est
  écrit dans la même transaction que le souvenir.
- `--json` émet la charge utile brute `data.remember`.

**Priorité** : Must (lot 1)

#### US-091 : Rappeler un souvenir

> En tant que Tech Lead, je veux retrouver une décision et son contexte à partir de quelques mots,
> afin de ne pas rejouer un arbitrage déjà tranché.

**Critères d'acceptation :**
- `aplan recall <id>` affiche un souvenir en détail : type, titre, contexte, date de survenue,
  statut, personnes concernées, projet et tâche rattachés. Un identifiant inconnu retourne le
  code de sortie « non trouvé » (2). `--history`, `--project` et `--limit` étant réservés à la
  recherche (le souvenir ciblé est affiché quel que soit son statut), les passer avec un
  identifiant est **refusé à l'analyse des arguments** plutôt qu'ignoré en silence.
- **L'identifiant accepte la référence courte affichée par le brief** : `m:7c1`, `[m:7c1]`, `7c1`
  ou l'UUID complet. C'est ce qui rend le forage possible depuis le brief. Un préfixe qui
  correspond à **plusieurs** souvenirs retourne le code « ambigu » (3) avec la liste des
  candidats — jamais un souvenir choisi au hasard.
- `aplan recall --q "<texte>" [--history] [--project <projet>] [--limit N]` effectue une
  recherche plein texte, résultats du plus pertinent au moins pertinent.
- **Le vocabulaire quotidien ne fait jamais échouer la recherche** : `AP-1234`,
  `Cartier : certificat`, `wave 0`, `*`, `NOT` sont acceptés tels quels — la saisie est
  convertie en requête sûre avant d'atteindre l'index.
- Les accents sont ignorés à la comparaison (`limitee` retrouve « limitée »).
- Les pluriels fonctionnent **dans les deux sens** : les mots de 4 caractères et plus sont
  étendus par préfixe (`engagement` retrouve « engagements »), et ceux de 5 caractères et plus
  finissant par `s` ou `x` reçoivent en plus une variante dépluralisée (`engagements` retrouve
  « engagement », `travaux` retrouve « travau… »). L'étoile seule ne pouvant que rallonger le
  mot saisi, les deux variantes sont nécessaires.
- **L'adjacence est respectée** : un identifiant reste une expression exacte. `AP-1234` ne
  retrouve pas un souvenir qui mentionne `AP` et `1234` à vingt mots d'intervalle.
- Tous les mots saisis restent **obligatoires** : chercher `Cartier engagements` ne retourne
  que les souvenirs contenant les deux notions, la variante dépluralisée n'élargissant jamais
  la requête aux autres mots.
- Par défaut, seuls les souvenirs `ACTIVE` et non invalidés sont retournés. `--history`
  lève ce filtre et affiche aussi les souvenirs périmés (marqués d'un ⚠) et non validés.
- Le classement combine quatre signaux : pertinence textuelle, correspondance d'entité
  (projet / tâche / personne du contexte courant), décroissance de récence sur `occurredAt`
  (demi-vie 90 jours), et poids par type (`decision` et `commitment` devant `fact` et
  `preference`).
- Une recherche sans résultat n'est pas une erreur (code de sortie 0).
- `--json` émet la charge utile brute (`data.memory` ou `data.recall`).

**Priorité** : Must (lot 1)

#### US-092 : Importer le corpus de souvenirs existant

> En tant que Tech Lead, je veux importer en une commande les notes de mémoire déjà écrites par
> le harness, afin d'avoir un corpus réel dès le premier usage plutôt qu'une base vide.

**Critères d'acceptation :**
- `aplan memory import <dossier>` importe **tous** les fichiers markdown du dossier — le nombre
  de fichiers n'est jamais figé dans le code, le corpus grossit avec le temps.
- Le type déclaré dans l'entête du fichier (`metadata.type`) détermine le type du souvenir :
  `feedback` et `user` → `preference` ; `project` et `reference` → `fact`. Un type inconnu ou
  absent retombe sur `fact` — un import ne peut jamais promouvoir une note en `decision`.
- Le titre du souvenir est la ligne `description` de l'entête (à défaut, `name`) ; le corps du
  fichier devient le contexte. La provenance est `manual`.
- Les souvenirs importés sont directement **actifs** : ce sont les notes de l'utilisateur, il n'y a
  rien à valider. Ils sont donc immédiatement rappelables.
- La date de survenue est celle de l'entête (`metadata.modified`) si présente, sinon la date de
  modification du fichier.
- **L'import est idempotent** : chaque souvenir importé retient une référence stable dérivée du
  `name` du fichier, et un fichier déjà importé est ignoré. Relancer la commande n'importe rien.
- Un fichier sans entête (le fichier d'index `MEMORY.md`, par exemple) est **ignoré avec un
  motif**, sans faire échouer l'import des autres.
- La commande **n'écrit jamais** dans le dossier : celui-ci a déjà un écrivain (le mécanisme
  d'auto-mémoire du harness), et deux écrivains sur un fichier généré divergent.
- Le rapport indique le nombre d'importés, le nombre d'ignorés et le motif de chaque exclusion.

**Priorité** : Must (lot 2)

#### US-093 : Trier les souvenirs candidats

> En tant que Tech Lead, je veux trier les candidats proposés — accepter, reformuler, réviser ou
> rejeter — afin que la mémoire ne se remplisse pas de doublons et de bruit.

**Critères d'acceptation :**
- `aplan inbox` liste les candidats en attente (`pending`).
- `aplan inbox accept <id> [--kind <type>]` valide un candidat : il devient `active` et donc
  rappelable. `--kind` permet de le retyper au passage.
- **Jamais d'ajout muet** : si le candidat ressemble à un souvenir déjà actif, l'acceptation est
  **refusée**, rien n'est écrit, et les sosies sont affichés avec les trois issues possibles —
  fusionner, superséder, ou accepter explicitement via `--force`.
- `aplan inbox merge <id> --into <id>` : *même fait, meilleure formulation*. Une seule ligne
  survit — celle de la cible, qui conserve son identité et ses dates et reçoit la nouvelle
  formulation. Les personnes concernées des deux lignes sont conservées. **Cette opération efface
  l'historique** et n'est donc pas le choix par défaut.
- `aplan inbox supersede <id> --replaces <id>` : *le fait a changé*. **Les deux lignes
  survivent** ; l'ancienne reçoit sa fin de validité et un pointeur vers la nouvelle.
- `aplan inbox reject <id>` : le candidat devient `rejected` et **la ligne est conservée** comme
  pierre tombale, afin que la consolidation ne le re-propose pas chaque soir.
- **Tous ces identifiants acceptent la référence courte affichée** (`m:7c1`, `[m:7c1]`, `7c1`) au
  même titre que l'UUID complet — y compris `--into` et `--replaces`. Le brief et l'inbox
  n'affichent qu'une référence courte : c'est le seul identifiant que l'utilisateur voit passer, et
  ces commandes sont celles qu'il lance plusieurs fois par matinée. Un identifiant qui se lit mais
  ne s'utilise pas obligerait à recopier 36 caractères à la main.
- Un préfixe qui correspond à **plusieurs** souvenirs retourne le code « ambigu » (3) et **liste
  les candidats**, sans rien écrire. L'ambiguïté est évaluée sur **l'ensemble des souvenirs**, pas
  seulement sur la file : un préfixe unique parmi les candidats du jour ne doit pas se mettre à
  désigner un autre souvenir dès qu'un candidat est accepté.
- Pour les commandes à **deux identifiants** (`merge`, `supersede`), les deux sont résolus **avant
  toute écriture** : une résolution paresseuse laisserait une opération à moitié appliquée.
- Un identifiant inconnu retourne le code de sortie « non trouvé » (2) ; une acceptation bloquée
  par un quasi-doublon retourne « précondition non satisfaite » (4).

**Priorité** : Must (lot 3)

#### US-094 : Réviser un souvenir déjà actif

> En tant que Tech Lead, je veux enregistrer qu'une décision n'est plus valable et par quoi elle a
> été remplacée, afin que Claude ne me rappelle jamais une décision annulée, tout en gardant la
> trace de ce qui avait été décidé et pourquoi cela a changé.

**Critères d'acceptation :**
- `aplan memory supersede <ancien> --by <nouveau>` révise un souvenir déjà actif, hors file.
- **Les deux identifiants acceptent la référence courte** (`m:7c1`, `7c1`) ou l'UUID complet, et
  sont **tous les deux résolus avant toute écriture** : une supersession à moitié appliquée
  masquerait un fait sans successeur. Inconnu → code 2 ; ambigu → code 3 avec la liste des
  candidats, rien n'étant écrit.
- L'ancien souvenir **disparaît du rappel** immédiatement, et **réapparaît avec `--history`**,
  marqué comme n'étant plus vrai, avec la référence de son successeur.
- Les **chaînes sont légales** : A remplacé par B, puis B remplacé par C. Chaque révision porte
  sur le souvenir actif en tête de chaîne.
- Un souvenir **ne peut pas se remplacer lui-même**, et une révision qui **refermerait un cycle**
  est refusée.
- Un souvenir **déjà invalidé ne peut pas être re-supersédé** : il a déjà un successeur, et
  écraser ce lien ferait perdre l'historique que le modèle existe précisément pour conserver. Le
  message d'erreur indique de viser la tête de chaîne.
- Un souvenir rejeté ou déjà invalidé ne peut pas devenir la nouvelle vérité.
- La supersession (en file ou hors file) est le **seul chemin** qui marque un souvenir comme
  n'étant plus vrai. Ni l'enregistrement, ni la recherche, ni aucun automatisme ne le fait.

**Priorité** : Must (lot 3)

#### US-095 : Recevoir un brief au démarrage de session

> En tant que Tech Lead, je veux qu'une session Claude démarre en connaissant mes échéances, mes
> engagements ouverts et les décisions actives du projet courant, afin de ne pas avoir à les
> rappeler moi-même à chaque fois.

**Critères d'acceptation :**
- `aplan brief [--morning] [--project <projet>] [--date AAAA-MM-JJ]` affiche, en français :
  les **échéances**, les **engagements ouverts** (avec les personnes concernées), les **décisions
  actives** du projet courant, le **nombre de candidats mémoire à trier**, et un
  **avertissement de vétusté** de la consolidation.
- **Le brief S'AJOUTE à la liste des tâches suivies de la session, il ne la remplace jamais** :
  cette liste alimente le sélecteur « Choisir une autre tâche » du hook de démarrage, et
  l'ensemble « échéances + engagements » n'est pas l'ensemble « tâches suivies ».
- **Plafond de 40 lignes**, vérifié par un test : cette sortie entre dans le contexte du modèle à
  chaque session. Chaque ligne est bornée à 140 caractères. La troncature est **toujours visible** :
  l'en-tête de section indique le total et le nombre affiché (`Échéances (8, 6 affichés) :`).
- **Chaque souvenir affiché porte sa référence courte** (`[m:7c1]`), directement réutilisable :
  `aplan recall m:7c1`. Sans référence, le brief serait une impasse. La longueur de la référence
  s'allonge automatiquement si deux souvenirs du même brief partagent un préfixe.
- **Les fixtures de test sont filtrées** (`Test uppercase kind`, `Test recurring enum`, `test`) et
  les **titres en doublon sont fusionnés** — une tâche récurrente matérialisée en 17 occurrences
  n'occupe qu'une ligne, celle dont l'échéance est la plus proche.
- Les échéances sont classées **par proximité d'aujourd'hui**, le retard passant devant à distance
  égale. Une tâche en retard de huit mois ne doit pas chasser l'échéance de la semaine.
- Les tâches `Done`, `Cancelled` et `dismissed` sont exclues ; les tâches non triées (`inbox`) sont
  **conservées** — une tâche non triée dont l'échéance est demain est précisément ce qu'il faut voir.
- Seuls les souvenirs `ACTIVE` et non invalidés apparaissent (le filtre dur de R45 s'applique aussi
  ici), et seuls les types `commitment` et `decision` : un `fact` ou une `preference` se récupère à
  la demande, pas à chaque démarrage.
- L'**avertissement de consolidation** n'apparaît qu'au-delà de 3 jours sans passage, ou si aucun
  passage n'a jamais été enregistré (« jamais exécutée »). La date du dernier passage est lue dans
  la table `configuration` ; une clé absente, illisible ou une valeur invalide ne fait **jamais
  échouer** le brief.
- `--morning` produit la variante de la notification de 8h30 : échéances **du jour** (et retards),
  engagements ouverts, candidats à trier. Ni décisions, ni rappels de commandes.
- Une base sans rien à signaler affiche « Rien à signaler. » plutôt qu'un en-tête nu.
- `--json` émet la charge utile brute `data.brief`, qui contient à la fois le rendu (`lines`) et
  les données structurées.

**Priorité** : Must (lot 4)

#### US-096 : Consolider le journal de bord en souvenirs candidats

> En tant que Tech Lead, je veux qu'une relecture quotidienne de mon journal de bord me propose les
> faits, préférences et décisions que je n'ai pas pensé à enregistrer, afin que la mémoire se
> remplisse sans que j'y pense — mais sans que rien n'y entre sans mon accord.

La consolidation elle-même est une **session Claude Code planifiée**, pas du code backend : le
backend ne contient aucun client de modèle, aucune clé d'API et aucun *prompt*. Ce qui est livré
ici est la **machinerie déterministe** que cette session pilote, plus son jeu d'instructions
(`docs/prompts/consolidation-memoire.md`), modifiable sans recompiler.

**Critères d'acceptation :**
- `aplan consolidate pending [--limit N]` liste les entrées de journal **jamais consolidées**
  (`consolidatedAt` nul), **de la plus ancienne à la plus récente**. La commande est en lecture
  seule : elle ne marque rien, et sert donc aussi de **sonde de joignabilité**.
- `aplan consolidate mark <id>…` pose le filigrane sur les entrées traitées. Idempotent : une
  entrée déjà marquée ne bouge pas et ce n'est **pas** une erreur — la sortie annonce
  `marqué/demandé` afin que l'écart soit visible. Marquer sans identifiant est refusé à l'analyse
  des arguments : un passage qui a oublié de collecter ses identifiants ne doit pas ressembler à un
  passage propre.
- `aplan consolidate record-run` enregistre la date du passage dans la table `configuration`
  (clé `memory.consolidation.last_run`), celle-là même que lit le brief (R57).
- Les trois verbes acceptent `--json`, condition pour qu'une session planifiée les pilote.
- **Le filigrane est posé APRÈS les écritures réussies.** Un doublon se rejette et devient une
  pierre tombale ; une entrée marquée qui n'a jamais produit de souvenir est perdue sans que rien ne
  le signale.
- **Si l'API n'est pas joignable, la session ne fait rien du tout** et ne pose aucun filigrane :
  l'exécution suivante rattrape l'intégralité du retard.
- Tout ce que la session propose est écrit en `PENDING` (jamais `--confirm`, jamais `--force`) :
  la porte de validation humaine est le seul chemin vers `ACTIVE`.
- La session **ne re-propose pas** ce qui est déjà `ACTIVE`, `PENDING` ou `REJECTED` — les pierres
  tombales existent précisément pour faire converger la boucle (R53).
- Pour un candidat de type `decision`, la session le compare aux **décisions actives du même
  projet** et, s'il en contredit une, le soumet comme **supersession en nommant l'ancien
  identifiant**. Elle ne l'applique jamais : `invalidatedAt` n'a que trois écrivains, tous passant
  par une validation humaine (R46).
- Les entrées consignées après l'heure du passage sont reprises au passage suivant, sans perte :
  l'horaire n'est donc pas critique.

**Priorité** : Must (lot 5)

**Hors périmètre à ce stade** (livré ultérieurement) : la **planification** elle-même (déclenchement
à 17h30 de la session de consolidation, notification bureau de 8h30 adossée à
`aplan brief --morning`) vit hors du dépôt et reste à installer. Les deux prérequis hors dépôt de
la conception restent à valider : le hook `SessionStart` doit détecter une session non interactive
plutôt que d'exiger une question, et l'API doit tourner en service (`systemd --user`) pour que la
garde de joignabilité rende une panne visible au lieu d'être la panne.

---

## 7. Règles métier

### 7.1 Granularité temporelle

| Règle | Description |
|-------|-------------|
| **R01a** | L'affectation des développeurs aux projets utilise la granularité **demi-journée** : matin (8h-12h) et après-midi (13h-17h) |
| **R01b** | La planification des **tâches** et des **réunions** utilise des **créneaux horaires** (heures de début et de fin). Les tâches sont représentées visuellement à une taille proportionnelle à leur estimation. |
| **R02** | La capacité hebdomadaire par défaut est de **10 demi-journées** (5 jours × 2 demi-journées). Cette valeur est configurable. |
| **R03** | Les réunions Outlook de plus de 2 heures sur une demi-journée consomment la totalité de cette demi-journée. Les réunions de moins de 2 heures consomment une fraction proportionnelle. |

### 7.2 Synchronisation et agrégation

| Règle | Description |
|-------|-------------|
| **R04** | La synchronisation avec les sources est déclenchée automatiquement à l'ouverture de l'application et peut être déclenchée manuellement. |
| **R05** | La fréquence de synchronisation automatique en arrière-plan est configurable (par défaut : toutes les 15 minutes). |
| **R06** | Les données agrégées sont stockées en cache local. En cas d'indisponibilité d'une source, le cache est utilisé. |
| **R07** | Les données propres de l'utilisateur (tâches personnelles, priorisations, journal d'activité) sont persistées localement et ne dépendent pas de la disponibilité des sources. |

### 7.3 Dédoublonnage

| Règle | Description |
|-------|-------------|
| **R08** | Quand un numéro de ticket Jira est identifié dans une ligne Excel (quelle que soit la colonne), les deux entrées sont fusionnées automatiquement. La source Jira fait foi pour les champs communs (statut, titre, assigné). |
| **R09** | Quand il n'y a pas de clé commune, l'outil propose un rapprochement par similarité (titre, assigné, projet) avec un score de confiance. L'utilisateur valide ou rejette la fusion. |
| **R08b** | **Choix du survivant lors d'une fusion** (auto R08 ou manuelle R09) : (1) si exactement l'une des deux tâches provient de Jira, c'est elle le survivant ; (2) sinon, c'est la tâche déjà visible dans le tableau de bord (`tracking_state = Followed`) ; (3) sinon, c'est la tâche primaire (repli déterministe). Le survivant est rendu visible : `tracking_state` est forcé à `Followed`. S'il n'avait pas de dates planifiées (`planned_start`/`deadline`), il hérite des valeurs correspondantes du doublon. Le doublon (enregistré dans `task_id_secondary` du lien `AutoMerged`/`ManualMerged`) est masqué du tableau de bord et de toutes les listes de tâches. |

### 7.4 Calcul automatique de l'urgence

| Règle | Description | Valeur d'urgence |
|-------|-------------|-----------------|
| **R10** | Tâche sans échéance définie | Urgence = 1 (basse) |
| **R11** | Échéance dans plus de 5 jours ouvrés | Urgence = 1 (basse) |
| **R12** | Échéance dans 2 à 5 jours ouvrés | Urgence = 2 (moyenne) |
| **R13** | Échéance dans 1 jour ouvré ou moins | Urgence = 3 (haute) |
| **R14** | Échéance dépassée | Urgence = 4 (critique) |
| **R15** | Une valeur d'urgence manuellement définie par l'utilisateur **prévaut toujours** sur le calcul automatique. Elle est conservée jusqu'à ce que l'utilisateur la réinitialise. |

### 7.5 Alertes

| Règle | Description |
|-------|-------------|
| **R16** | Une alerte de surcharge est émise lorsque la charge totale en heures (tâches planifiées + réunions) dépasse la capacité hebdomadaire. Les agrégations d'heures (totaux par jour et hebdomadaire) **excluent les tâches Terminées (`Done`) et Annulées (`Cancelled`)** : elles conservent leur estimation mais ne représentent plus de travail à venir, donc les compter gonflerait artificiellement les totaux. Les tâches Bloquées (`Blocked`) continuent de compter. (Les tâches terminées restent néanmoins **affichées** sur le tableau de bord.) |
| **R17** | Une alerte de deadline est émise lorsque l'échéance d'une tâche est à J-2 ou moins (configurable). |
| **R18** | Une alerte de conflit est émise lorsque les créneaux horaires de deux éléments (tâche/réunion) se chevauchent. |
| **R19** | Les alertes sont classées par gravité : **Critique** (dépassement, deadline dépassée), **Avertissement** (surcharge proche, deadline proche), **Information** (conflit mineur). |

### 7.6 Suivi d'activité

| Règle | Description |
|-------|-------------|
| **R20** | Un créneau d'activité est défini par : tâche, heure de début, heure de fin. |
| **R21** | Quand l'utilisateur change de tâche, le créneau précédent est automatiquement fermé (heure de fin = maintenant). |
| **R22** | Les créneaux sans tâche déclarée sont marqués comme "non renseigné" dans le journal. |
| **R23** | L'utilisateur peut modifier le journal a posteriori (corriger, ajouter, supprimer des créneaux). |
| **R28** | **Ordre des tâches dans le sélecteur du minuteur d'activité** : les tâches dont `plannedStart` OU `deadline` correspond à aujourd'hui (selon le fuseau horaire configuré) sont remontées en tête de liste. Aucune tâche n'est filtrée ou masquée. À l'intérieur du groupe « du jour », le tri secondaire est par urgence décroissante puis impact décroissant. Les tâches hors du jour sont triées après ce groupe, selon le tri habituel par priorité. |

### 7.7 Configuration du fichier Excel

| Règle | Description |
|-------|-------------|
| **R24** | La structure du fichier Excel (nom des colonnes, onglet actif, plage de données) est configurable via les paramètres de l'application. |
| **R25** | La colonne contenant le numéro de ticket Jira est configurable (si elle existe). |
| **R26** | Le mapping entre les colonnes Excel et les champs de l'outil est configurable. |

### 7.8 Tâches récurrentes

| Règle | Description |
|-------|-------------|
| **R31** | **Patterns supportés** : les tâches personnelles (`Source::Personal`) peuvent être associées à un modèle de récurrence. Cinq fréquences sont disponibles : (1) **Quotidienne** — intervalle en jours (minimum 1) ; (2) **Hebdomadaire** — intervalle en semaines + sélection libre des jours de la semaine (lundi–dimanche, au moins un jour requis) ; (3) **Toutes les deux semaines** — cas particulier de l'hebdomadaire avec intervalle = 2 ; (4) **Mensuelle par jour du mois** — intervalle en mois + numéro de jour 1–31 ; (5) **Mensuelle par Nième jour ouvrable du mois** — intervalle en mois + rang (1er, 2e, 3e, 4e, dernier) + jour de la semaine (ex. « premier mardi »). Seules les tâches `Source::Personal` peuvent être récurrentes. Les tâches issues de Jira, Excel ou Outlook restent ponctuelles — la récurrence est gérée par les systèmes amont. |
| **R32** | **Politique de fin** : un modèle de récurrence se termine selon l'une de trois politiques, configurée à la création : (a) **Jamais** — les occurrences sont générées indéfiniment dans la fenêtre glissante ; (b) **À une date** — aucune occurrence n'est créée après `endsOn` (inclus) ; (c) **Après N occurrences** — le décompte cumulatif des instances déjà créées borne la génération. Si `endsOn` et `maxOccurrences` sont tous deux définis, le premier critère atteint l'emporte. |
| **R33** | **Skip d'une occurrence** : l'utilisateur peut ignorer une instance individuelle sans interrompre la série. L'instance reçoit le statut `Cancelled` ; le calendrier des occurrences suivantes n'est pas modifié. Le skip est idempotent. |
| **R34** | **Exemption du report lundi** : une instance récurrente conserve sa date d'origine (`occurrenceDate`). La règle de report `carryForwardTasks` — qui avance les tâches personnelles non commencées au lundi courant — ne s'applique pas aux instances dont `recurrenceId` est non nul. |
| **R35** | **Politique des fins de mois (mensuelle par jour)** : pour `MonthlyByDay` avec `day = 31`, l'occurrence tombe le dernier jour du mois quelle que soit sa longueur (ex. 28 février, 30 novembre). Pour `day` compris entre 1 et 30, les mois dont la longueur est inférieure à `day` sont ignorés — l'occurrence est simplement absente ce mois-là, sans glissement vers le mois suivant. |
| **R36** | **Édition du modèle** : modifier le titre, la description, les tags, l'urgence, l'impact ou l'estimation d'un modèle de récurrence propage les changements à toutes les instances futures dont le statut est `Todo` et qui n'ont aucune entrée de worklog. Les instances passées et les instances dont le statut n'est pas `Todo` (en cours, terminées, bloquées, annulées) sont préservées telles quelles. Il n'est pas possible de modifier une seule occurrence sans affecter le modèle (hors périmètre MVP). |
| **R37** | **Horizon de matérialisation** : les instances récurrentes sont générées de manière paresseuse sur un horizon glissant de 14 jours à compter d'aujourd'hui. Aucune instance n'est pré-créée au-delà de cet horizon. La génération est déclenchée à chaque consultation de la liste de tâches, de la matrice de priorité, du dashboard et après chaque synchronisation externe. La génération est idempotente : tenter de créer une instance déjà existante pour un (modèle × date) n'a aucun effet. |
| **R38** | **Ancrage horaire des instances** : chaque instance reçoit un `plannedStart` égal à `occurrenceDate + 08:00 UTC` (≈ 10h00 heure de Paris, écart de fuseau DST toléré pour le MVP). La `deadline` d'une instance récurrente est `null` ; l'urgence est héritée du modèle (flag `urgencyManual = true`) afin d'éviter un calcul automatique qui assignerait une urgence basse en permanence. Une instance récurrente dont le `plannedStart` est antérieur à aujourd'hui et dont le statut est `Todo` est traitée comme urgente (niveau Critique) indépendamment de son `urgencyManual`. |
| **R39** | **Dédoublonnage de la matrice de priorité** : la matrice n'affiche qu'une seule carte par modèle récurrent — l'occurrence la plus proche (date d'occurrence la plus ancienne parmi les instances visibles). Les autres occurrences matérialisées restent disponibles dans la vue Workload et le journal d'activité. |
| **R40** | **Édition par occurrence** : le statut, les dates planifiées (`plannedStart`, `plannedEnd`), la deadline, les notes, l'état de suivi (`trackingState`) et les overrides d'heures (`remainingHoursOverride`, `estimatedHoursOverride`) sont modifiables sur une occurrence individuelle via `updateTask` sans affecter la série. Les champs de modèle (titre, description, urgence, impact, estimation de base, projet, tags) doivent être modifiés via `updateRecurringTask`. |
| **R41** | **Worklog partagé** : les entrées de worklog d'une série récurrente sont visibles sur toutes ses occurrences via le filtre `recurrenceId`. Chaque entrée affiche la date de l'occurrence concernée (`occurrenceDate`) sous forme d'étiquette formatée. |
| **R42** | **Changement de statut rapide** : le statut d'une tâche (récurrente ou non) est modifiable en un clic depuis la carte de tâche via un menu déroulant intégré (`StatusMenu`), sans ouvrir le panneau d'édition. |

### 7.9 Mémoire sémantique et rappel

| Règle | Description |
|-------|-------------|
| **R43** | **Frontière des trois tables** : `tasks` porte l'actionnable (statut, échéance, alertes), `worklog_entries` l'épisodique (ce qui s'est passé, horodaté), `memories` le sémantique (ce qu'il faut savoir). Un engagement produit **les deux** : une tâche pour la partie actionnable, et un souvenir pour le fait qu'un engagement a été pris, envers qui et en quels termes. Il est interdit d'écrire une date d'échéance dans un souvenir. |
| **R44** | **Deux axes de cycle de vie, distincts** : `status` (`pending` / `active` / `rejected`) gouverne la **file de validation** ; `invalidatedAt` + `supersededBy` gouvernent la **vérité** (bi-temporel). `rejected` est une pierre tombale : un candidat rejeté ne doit plus être re-proposé. |
| **R45** | **Filtre dur du rappel** : le rappel ne retourne que les souvenirs vérifiant `invalidatedAt IS NULL AND status = 'active'`, sauf demande explicite d'historique (`--history`). Non négociable : rappeler une décision annulée est pire que ne rien rappeler. |
| **R46** | **Écrivains de `invalidatedAt`** : uniquement les commandes de supersession (`aplan inbox supersede`, `aplan memory supersede`), toutes passant par une validation humaine. Ni `remember`, ni l'import, ni la recherche, ni aucun automatisme ne peut périmer un souvenir. |
| **R50** | **Fusion ≠ supersession**. `merge` = « même fait, mieux écrit » : une seule ligne survit, celle de la cible, qui garde son identité et ses dates et reçoit la nouvelle formulation ; **l'historique est écrasé**. `supersede` = « le fait a changé » : les deux lignes survivent, l'ancienne recevant sa fin de validité et un pointeur vers la nouvelle ; **l'historique est conservé**. Confondre les deux fait disparaître la réponse à « pourquoi a-t-on changé d'avis ». Les deux opérations sont appliquées en **une seule transaction** : une fusion à moitié faite laisserait le candidat en file avec sa formulation déjà recopiée, une supersession à moitié faite laisserait soit un fait masqué sans successeur, soit deux vérités contradictoires actives. |
| **R51** | **Chaînes légales, cycles interdits**. Une chaîne de supersessions (A → B → C) est valide, chaque révision portant sur le souvenir actif en tête. Un souvenir ne peut pas se superséder lui-même, une révision qui refermerait un cycle est refusée, et un souvenir déjà invalidé ne peut pas être re-supersédé (il faut viser la tête de chaîne). Un souvenir rejeté ou invalidé ne peut pas devenir la nouvelle vérité. |
| **R52** | **Jamais d'ajout muet**. À l'acceptation, un contrôle de quasi-doublon compare le candidat aux souvenirs **actifs** : l'index plein texte présélectionne (correspondance sur **n'importe quel** mot du titre — une correspondance sur *tous* les mots manquerait justement les reformulations), puis une règle de similarité pure tranche. Au-delà du seuil, rien n'est écrit et les trois issues sont proposées : fusionner, superséder, ou accepter explicitement. Distinguer une reformulation d'une contradiction est un jugement sémantique : le backend n'a aucun modèle, c'est donc l'humain qui tranche. |
| **R53** | **Le rejet est une pierre tombale**, pas une suppression : la ligne est conservée en `rejected` afin que la consolidation ne re-propose pas indéfiniment un candidat déjà écarté. Le rejet est un verdict de file, il n'écrit **pas** `invalidatedAt`. |
| **R54** | **Import idempotent et en lecture seule**. Chaque souvenir importé porte une référence de provenance stable dérivée du `name` de son fichier ; un fichier dont la référence existe déjà est ignoré. Relancer l'import n'écrit rien. Le dossier source n'est jamais modifié : il a déjà un écrivain. Un fichier sans entête est ignoré avec un motif, sans faire échouer l'import des autres. |
| **R47** | **La saisie utilisateur n'atteint jamais l'index telle quelle** : elle est découpée **sur les espaces uniquement**, et chaque groupe devient **une phrase entre guillemets, ponctuation interne conservée** (l'adjacence est donc préservée : `AP-1234` reste une expression exacte). Les groupes purement alphabétiques de 4 caractères ou plus sont étendus par préfixe ; ceux de 5 caractères ou plus finissant par `s` ou `x` reçoivent en plus une variante dépluralisée entre parenthèses. Les groupes sont joints par un `AND` explicite. Une saisie sans aucun caractère alphanumérique (`""`, `*`, ponctuation seule) est refusée avec une erreur de validation. Conséquence : `AP-1234` et `Cartier : certificat` — le vocabulaire quotidien — ne peuvent plus faire échouer la recherche. |
| **R48** | **Score de rappel** = somme pondérée de quatre signaux normalisés : pertinence textuelle (BM25), bonus d'entité (projet / tâche / personne du contexte courant), décroissance de récence sur `occurredAt` (demi-vie 90 jours), poids par type (`decision` = `commitment` > `fact` > `preference`). Pas de fusion par rangs (RRF) : il n'y a qu'une seule liste classée en v1. |
| **R49** | **Indexation atomique** : le souvenir et sa ligne d'index plein texte sont écrits dans la même transaction. Un souvenir enregistré est donc toujours retrouvable, ou pas enregistré du tout. |
| **R55** | **Budget du brief** : le rendu de `aplan brief` est plafonné à **40 lignes** et chaque ligne à **140 caractères**, plafonds appliqués dans le domaine et vérifiés par un test sur une entrée pathologique. Cette sortie entre dans le contexte du modèle à chaque session : un rendu non borné est une fuite de tokens permanente, pas un défaut cosmétique. La troncature est **toujours annoncée** (`(8, 6 affichés)`) et s'applique de la section la moins utile vers la plus utile : les décisions cèdent avant les engagements, qui cèdent avant les échéances. |
| **R56** | **Composition du brief** : sont retenus les souvenirs `commitment` (les plus anciens d'abord — un engagement pris il y a trois mois est celui qu'on a oublié) et `decision` (les plus récents d'abord — la question est « où en est le projet »), filtrés par R45. Le projet courant est celui de la tâche en cours de suivi, sauf `--project` explicite ; sans projet en focus, toutes les décisions actives sont montrées plutôt qu'une section vide. Les échéances sont classées par proximité d'aujourd'hui, dédoublonnées par titre, et purgées des fixtures de test. **Chaque souvenir affiché porte une référence courte réutilisable par `aplan recall`** : c'est tout le mécanisme de récupération à la demande. |
| **R58** | **Un seul résolveur d'identifiant de souvenir**. Tout argument d'identifiant — en lecture (`recall`) comme en écriture (`inbox accept`/`reject`/`merge --into`/`supersede --replaces`, `memory supersede --by`) — accepte l'UUID complet **ou** la référence courte affichée (`m:7c1`, `[m:7c1]`, `7c1`), et passe par la **même** résolution. Sans cette règle, le produit affiche une référence courte, laisse *lire* avec elle, puis la refuse dès qu'on veut *agir* : les commandes les plus fréquentes deviennent des recopies de 36 caractères. Un préfixe ambigu est refusé (code 3, candidats listés) et l'ambiguïté est évaluée sur **tout le magasin**, pas sur la seule file — sinon un préfixe unique aujourd'hui désignerait un autre souvenir demain. Un identifiant introuvable est un « non trouvé » (code 2), jamais une erreur générique. Les verbes à deux identifiants résolvent **les deux avant d'écrire** (corollaire de R50). |
| **R57** | **Visibilité de la panne de consolidation** : le brief affiche l'âge du dernier passage de consolidation dès qu'il dépasse **3 jours**, et « jamais exécutée » si aucun passage n'est enregistré. L'horodatage vit dans la table `configuration` (clé `memory.consolidation.last_run`) et non dans `sync_status`, dont la colonne `source` est sous une contrainte `CHECK` fermée. Une clé absente ou invalide se lit comme « jamais exécutée » : le brief rend la panne visible, il ne tombe pas avec elle. |
| **R59** | **Filigrane de consolidation PAR ENTRÉE, jamais curseur horodaté**. Une entrée de journal porte son propre marqueur (`worklog_entries.consolidated_at`) ; la consolidation lit exactement les entrées dont il est nul, de la plus ancienne à la plus récente. Un curseur horodaté sauterait **définitivement** toute entrée insérée tardivement avec un `loggedAt` antérieur au curseur, et rien ne signalerait la perte. Le marqueur est **idempotent** (`consolidated_at IS NULL` fait partie de la condition de mise à jour, le premier marquage gagne) et **posé après les écritures réussies** : un souvenir en double se rejette (R53), une entrée marquée sans souvenir est perdue. Marquer un lot est **atomique** (une transaction), pour qu'un lot ne puisse pas être marqué à moitié. |
| **R60** | **Garde de joignabilité de la consolidation** : la session planifiée vérifie que l'API répond **avant toute autre chose** et, si elle ne répond pas, **ne fait rien et ne pose aucun marqueur** — l'exécution suivante rattrape tout. La CLI étant un client GraphQL, sans cette garde une API arrêtée produirait une suite d'échecs silencieux. Corollaire : une consolidation à moitié faite n'existe pas ; soit le lot est traité et marqué, soit rien ne bouge. |
| **R61** | **La consolidation propose, elle n'applique jamais**. Tout ce qu'elle écrit est `PENDING` ; elle n'exécute aucun verbe de la file (`accept`, `merge`, `supersede`, `reject`) et n'emploie ni `--confirm` ni `--force`. Pour une décision qui contredit une décision active du même projet, elle **soumet une supersession en nommant l'ancien identifiant** et laisse l'utilisateur trancher entre supersession (le fait a changé) et fusion (même fait, mieux écrit) — cette distinction est un jugement sémantique, et le backend n'a aucun modèle (R50, R52). |
| **R62** | **Code de sortie 4 pour une précondition refusée**. Un état que le magasin refuse de quitter — candidat déjà `ACTIVE` ou `REJECTED`, cible de fusion non active, souvenir déjà invalidé, cycle de supersession, saisie sans rien de recherchable — sort en **4**, jamais en 1. Le 1 est réservé à « l'appel n'a pas abouti » (réseau, base). Un appelant automatisé doit distinguer les deux : le premier se saute, le second impose de reprendre tout le passage sans poser de marqueur. |

---

## 8. Données et informations manipulées

### 8.1 Entités principales

#### Tâche (agrégée)

L'entité centrale de l'outil. Une tâche peut provenir de plusieurs sources.

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| titre | Texte | Oui | Titre de la tâche |
| description | Texte | Non | Description détaillée. Pour les tâches Jira, **synchronisée et écrasée à chaque sync**. |
| notes | Markdown | Non | Notes utilisateur en markdown, **toujours préservées** par les synchronisations. C'est le champ recommandé pour stocker des observations, plans, journal de bord, etc. |
| delegated_to | Texte | Non | Personne à qui la tâche est déléguée (texte libre, **toujours préservé** par les synchronisations, distinct de l'assigné Jira). Voir R-DEL-01 à R-DEL-06. |
| source | Enum | Oui | `jira`, `excel`, `obsidian`, `personnel` |
| sourceId | Texte | Non | Identifiant dans la source d'origine (ex : numéro Jira) |
| statut | Enum | Oui | `à_faire`, `en_cours`, `terminée`, `bloquée` |
| projet | Référence Projet | Non | Projet associé |
| assigné | Texte | Non | Personne assignée |
| échéance | Date | Non | Date limite |
| planificationDébut | Date/heure | Non | Date et heure de début planifiée |
| planificationFin | Date/heure | Non | Date et heure de fin planifiée |
| estimationHeures | Décimal | Non | Estimation de la durée en heures. Détermine la taille visuelle de la tâche. |
| urgence | Entier (1-4) | Oui | Calculée ou manuelle (R10-R15) |
| urgenceManuelle | Booléen | Oui | Indique si l'urgence a été forcée manuellement |
| impact | Entier (1-4) | Oui | Qualifié par l'utilisateur, défaut : 2 |
| tags | Liste de textes | Non | Catégories transverses |
| étatSuivi | Enum | Oui | `inbox` (non trié), `followed` (suivi actif), `dismissed` (écarté). Défaut : `inbox` pour les tâches synchronisées, `followed` pour les tâches créées manuellement |
| tempsRestantJira | Entier | Non | Temps restant Jira en secondes (champ `timeestimate`) |
| tempsOriginalEstiméJira | Entier | Non | Estimation originale Jira en secondes (champ `timeoriginalestimate`) |
| tempsDépenséJira | Entier | Non | Temps déjà consommé Jira en secondes (champ `timespent`) |
| surchargeHeuresRestantes | Décimal | Non | Surcharge locale des heures restantes (prioritaire sur la valeur Jira) |
| surchargeHeuresEstimées | Décimal | Non | Surcharge locale des heures estimées (prioritaire sur la valeur Jira) |
| créé_le | Date/heure | Oui | Date de création/import |
| modifié_le | Date/heure | Oui | Date de dernière modification |

#### Réunion

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| titre | Texte | Oui | Titre de l'événement Outlook |
| dateDebut | Date/heure | Oui | Début de la réunion |
| dateFin | Date/heure | Oui | Fin de la réunion |
| lieu | Texte | Non | Lieu ou lien Teams |
| participants | Liste de textes | Non | Noms/emails des participants |
| projetAssocié | Référence Projet | Non | Déduit du titre ou associé manuellement |
| outlookId | Texte | Oui | Identifiant dans Outlook |

#### Projet

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| nom | Texte | Oui | Nom du projet |
| source | Enum | Oui | `jira`, `excel`, `manuel` |
| sourceId | Texte | Non | Identifiant dans la source (clé Jira, nom dans Excel) |
| statut | Enum | Non | `actif`, `en_pause`, `terminé` |

#### Créneau d'activité

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| tâche | Référence Tâche | Non | Tâche en cours (`null` = non renseigné) |
| heureDebut | Date/heure | Oui | Début du créneau |
| heureFin | Date/heure | Non | Fin du créneau (`null` = en cours) |
| demiJournée | Enum | Oui | `matin`, `après-midi` |
| date | Date | Oui | Jour du créneau |

#### Alerte

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| type | Enum | Oui | `deadline`, `surcharge`, `conflit` |
| gravité | Enum | Oui | `critique`, `avertissement`, `information` |
| message | Texte | Oui | Description de l'alerte |
| élémentsConcernés | Liste de références | Oui | Tâches/réunions impliquées |
| date | Date | Oui | Date de l'alerte |
| résolu | Booléen | Oui | L'utilisateur a traité l'alerte |

#### Tag

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| nom | Texte | Oui | Nom du tag (ex : #revue-code) |
| couleur | Texte | Non | Couleur d'affichage |

#### Souvenir (mémoire sémantique)

Ce qu'il faut **savoir** : une décision, un engagement, un fait ou une préférence. Voir R43 à R49.

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| type | Enum | Oui | `decision`, `commitment`, `fact`, `preference`. Pas de `procedure` : le procédural est couvert par `CLAUDE.md` et les skills. |
| titre | Texte (≤ 500) | Oui | Une phrase : ce qu'on retient |
| contexte | Texte (≤ 10 000) | Non | Le « pourquoi », les alternatives écartées. **Jamais** une date d'échéance. |
| survenuLe | Date/heure | Oui | Quand la chose a été décidée / promise (base de la décroissance de récence) |
| enregistréLe | Date/heure | Oui | Quand aplan l'a su |
| invalidéLe | Date/heure | Non | `null` = encore vrai. Écrit uniquement par la supersession. |
| remplacéPar | Référence Souvenir | Non | Le souvenir qui l'a remplacé |
| provenance | Enum | Oui | `claude_session`, `manual`, `dreaming` |
| référenceProvenance | Texte | Non | Identifiant d'entrée de worklog ou de session. Sans contrainte d'intégrité : une chaîne de provenance pendante est préférée à un souvenir supprimé. |
| statut | Enum | Oui | `pending` (file de validation), `active`, `rejected` (pierre tombale) |
| projet | Référence Projet | Non | Rattachement. La suppression du projet **n'efface pas** le souvenir (mise à `null`). |
| tâche | Référence Tâche | Non | Rattachement. La suppression de la tâche **n'efface pas** le souvenir (mise à `null`). |
| personnes | Liste de textes | Non | « Envers qui », « avec qui » — permet de répondre à « quels engagements ai-je pris envers X ? » |

### 8.2 Données de configuration

| Paramètre | Type | Défaut | Description |
|-----------|------|--------|-------------|
| capacitéHebdomadaire | Entier | 10 | Nombre de demi-journées disponibles par semaine |
| fréquenceSynchro | Entier (minutes) | 15 | Intervalle de synchronisation automatique |
| fréquenceRappelActivité | Entier (minutes) | 120 | Intervalle du rappel "sur quoi tu travailles ?" |
| seuilAlerteDeadline | Entier (jours) | 2 | Nombre de jours avant échéance pour déclencher l'alerte |
| déclencheurPostRéunion | Booléen | true | Activer/désactiver la notification après chaque réunion |
| déclencheurPériodique | Booléen | true | Activer/désactiver le rappel périodique |
| heuresDébutTravail | Heure (HH:MM) | 08:00 | Heure de début de la journée de travail |
| heuresFinTravail | Heure (HH:MM) | 17:00 | Heure de fin de la journée de travail |
| jiraUrl | Texte | — | URL de l'instance Jira |
| jiraProjetKeys | Liste de textes | — | Clés de projets Jira à importer |
| microsoft.account | Texte | — | Adresse email du compte Microsoft connecté (lecture seule — renseigné après connexion via la porte d'authentification) |
| outlook.calendar_days | Entier (jours) | 14 | Horizon de synchronisation du calendrier Outlook |
| aplan.active_task_id | Texte (UUID) | — | Identifiant de la tâche liée à la session Claude courante (pointeur de configuration, défini par `aplan start`, effacé par `aplan stop`/`aplan done`) |
| aplan.active_since | Texte (ISO 8601) | — | Horodatage du début du suivi de la tâche active (utilisé comme borne de début pour la matérialisation des créneaux) |
| aplan.timezone | Texte (IANA) | `Europe/Paris` | Fuseau horaire utilisé pour convertir les horodatages UTC des entrées de worklog en bornes de journée/demi-journée |
| memory.consolidation.last_run | Texte (ISO 8601) | — | Date du dernier passage de la consolidation, écrite par `aplan consolidate record-run` et lue par le brief (R57, R59). Absente, illisible ou invalide → « jamais exécutée », sans faire échouer le brief. Stockée ici et non dans `sync_status`, dont la colonne `source` est sous contrainte `CHECK` fermée. |
| excelSharepointPath | Texte | — | Chemin du fichier Excel sur SharePoint |
| excelMappingConfig | Objet | — | Mapping colonnes Excel → champs de l'outil |
| obsidianVaultPath | Texte | — | Chemin du vault Obsidian (v2) |
| obsidianTaskTags | Liste de textes | `['#task']` | Tags Obsidian identifiant les tâches (v2) |

---

## 9. Cas particuliers / Cas limites

### 9.1 Sources de données

| Cas | Comportement attendu |
|-----|---------------------|
| **Source indisponible** (Jira down, réseau coupé) | L'outil utilise le cache local. Un indicateur montre que les données sont périmées avec la date de dernière synchronisation réussie. |
| **Excel modifié (structure changée)** | L'outil détecte les changements de structure et alerte l'utilisateur. Les données non mappables sont ignorées avec un avertissement. |
| **Tâche supprimée dans Jira** | La tâche disparaît de la vue après synchronisation. Si elle a des données locales (priorisation, tags), l'utilisateur est notifié. |
| **Réunion annulée dans Outlook** | La réunion disparaît de la vue. La capacité est restaurée automatiquement. |
| **Ticket Jira déplacé vers un autre projet** | La tâche est mise à jour avec le nouveau projet. Les données locales (priorité, tags) sont conservées. |

### 9.2 Dédoublonnage

| Cas | Comportement attendu |
|-----|---------------------|
| **Même tâche dans 3+ sources** | Fusion en cascade : Jira fait foi, enrichi par Excel, puis par Obsidian. |
| **Faux positif de rapprochement** | L'utilisateur peut rejeter la suggestion de fusion. La paire est mémorisée pour ne plus être proposée. |
| **Tâche fusionnée puis dissociée dans la source** | Si le ticket Jira est supprimé mais la ligne Excel reste, la tâche revient en tant que source unique Excel. |

### 9.3 Suivi d'activité

| Cas | Comportement attendu |
|-----|---------------------|
| **L'utilisateur ne répond pas à la notification** | Le créneau reste "non renseigné". L'outil ne bloque pas le travail. |
| **Travail sur plusieurs tâches en parallèle** | L'utilisateur peut sélectionner une seule tâche active à la fois. S'il travaille sur plusieurs, il choisit la principale. |
| **Application fermée pendant un moment** | Le créneau entre la fermeture et la réouverture est marqué "non renseigné" (modifiable a posteriori). |
| **Week-end ou jour non ouvré** | Pas de notification de suivi d'activité. Les jours non ouvrés ne comptent pas dans la capacité. |

### 9.4 Priorisation

| Cas | Comportement attendu |
|-----|---------------------|
| **Tâche sans échéance et sans impact défini** | Urgence = 1, Impact = 2 (défauts). La tâche apparaît dans le quadrant "Ni urgent ni important" par défaut. |
| **Échéance modifiée dans Jira** | L'urgence auto-calculée est mise à jour. Si l'urgence était manuelle, elle n'est PAS écrasée (R15). |
| **Tâche terminée dans Jira mais priorité haute dans l'outil** | La tâche passe en statut "terminée" et sort de la vue quotidienne, quelle que soit sa priorité. |

---

## 10. Exigences non fonctionnelles

### 10.1 Performance

| Exigence | Cible |
|----------|-------|
| Temps de chargement du dashboard (données en cache) | < 2 secondes |
| Temps de synchronisation complète (Jira + Outlook + Excel) | < 10 secondes |
| Temps de réponse pour un changement de tâche (suivi activité) | < 500ms |

### 10.2 Disponibilité et résilience

| Exigence | Description |
|----------|-------------|
| Mode hors ligne | L'outil reste fonctionnel avec les données en cache si les sources sont indisponibles |
| Persistance locale | Les données propres ne sont jamais perdues, même en cas de crash |
| Indicateur de fraîcheur | La date/heure de dernière synchronisation est toujours visible |

### 10.3 Sécurité

| Exigence | Description |
|----------|-------------|
| Credentials | Les tokens d'accès aux API (Jira, Graph) sont stockés de manière sécurisée |
| Données locales | Les données persistées localement sont sur le poste de l'utilisateur uniquement |
| Pas de données sensibles exposées | L'outil ne transmet pas de données vers des services tiers non déclarés |

### 10.4 Compatibilité

| Exigence | Description |
|----------|-------------|
| Navigateur | Chrome et Edge (dernières 2 versions) |
| Résolution | Desktop 1280×720 minimum |

### 10.5 Configurabilité

| Exigence | Description |
|----------|-------------|
| Paramétrage | Tous les paramètres listés en section 8.2 sont modifiables via une interface de configuration |
| Mapping Excel | La structure du fichier Excel est entièrement configurable sans modifier le code |

---

## 11. Hypothèses et points ouverts

### 11.1 Hypothèses prises

| # | Hypothèse |
|---|-----------|
| H1 | L'utilisateur dispose d'un accès API à Jira (token personnel ou OAuth) |
| H2 | L'utilisateur dispose d'un accès Microsoft Graph API (pour Outlook et SharePoint) via un enregistrement d'application Azure AD |
| H3 | Le fichier Excel SharePoint a une structure tabulaire avec des en-têtes de colonnes identifiables |
| H4 | L'utilisateur est le seul à utiliser l'outil ; il n'y a pas de besoin de partage de données |
| H5 | Le vault Obsidian est accessible localement depuis la machine où tourne l'outil (v2) |
| H6 | L'architecture existante (React + Hono, TypeScript, monorepo pnpm) est conservée |

### 11.2 Points ouverts

| # | Question | Impact | Décision à prendre par |
|---|----------|--------|----------------------|
| P1 | Quelle est la structure exacte du fichier Excel ? (colonnes, onglets) | Configuration du mapping Excel (R24-R26) | L'utilisateur, lors de la mise en place |
| P2 | Quel type d'authentification Jira est utilisé ? (API token, OAuth 2.0, PAT) | Architecture du connecteur Jira | L'utilisateur |
| P3 | L'outil tourne-t-il en local uniquement ou pourrait-il être déployé sur un serveur ? | Architecture de déploiement, accès Obsidian | À décider |
| P4 | Comment gérer les jours fériés et les congés dans le calcul de capacité ? | Règles R01-R03 | À spécifier |
| P5 | Faut-il un mécanisme de backup des données locales ? | Persistance | À décider |
| P6 | La convention de tags Obsidian doit-elle être compatible avec un plugin existant (Obsidian Tasks, Dataview) ? | Spécification du format de parsing (v2) | L'utilisateur |
| P7 | Faut-il pouvoir associer manuellement un projet à une réunion Outlook, ou la détection automatique par titre suffit-elle ? | US-061 (vue projet consolidée) | À décider |

### 11.3 Décisions prises

| # | Décision | Contexte |
|---|----------|----------|
| D1 | L'outil est en lecture seule vis-à-vis des sources externes | Pas de sync bidirectionnelle pour simplifier et éviter les conflits |
| D2 | Utilisateur unique, pas de multi-utilisateurs ni de rôles | Simplifie drastiquement l'architecture |
| D3 | Granularité demi-journée conservée | Cohérent avec la réalité du travail du Tech Lead |
| D4 | Priorité Jira ≠ Priorité de l'outil | L'outil a sa propre matrice impact/urgence, indépendante de la priorité Jira |
| D5 | Le journal d'activité est pour usage personnel uniquement | Pas d'export automatique ni de partage |
| D6 | Architecture multi-user ready (`user_id` sur toutes les tables, middleware d'authentification) | Prépare le déploiement futur en tant qu'application Microsoft Teams. Utilisateur unique en MVP — utilisateur par défaut créé automatiquement. |
| D7 | CLI timesheet flag-driven, sans mode REPL | L'édition interactive riche se fait dans l'interface React (Surface B, Plan 3) ; la CLI sert l'automatisation et l'intégration. |

### 11.4 Limitations connues (Timesheet)

Les fonctionnalités de timesheet (Plan 2) ont les limitations suivantes, à améliorer dans des itérations futures :

| Limitation | Détail |
|-----------|--------|
| **Scope demi-journée non appliqué à `markDayOff`** | La mutation `markDayOff(date, scope)` et la sous-commande CLI `aplan timesheet off [--am\|--pm]` ignorent actuellement le scope : une demi-journée marquée (AM/PM) indisponible marque la **journée complète** comme off. Affinement futur : émettre le scope à la base et refléter ce découpage dans la reconstruction de feuille de temps. |
| **Pas de commande pour vider le bucket non-attribué** | Il n'existe pas de verbe CLI dédié pour affecter en masse les heures non attribuées d'un jour à un projet. Solution actuelle : épingler des heures projet par projet via `aplan timesheet set <projet> <heures>` jusqu'à atteindre le total, ou créer des règles de mappage via `aplan map add` pour suggérer automatiquement. |
| **`aplan map add` n'applique pas de priorité de sélecteurs** | La commande `aplan map add` accepte actuellement une seule règle (kind + pattern + gryzzlyProjectId) mais ne rejetse pas les collisions de sélecteurs distincts (p. ex., deux règles `repository` avec patterns différents). La logique de priorité (repository > subject > organizer > internal_project) sera appliquée et documentée dans une prochaine version afin de traiter élégamment les cas d'ambiguïté. |

---

## 12. Glossaire

| Terme | Définition |
|-------|-----------|
| **Demi-journée** | Unité de temps de base. Matin : 8h-12h, Après-midi : 13h-17h |
| **Capacité** | Nombre de demi-journées disponibles par semaine (défaut : 10) |
| **Charge** | Nombre d'heures consommées par les tâches planifiées et les réunions |
| **Surcharge** | Situation où la charge dépasse la capacité |
| **Conflit** | Deux éléments (tâche/réunion) dont les créneaux horaires se chevauchent |
| **Source** | Système externe dont les données sont importées (Jira, Outlook, Excel, Obsidian) |
| **Tâche agrégée** | Tâche dans l'outil, pouvant résulter de la fusion de données de plusieurs sources |
| **Dédoublonnage** | Processus de détection et fusion de tâches apparaissant dans plusieurs sources |
| **Journal d'activité** | Historique des créneaux de travail déclarés par l'utilisateur |
| **Matrice impact/urgence** | Outil de priorisation à 4 quadrants basé sur deux axes : importance et urgence |
| **Tag** | Catégorie personnalisée permettant de classifier les tâches transversalement |
| **Cache** | Copie locale des données agrégées, utilisée quand les sources sont indisponibles |
| **Créneau d'activité** | Période de temps associée à une tâche dans le journal d'activité |
| **Créneau horaire** | Plage horaire définie par une heure de début et une heure de fin, utilisée pour planifier tâches et réunions |
| **Estimation** | Durée estimée d'une tâche en heures, déterminant sa taille visuelle dans les vues planning |
| **Semaine** | Période du lundi au vendredi (5 jours ouvrés). Le lundi est le premier jour de la semaine. |
| **Souvenir** | Entrée de mémoire sémantique : une décision, un engagement, un fait ou une préférence. Répond à « qu'est-ce que je dois savoir ? », par opposition à la tâche (« que dois-je faire ? ») et au journal (« que s'est-il passé ? ») |
| **Bi-temporel** | Modèle où chaque fait porte à la fois la date à laquelle il est devenu vrai (`survenuLe`) et celle à laquelle il a cessé de l'être (`invalidéLe`), plutôt qu'une suppression |
| **Supersession** | Remplacement d'un souvenir par un autre : l'ancien reçoit une fin de validité et un successeur, les deux lignes survivent. À distinguer de la fusion, qui écrase l'historique |
| **File de validation** | Ensemble des souvenirs candidats en statut `pending`, en attente d'acceptation, de fusion, de supersession ou de rejet par l'utilisateur |
| **Rappel (recall)** | Récupération d'un souvenir, par identifiant ou par recherche plein texte classée |
| **Consolidation** | Relecture planifiée du journal de bord qui en extrait des souvenirs candidats. Assurée par une session Claude Code planifiée, pas par le backend : le dépôt ne contient aucun client de modèle. Ne propose que du `pending` |
| **Filigrane de consolidation** | Marqueur porté par **chaque** entrée de journal (`consolidatedAt`), qui dit si la consolidation l'a déjà lue. Distinct d'un curseur horodaté, qui sauterait définitivement une entrée insérée tardivement mais datée du passé (R59) |
