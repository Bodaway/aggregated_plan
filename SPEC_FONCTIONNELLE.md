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

> En tant que Tech Lead, je veux que le catalogue Gryzzly (projets actifs et terminés, avec leurs tâches) soit synchronisé automatiquement afin de pouvoir associer mon activité à la bonne tâche Gryzzly lors de la déclaration de temps.

**Critères d'acceptation :**
- Le catalogue Gryzzly est importé en lecture seule : projets actifs et terminés, avec leurs tâches (l'outil ne modifie jamais Gryzzly). Les projets supprimés dans Gryzzly sont exclus.
- Pour chaque tâche du catalogue, sont conservés : nom de la tâche, projet associé et client (`customer_name`).
- La synchronisation est déclenchée par `forceSync` (comme les autres sources). Gryzzly ne délivrant aucune clé d'API, l'authentification réutilise la **session du navigateur** : le cookie posé par la connexion SSO Microsoft sur `app.gryzzly.io` est lu et déchiffré depuis le profil navigateur local. Il faut donc s'être connecté à Gryzzly dans le navigateur — la session vaut **7 jours**, soit une reconnexion par semaine.
- Si aucune session n'est trouvée, la source est marquée « non configurée ». Si la session a **expiré**, le message d'erreur indique la date d'expiration et invite à se reconnecter. Un jeton peut aussi être collé à la main en configuration si la lecture automatique échoue.
- Le catalogue alimente la sélection d'une tâche Gryzzly lors de la déclaration d'activité (US-030) — il ne crée pas de tâches aplan.
- Une tâche Gryzzly disparue d'une synchronisation est désactivée mais jamais supprimée : une activité déjà associée à cette tâche reste résoluble.
- Les projets Gryzzly **terminés** sont désormais synchronisés eux aussi, et signalés par un badge `terminé` dans le sélecteur de tâche Gryzzly et sur la tâche assignée. Leurs tâches restent **sélectionnables** : un projet se clôt souvent alors qu'il reste des heures à déclarer dessus. Avant cela, une tâche sur projet clos était indistinguable d'une tâche supprimée dans Gryzzly.
- Une source non configurée est affichée comme « Non configuré » (gris) et non comme une erreur (rouge). Pour Gryzzly, la raison exacte (session expirée, avec sa date) est affichée directement sous la barre de synchronisation et accompagnée d'un lien **Reconnecter** vers `app.gryzzly.io`.
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
- L'assignation est accessible depuis **deux surfaces**, qui partagent la même liste (recherche, regroupement par projet, badges `stale` et `terminé`) :
  - le volet d'édition de tâche, via un sélecteur pleine largeur ;
  - la **carte de tâche du dashboard**, via une puce déroulante placée à côté du menu de statut — l'assignation se change sans ouvrir le volet.
- La puce du dashboard affiche le nom de la tâche Gryzzly assignée (ou `Gryzzly` quand la tâche est libre), en ambre si l'assignation est périmée (`stale`) et suivie du badge `terminé` si le projet propriétaire est clos.

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
- **R-WL-09** : Une session Claude Code peut être liée à sa propre tâche, indépendamment du pointeur global. Le pointeur global (`aplan.active_task_id`) reste **l'utilisateur en manuel**, une tâche à la fois ; une ligne de `sessions` est **un Claude**, et plusieurs sessions peuvent journaliser sur des tâches différentes en parallèle sans se déplacer mutuellement. Commandes : `aplan sessions` (les sessions ouvertes plus la ligne manuelle), `aplan session show|bind|off|end`, et le drapeau global `--session <id>` qui prend par défaut la valeur de `CLAUDE_CODE_SESSION_ID`.
- **R-WL-10** : La cible implicite de `log`, `note`, `status` et `done` se résout en trois niveaux : `--task` d'abord, puis la tâche de la session, puis le pointeur global. **Une session en mode « ne pas tracker » refuse (code 4) au lieu de retomber sur le pointeur global** — c'est ce refus qui empêche un Claude de rapporter du travail sur une tâche que l'utilisateur a explicitement déclinée. Une session dont l'identifiant n'existe dans aucune ligne n'est pas une décision de l'utilisateur : elle retombe sur le pointeur global, comme une session absente.
- **R-WL-11** : `remember` est l'exception délibérée à R-WL-10 : il ne refuse jamais. `--task` gagne, sinon une session qui tracke rattache la mémoire à sa tâche, sinon la mémoire est créée non rattachée et la commande réussit — y compris pour une session en mode « ne pas tracker ». Les mémoires sont hors des règles du worklog, et une mémoire non rattachée n'attribue rien à tort, là où une entrée de worklog mal attribuée est du temps facturable sur la mauvaise tâche.
- **R-WL-09** : le temps est enregistré en créneaux **fermés** dérivés des horodatages des entrées de worklog, jamais via un créneau ouvert. Un créneau couvre une **plage de travail continue** (R-WL-13) : il va de sa première à sa dernière entrée, ne franchit jamais la frontière de demi-journée, et vaut une minute au minimum lorsqu'il est réduit à une seule entrée. Les créneaux sont matérialisés à `aplan stop`, `aplan done`, ou en fin de session (hook `SessionEnd`).
- **R-WL-10** : le fuseau horaire `aplan.timezone` (défaut `Europe/Paris`) définit les bornes de journée et de demi-journée utilisées pour dériver les créneaux à partir des horodatages UTC des entrées.
- **R-WL-11** : le lien session→tâche est le pointeur de configuration `aplan.active_task_id` (défini par `aplan start`, effacé par `aplan stop`/`aplan done`). Il n'existe aucun créneau d'activité ouvert associé à ce pointeur.
- **R-WL-12** : chaque entrée porte un **filigrane de consolidation** (`consolidatedAt`, nul par défaut) qui dit si la consolidation mémoire l'a déjà lue (US-096, R59). Ce marqueur n'est ni saisi ni affiché par l'utilisateur : il n'appartient qu'au dispositif de mémoire, et le supprimer ferait re-proposer chaque soir l'intégralité du journal.
- **R-WL-13** : **règle des 45 minutes** — un écart de **plus de 45 minutes** entre deux entrées consécutives est du temps qui **n'a pas** été passé sur la tâche. Les horodatages d'une même demi-journée sont donc découpés en autant de créneaux qu'il y a de plages de travail continues : la coupure tombe partout où deux entrées consécutives sont séparées de plus de 45 minutes, et deux entrées séparées de 45 minutes ou moins restent dans la même plage (45 min → même créneau, 45 min et 1 s → deux créneaux). Le seuil est une constante du domaine (`MAX_CONTINUATION_GAP_MINUTES`), délibérément non configurable : un seuil variable rendrait deux relevés incomparables. **Pourquoi 45 et non 15** : une entrée de journal est un **marqueur d'événement**, pas un échantillon d'activité — la consigne étant « une entrée par découverte, décision ou action », deux entrées peuvent légitimement être espacées de 40 minutes pendant une lecture de code ou une compilation, sans interruption du travail. Un seuil de 15 minutes suppose une cadence dense que le journal n'a pas et sous-compte lourdement (−73 % mesuré sur une journée réelle) ; 45 minutes exclut une vraie pause tout en tolérant une cadence clairsemée. La distribution réelle des écarts du 03/08/2026 (25 écarts entre ses 26 horodatages distincts) le confirme : 20 écarts de 5 à 15 min, deux de **15'02 et 15'10** — que le seuil de 15 min coupait à la seconde près —, deux de 37'08 et 43'02, aucun entre 46 et 60 min, et un arrêt de 2h53. **L'invariant « un créneau par demi-journée » est donc abandonné** — une demi-journée en porte autant que le journal en justifie. Conséquence assumée : le temps dérivé d'un journal donné **diminue**, et c'est le but — l'après-midi du 03/08/2026, dont les entrées s'étalent de 14:56 à 21:34 (heure de Paris), passe de **6h38** (un créneau unique, pauses comprises) à **3h45** en deux créneaux, l'arrêt de 2h53 après 18:41 n'étant plus facturé. Le temps inactif n'est imputé à personne. La règle ne s'applique **pas** à la reconstruction de feuille de temps (US-TS), qui a son propre report d'attribution sur les fenêtres matin/après-midi.
- **R-WL-14** : **rédiger une journée après coup** — `aplan log --at <QUAND>` place l'entrée à l'instant où le travail a eu lieu, en heure locale, au lieu de l'horodater `now`. Sans ce drapeau, une journée écoulée rédigée plus tard ne compte pour rien : sept entrées écrites le 11/08 à propos du 06 et du 10/08 laissaient ces deux jours à zéro heure et posaient un créneau quasi nul sur le 11. Quatre formes : `AAAA-MM-JJTHH:MM[:SS]`, `"AAAA-MM-JJ HH:MM[:SS]"`, `HH:MM[:SS]` (aujourd'hui) et `AAAA-MM-JJ` seul, **qui vaut midi**. Midi n'est pas arbitraire : par la règle de l'ombre portée (§ US-TS), une entrée est la preuve des 45 minutes qui la *précèdent*, rognées aux fenêtres de travail — midi porte 11:15–12:00, entièrement dans la matinée, tandis que minuit ne tombe dans aucune fenêtre et que 08:00 projette son ombre sur 07:15–08:00, avant l'ouverture : les deux factureraient zéro. Une valeur mal écrite est un refus (code 4) **avant toute écriture**. La valeur est envoyée non convertie ; le serveur applique `aplan.timezone` (R-WL-10), pour que l'entrée tombe sur le jour local saisi et pas sur un autre.
- **R-WL-15** : `--at` **reconstruit aussi les créneaux du jour concerné**, sans quoi le drapeau ne déplacerait que la note et pas les heures : la matérialisation (R-WL-09) déduit les demi-journées à reconstruire de sa propre fenêtre, laquelle commence au démarrage de la session, donc une entrée antidatée lui est invisible et son jour continuerait d'afficher zéro heure. Si cette reconstruction échoue, la commande **réussit quand même** — l'entrée est écrite et relancer `log` la dupliquerait — et nomme la réparation, `aplan slots rebuild --task <T> --date <J>`, qui est idempotente et sert aussi aux journées antidatées autrement (« Edit timestamp » de l'UI web, R-WL-02). Après coup, relancer `aplan timesheet --date <J>` : le brouillon avait été reconstruit avant. **Ce que `--at` n'invente pas** : les heures viennent toujours de l'étalement des entrées (R-WL-13). Sept entrées antidatées à la même minute valent une minute, exactement comme en direct — rédiger honnêtement une journée passée demande de donner à chaque entrée l'heure à laquelle elle a eu lieu.
- **R-WL-16** : **relire le journal depuis la CLI** — `aplan show <tâche>` se termine par la queue du journal que `aplan log` a écrit : les **10 entrées les plus récentes, de la plus ancienne à la plus récente**, chacune précédée de son horodatage en heure locale (R-WL-10) et de son **auteur** — `manuel` pour l'humain, sinon les quatre premiers caractères de l'identifiant de session (R-WL-09), la même abréviation que les avertissements de recouvrement de `aplan journal`, pour qu'une session se lise pareil partout. Sans ce verbe la CLI n'écrivait le journal que dans un sens : le relire imposait l'UI web ou une requête GraphQL à la main. `--worklog` prend un compte ou `all` : `N` conserve les N entrées les plus récentes, **`all` imprime le journal entier**, `0` supprime la section **et son aller-retour réseau**. La ligne « … older entries not shown » n'apparaît que s'il restait des entrées plus anciennes — jamais avec `all` —, pour qu'une queue tronquée ne se lise jamais comme un journal complet. Une valeur qui n'est ni un compte ni `all` est un refus **à l'analyse des arguments**, avant tout aller-retour réseau. La lecture est **accessoire** : si elle échoue, le détail de la tâche s'affiche quand même et la commande réussit — une note sur stderr, et en `--json` un `"worklogEntries": null` explicite plutôt qu'un tableau vide, qui affirmerait à tort qu'il n'y a rien de journalisé.

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

### 6.4quater Réattribution du temps consigné

#### US-RE : Corriger la tâche à laquelle du temps a été attribué

> En tant que Tech Lead, je veux déplacer les entrées de journal — et le temps qui en découle — d'une tâche vers une autre, afin de corriger une journée consignée sur la mauvaise tâche avant qu'elle n'alimente ma feuille de temps et la facturation client.

`aplan log` écrit sur la tâche qu'on lui donne : sans ce verbe, une attribution erronée reste erronée. Ce n'est pas hypothétique — 37 entrées et 4h35 du 03/08/2026 ont été consignées sur « Saft : basculer le temps projet… » alors qu'elles appartenaient à « Design : couche mémoire agent ».

**Critères d'acceptation :**
- **R-RE-01** : la sélection se fait **soit** par entrées explicites (`--entry`, UUID complet ou préfixe d'identifiant, répétable), **soit** par tâche source et fenêtre de dates locales (`--date` pour un jour, `--since`/`--until` pour une plage). Les deux modes ensemble sont refusés : on ne saurait pas ce qui a été corrigé.
- **R-RE-02** : les tâches (`--from`, `--to`) acceptent toutes les formes des autres verbes : UUID, préfixe, clé Jira, titre approché, `@current`. Un préfixe d'entrée ambigu est **signalé** (code 3), jamais deviné : deviner ici déplace la mauvaise heure de travail.
- **R-RE-03** : **aperçu par défaut**. Sans `--confirm`, rien n'est écrit : les deux tâches sont résolues et nommées, les heures avant/après sont affichées, et l'opération s'arrête. `--confirm` applique exactement ce qui a été montré (même chemin de code, donc aucune dérive possible entre l'aperçu et l'écriture).
- **R-RE-04** : les créneaux d'activité sont **redérivés**, pas repointés. Ils sont une projection des horodatages du journal (un créneau par plage de travail continue, jamais à cheval sur deux demi-journées locales — R-WL-13) : la correction supprime la projection des deux tâches **dans les seules demi-journées** où tombe une entrée déplacée, puis la reconstruit depuis les entrées. Un déplacement partiel devant re-borner les deux côtés, repointer `task_id` attribuerait à la destination du temps qui n'a pas bougé.
- **R-RE-05** : la correction ne touche **jamais** les créneaux d'une troisième tâche sur la même demi-journée, ni la matinée quand seul l'après-midi bouge, ni un créneau **ouvert** (minuteur en cours). Suppression et reconstruction portent sur le même ensemble de demi-journées, et la suppression emporte **tous** les créneaux fermés des deux tâches dans ces demi-journées, ce qui interdit le double comptage : sans elle, une destination qui travaillait déjà cette matinée-là conserverait son créneau **en plus** de celui reconstruit.
- **R-RE-06** : le total des deux tâches sur les demi-journées touchées peut légitimement changer, et c'est **signalé** : (a) un déplacement partiel re-borne les deux côtés ; (b) une demi-journée portant des créneaux que le journal ne justifie pas (plusieurs matérialisations partielles, ou une matérialisation antérieure à la règle des 45 minutes) est ramenée à ce que les entrées projettent aujourd'hui. L'aperçu montre ce changement avant écriture.
- **R-RE-07** : le compte rendu indique ce qui a bougé (entrées sélectionnées, entrées effectivement déplacées, dates touchées, créneaux supprimés et reconstruits) et les heures avant/après **par tâche**, afin que la correction soit vérifiable et non pas crue sur parole.
- **R-RE-08** : sont refusés sans rien écrire, avec le code de sortie 4 : source et destination identiques, entrée n'appartenant pas à la tâche source annoncée, sélection vide, sélection atteignant le plafond de page (1 000 entrées — à découper en plusieurs passes plutôt que tronquée en silence).
- **R-RE-09** : le filigrane de consolidation (`consolidatedAt`) n'est **pas** remis à zéro : l'attribution d'une entrée et le fait que la consolidation l'ait lue sont deux questions distinctes.
- **R-RE-10** : après application, la feuille de temps du jour a été reconstruite **avant** la correction : le compte rendu invite à relancer `aplan timesheet --date <jour>`.

**Priorité** : Must (MVP v1)

---

### 6.4quinquies Réparation des créneaux orphelins

#### US-SR : Rendre leur tâche aux créneaux qui l'ont perdue

> En tant que Tech Lead, je veux réparer les créneaux d'activité affichés « (aucune tâche) » sur une plage de dates que je nomme, afin que les heures qu'ils portent redeviennent attribuables — sans toucher aux créneaux que j'ai créés à la main.

Ce n'est pas hypothétique : une écriture qui utilisait `INSERT OR REPLACE INTO tasks` supprime avant d'insérer, ce qui a déclenché le `ON DELETE SET NULL` de `activity_slots.task_id`. La ligne de la tâche revenait identique — rien ne paraissait cassé — pendant que les créneaux qui la désignaient perdaient leur attribution. 16 créneaux ont survécu ainsi sur trois jours d'août 2026 (04/08 après-midi, 06/08 après-midi, 10/08 matin et après-midi) : du temps réel, chiffré, appartenant à personne, que la feuille de temps ne peut pas facturer.

Ni `aplan flush` ni `aplan reattribute` ne pouvaient les atteindre : le premier ne nomme que les demi-journées de sa propre fenêtre, donc jamais une date passée ; le second refuse un déplacement dont la source et la destination sont la même tâche, et sa liste de suppression ne reconnaît que les créneaux dont le `task_id` vaut la tâche nommée — jamais un `task_id` NULL. Un simple `flush` aurait donc laissé l'orphelin en place **et** écrit un créneau neuf à côté : la demi-journée facturée deux fois.

**Critères d'acceptation :**
- **R-SR-01** : la sélection est une **plage de jours locaux explicite** (`--from`, `--to`, tous deux obligatoires). Aucune valeur par défaut : « tout » réécrirait des années d'historique sur une faute de frappe, et « aujourd'hui » n'atteindrait jamais le dégât, qui est toujours passé quand on le constate.
- **R-SR-02** : sont concernés les seuls créneaux **à la fois** sans tâche **et** propriété de la projection du journal (fermés, `source = worklog`). Un créneau `manual` sans tâche n'est **pas** un dégât : c'est un minuteur lancé à la main avant la migration `014`, il n'a jamais eu de tâche, et aucune entrée de journal ne peut le reproduire. Il est laissé **exactement** en l'état, ligne comprise.
- **R-SR-03** : le périmètre réel de la réparation est l'ensemble des **demi-journées qui contiennent un orphelin réparable**, et non la plage entière : une demi-journée intacte de la plage n'est ni relue pour suppression ni réécrite.
- **R-SR-04** : dans chaque demi-journée concernée, les orphelins sont supprimés **puis** la demi-journée est réécrite depuis les entrées de journal de **chaque tâche** qui y a consigné du temps. La suppression précède l'écriture : l'ordre inverse laisse une fenêtre où la demi-journée porte l'ancien créneau et son remplaçant, et un lecteur qui y arrive voit des heures doublées.
- **R-SR-05** : une tâche n'est interrogée que sur les demi-journées où elle a effectivement des entrées. Nommer une demi-journée sans entrée mettrait ses créneaux sur la liste de suppression sans rien pour les réécrire — c'est ainsi qu'une réparation efface des heures.
- **R-SR-06** : **aperçu par défaut**. Sans `--confirm`, rien n'est écrit. L'aperçu indique, **par date**, combien d'orphelins seraient supprimés et ce qu'ils valaient, contre combien de créneaux seraient écrits ; et **par tâche**, les heures avant/après. Les chiffres de l'aperçu sont lus sur les mêmes plans que l'écriture applique : ils ne peuvent pas diverger.
- **R-SR-07** : un orphelin dont le journal ne porte plus aucune entrée est **supprimé sans qu'un créneau soit inventé**, et cette perte est **signalée** ligne à ligne (date, nombre, heures). Le conserver laisserait une durée que rien n'attribue dans une demi-journée que la réparation vient de déclarer canonique ; l'inventer serait pire encore. C'est le seul cas où ce verbe perd des heures, et l'opérateur le voit avant de confirmer.
- **R-SR-08** : une plage sans dégât est un **succès** (rien à réparer), jamais un refus : c'est ce qui permet de relancer le verbe pour **vérifier** son propre travail, et de le planifier. Rejouer la réparation ne change rien (idempotence). Sont refusés avec le code de sortie 4 : une plage qui finit avant de commencer, une date mal formée, une plage dont le journal dépasse le plafond de page (1 000 entrées).
- **R-SR-09** : après application, les brouillons de feuille de temps des jours touchés précèdent la correction : le compte rendu invite à relancer `aplan timesheet --date <jour>`.

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
- Le panneau se ferme via bouton ×, touche Escape ou clic sur le backdrop. Il n'a **pas de bouton d'enregistrement** : toute modification est écrite automatiquement (R77), et la fermeture pousse d'abord ce qui restait en attente.
- Un indicateur d'état remplace le bouton d'enregistrement dans le pied du panneau : « Modification… », « Enregistrement… », « ✓ Enregistré », et en cas d'échec « ⚠ Échec de l'enregistrement » avec un bouton « Réessayer ».
- L'en-tête du panneau porte un bouton de copie qui place le titre de la tâche dans le presse-papiers ; l'icône passe à une coche pendant ~1,5 s pour confirmer. Si le presse-papiers est indisponible (contexte non sécurisé) ou refusé, aucune confirmation n'est affichée.
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

#### US-080 : Voir le travail concurrent et arbitrer la journée par quarts

> En tant que Tech Lead, je veux voir **tout** le travail de la journée, y compris ce qui
> s'est déroulé en parallèle, puis décider moi-même comment chaque quart de journée se
> répartit, afin que la déclaration Gryzzly corresponde à ce que j'ai réellement fait.

**Le problème résolu.** L'ancien moteur modélisait la journée sur une seule piste : chaque
intervalle était crédité au signal qui l'ouvrait. Le 2026-08-10, trois sessions Claude
tournaient en parallèle ; le bloc 13:00–16:02 — le plus gros de la journée — a été attribué
en entier à la tâche qui avait journalisé la première après le déjeuner, et une tâche qui a
tourné tout l'après-midi n'a déclaré que **0,29 h**. Aucune correction n'était possible,
puisque la seule vue offerte était celle qui avait déjà perdu l'information.

**Critères d'acceptation :**

- **Voies de présence concurrentes.** L'écran et `aplan timesheet [--date YYYY-MM-DD]`
  affichent **une voie par tâche**, avec les intervalles où elle peut être *montrée* active.
  Les voies **se chevauchent** : deux sessions simultanées produisent deux voies sur les
  mêmes minutes.
- **Règle de l'ombre portée (45 minutes).** Une entrée de journal est un horodatage écrit
  *après* le travail. Elle atteste donc la période qui la précède : au plus
  **45 minutes** (la constante `MAX_CONTINUATION_GAP_MINUTES`, la même que la projection des
  créneaux — voir R-WL-13), et jamais au-delà de l'entrée précédente **de la même voie**.
  Ce second écrêtage est la correction du bug : découper contre les entrées des *autres*
  voies est précisément ce qui créditait un long intervalle à la première tâche journalisée.
- **Temps mesuré vs temps inféré.** Une réunion et un créneau d'activité **manuel** (minuteur
  lancé à la main) comptent pour leur durée réelle. Un créneau issu de la projection du
  journal (`source = worklog`) ne compte pas : il est déjà représenté par les entrées.
- **Quatre quarts de journée.** Les deux plages configurées sont coupées en deux :
  `08–10, 10–12, 13–15, 15–17` par défaut. Chaque quart répartit **sa propre durée** entre
  les voies présentes, au prorata des minutes de présence, arrondie à l'incrément Gryzzly
  (15 min par défaut). La présence est un **poids**, pas une revendication d'horloge :
  98 + 76 + 71 minutes de présence dans un quart de 120 minutes est normal et attendu.
- **Arbitrage manuel.** L'utilisateur peut fixer les heures d'une voie dans un quart ; la
  part devient **épinglée** et le reste du quart se rééquilibre autour d'elle. Une
  reconstruction ultérieure **conserve** les épingles.
- **Le total de la journée est la somme des quarts** (8 h avec les plages par défaut), et
  **non** `workday.daily_target_hours`. Un quart qui totalise sa propre durée par
  construction ne peut pas simultanément totaliser une fraction mise à l'échelle d'un autre
  objectif : l'objectif journalier devient un **repère signalé**, pas un facteur d'échelle.
- **Confiance par quart** : proportion de l'horloge du quart couverte par **au moins une**
  voie (union, jamais somme) — `HIGH` ≥ 75 %, `MEDIUM` ≥ 40 %, `LOW` sinon. La confiance du
  jour est la plus faible des quatre.
- **Journée absente.** Un quart couvert par une absence (`OUT_OF_OFFICE`) voit sa durée
  déclarable réduite d'autant ; à zéro il ne déclare rien.
- **Journée sans trace.** Une journée sans **aucune** présence à l'intérieur des plages ne
  déclare rien (total 0). Sans cette garde, chaque week-end déclarerait quatre quarts non
  attribués et huit heures que personne n'a travaillées.
- **Travail hors plage horaire signalé.** Les traces dont l'ombre tombe hors des plages
  configurées sont **rapportées** par voie, avec leur durée. L'ancien moteur en jetait
  vingt d'un coup (le 2026-08-10, tout ce qui suivait 17:00) sans trace nulle part.
- **Correction du projet Gryzzly depuis la voie.** La ligne d'une voie porte le projet
  qu'elle déclare (ou « sans projet Gryzzly ») ; un clic dessus ouvre le sélecteur de tâches
  Gryzzly (le même que la carte de tâche, groupé par projet, avec recherche). Le choix
  **assigne durablement** la tâche Gryzzly à la tâche aplan — donc le `gryzzly_project_id`
  snapshoté, voir US-007 — puis **reconstruit la journée** sans confirmation : les épingles
  survivent, et la correction vaut pour tous les jours suivants. Le sélecteur choisit un projet
  *via une tâche* et non un projet seul : Gryzzly impute les heures sur une tâche, et un
  projet sans tâche produirait des heures indéclarables. Les voies **sans tâche** (réunion,
  dépôt Git non rattaché) restent en texte simple — il n'y a pas de tâche à corriger, leur
  rattachement passe par une règle d'apprentissage. Une journée validée ou soumise n'offre
  pas le sélecteur.
- **Lignes projet dérivées.** Le tableau heures×projet est la **somme des parts** par projet
  Gryzzly. Deux tâches du même projet fusionnent ici — le cas normal — et leurs voies restent
  distinctes à l'écran pour que la fusion soit visible avant la déclaration. Il n'existe plus
  d'épinglage au niveau de la ligne : ce serait une seconde source de vérité que l'arbitrage
  ne pourrait pas expliquer.
- **Sous-commandes CLI** (édition sans REPL) :
  - `aplan timesheet set --quarter <1-4> <tâche> <heures>` — épingle une voie dans un quart.
    `<tâche>` est résolu contre les voies du jour (titre approximatif ou clé de voie) ; une
    correspondance ambiguë est un **refus** (code 3) avec les candidats listés, jamais une
    supposition — ces heures atteignent la facture d'un client.
  - `aplan timesheet validate [--date YYYY-MM-DD]` — valide la feuille de temps
  - `aplan timesheet off [--am|--pm] [--date YYYY-MM-DD]` — marque une demi-journée (ou la
    journée complète) comme indisponible

**Décision d'architecture — l'édition fine vit dans l'écran React :**

L'arbitrage par quart est visuel par nature : il faut voir les voies se chevaucher pour
juger. L'écran React porte l'édition complète (voies, éditeur par quart, pas de 15 min) ;
la CLI reste lisible et n'expose qu'un seul verbe d'édition. Les deux interfaces opèrent sur
le même modèle GraphQL.

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
  - **Régression corrigée (migration 013)** : cette alerte était **rejetée par la base**, dont la
    contrainte sur les types d'alerte ne connaissait que trois valeurs. Le passage entier avortait
    donc à chaque exécution — pas seulement l'alerte — depuis la mise en service de la
    fonctionnalité, sans autre trace que le journal du service. Voir R63.
- **Heure de déclenchement configurable :** accessible via la clé de configuration `workday.auto_reconstruct_hour` (0–23, défaut 18).
- **Latence de déclenchement :** le brouillon apparaît dans les **5 minutes** qui suivent l'heure
  configurée. Le job interroge le watermark toutes les 5 minutes (et non chaque minute) : pour une
  tâche de fin de journée, la réactivité à la minute n'apporte rien.
- **Le brouillon survit à une alerte en échec :** si la levée de l'alerte passive échoue, la
  reconstruction déjà enregistrée est **conservée** et le jour est considéré comme traité. Seule
  l'alerte manque, et l'échec reste tracé dans le journal du service (voir SPEC_TECHNIQUE § 10.9).

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
  - **Libellé d'un bloc** — un bloc doit dire ce qu'il représente, jamais un identifiant technique ni un marqueur d'ignorance :
    - bloc de travail **attribué** : le **nom** du projet Gryzzly (libellé « projet — client » du catalogue, source `gryzzlyTasks`). L'identifiant brut n'est utilisé qu'en repli, si le nom est absent du catalogue chargé.
    - bloc de travail **non attribué** : « **Non attribué** ». Anciennement « ?? », qui ne distinguait pas « pas de projet » de « nom inconnu ».
    - réunion : « réu » ; absence (hors bureau) : « absence ».
  - **Deuxième ligne d'un bloc — le nom de la tâche** : sous le nom du projet, le bloc affiche le **nom de ce dont il provient** (champ `originLabel`) — le **titre de la tâche** propriétaire pour un bloc de travail, le **sujet de la réunion** pour une réunion. Le nom du projet dit *pour qui* le temps est facturé, le nom de la tâche dit *ce qui* a été fait.
    - Rendu **plus petit** que le nom du projet (`text-[9px]` contre `text-[10px]`) et légèrement atténué (`text-white/70`), sur une seule ligne tronquée. La géométrie du bloc est inchangée (les deux lignes tiennent dans la hauteur existante).
    - **Origine inconnue** (commit sans clé Jira résolue, ou journée reconstruite avant l'ajout du champ) : le bloc n'affiche que la ligne projet. Jamais d'identifiant en remplacement.
    - **Blocs étroits** : la ligne projet disparaît sous ~10 % de la demi-journée, la ligne tâche sous ~18 % — une ligne plus petite exige plus de place, et un demi-caractère tronqué renseigne moins que rien.
  - **Infobulle d'un bloc** : `<libellé projet> · <nom de la tâche> · <heures>h` (le nom de la tâche est omis s'il est inconnu), complétée — quand le bloc est non attribué et que les signaux non résolus le permettent — des **notes de journal** correspondantes (`… — note 1 · note 2`). L'infobulle nomme toujours les deux, même quand le bloc est trop étroit pour afficher quoi que ce soit. La jointure se fait sur `sourceRef` (`wl:<uuid>`), partagé par les blocs et les signaux non résolus.
  - **Sous la timeline**, si le jour compte des signaux non résolus : le nombre de signaux **et la liste** de ces signaux (heure locale `HH:MM` + libellé de la note), en amber. Le compte seul n'indiquait pas quel travail était non attribué.
  - La timeline est **en lecture seule** : aucune édition par glisser-déposer, aucun clic pour modifier un bloc. Les modifications se font exclusivement via le sidebar.
  - Les espaces libres dans la timeline sont visuellement apparents (arrière-plan blanc/clair).
  - **Persistance de la timeline et des signaux non résolus** : les deux sont enregistrés sur le brouillon du jour (colonnes `blocks_json` et `unresolved_json`), donc un rechargement de page les retrouve à l'identique. Un jour reconstruit avant la migration 017 n'a pas de liste de signaux à afficher jusqu'à sa prochaine reconstruction. De même, un jour reconstruit avant l'ajout du nom de la tâche affiche ses blocs **sans** deuxième ligne — la timeline reste complète, seule la ligne tâche manque jusqu'à la prochaine reconstruction.

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
     - **L'enregistrement ne détruit pas la timeline** : éditer les heures par projet ne dit rien de *quand* le travail a eu lieu, la chronologie enregistrée et la liste des signaux non résolus sont donc reportées telles quelles sur le brouillon. Auparavant, « Enregistrer » les remettait à vide et la timeline restait blanche jusqu'à une nouvelle reconstruction.
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
  - La représentation visuelle est le pendant interactif du CLI. Les commandes `aplan timesheet set --quarter <1-4> <tâche> <heures>` et `aplan timesheet off` modifient la même base de données.
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
  `aplan remember "<titre>" [--kind decision|commitment|fact|preference] [--why "<contexte>"] [--project <projet>] [--to <personne>] [--task <tâche>] [--source-ref <réf>] [--contradicts <réf-souvenir>] [--confirm]`.
  `--kind` vaut `fact` par défaut ; `--to` est répétable.
- `--contradicts <réf>` enregistre une **supersession proposée** : « ce souvenir contredit celui-là ».
  Il accepte un UUID complet **ou** la référence courte affichée (`m:7c1`). **Rien n'est invalidé** :
  c'est une affirmation soumise au triage, qui tranchera entre `supersede` (le fait a changé) et
  `merge` (même fait, mieux écrit). C'est la forme structurée de ce que la consolidation écrivait
  auparavant en prose dans `--why`, que nulle surface ne pouvait lire ni appliquer.
- `--contradicts` est **refusé avec `--confirm`** : une proposition est une question posée à la file
  de validation, et un souvenir confirmé n'y entre pas — la question ne serait jamais tranchée et
  le conflit resterait affiché indéfiniment. Pour réviser un souvenir déjà actif, le verbe est
  `aplan memory supersede <ancien> --by <nouveau>` (US-093).
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
- Un candidat portant une **supersession proposée** (`--contradicts`, US-090) est affiché comme
  **contredisant le souvenir nommé** : sa référence courte `[m:xxx]` **et son titre**, sur une ligne
  sous le candidat. Le titre en fait partie : un identifiant seul ne dit pas *quelle* décision est
  contredite. Le triage voit donc le conflit dans la liste, sans avoir à ouvrir le `--why` et à y
  lire un paragraphe.
- Si le souvenir nommé n'est **déjà plus vrai** — une autre supersession a pu l'invalider entre le
  passage de 17 h 30 qui a proposé le conflit et le triage du lendemain matin — la ligne le signale
  et annonce que `supersede` le refusera. Un conflit périmé présenté comme vivant serait pire que
  pas de conflit du tout.
- `aplan inbox accept <id> [--kind <type>]` valide un candidat : il devient `active` et donc
  rappelable. `--kind` permet de le retyper au passage.
- **Jamais d'ajout muet** : si le candidat ressemble à un souvenir déjà actif, l'acceptation est
  **refusée**, rien n'est écrit, et les sosies sont affichés avec les trois issues possibles —
  fusionner, superséder, ou accepter explicitement via `--force`.
- `aplan inbox merge <id> --into <id>` : *même fait, meilleure formulation*. Une seule ligne
  survit — celle de la cible, qui conserve son identité et ses dates et reçoit la nouvelle
  formulation. Les personnes concernées des deux lignes sont conservées. **Cette opération efface
  l'historique** et n'est donc pas le choix par défaut.
- `aplan inbox supersede <id> [--replaces <id>]` : *le fait a changé*. **Les deux lignes
  survivent** ; l'ancienne reçoit sa fin de validité et un pointeur vers la nouvelle.
- **`--replaces` est facultatif** : omis, il retombe sur la supersession que le candidat **propose
  lui-même** (celle qu'affiche `aplan inbox`). C'est le cas que produit la consolidation, et cela
  évite de recopier à la main un identifiant déjà porté par la ligne. Un candidat qui ne propose
  rien est **refusé** (code 4) avec un message nommant `--replaces` — jamais une supersession de
  rien : lire « superseded » sans que rien ne soit invalidé serait le pire des résultats.
- **La proposition est consommée par le verdict, quel qu'il soit.** `accept`, `reject`, `merge` et
  `supersede` l'effacent tous : accepter, c'est répondre « non, c'est un fait nouveau » ;
  `supersede`, c'est l'honorer, et la fin de validité porte alors le même fait sous forme
  structurée. Un souvenir qui n'est plus `pending` ne porte donc **jamais** de proposition, et une
  proposition trouvée dans la base est toujours une question ouverte, jamais un vestige.
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

#### US-097 : Onglet Mémoire dans l'application web

> En tant que Tech Lead, je veux trier, chercher et alimenter ma mémoire depuis l'application web,
> afin que la file de validation ne dépende plus du seul CLI.

**Critères d'acceptation :**
- L'onglet `/memory` est accessible depuis la navigation principale (libellé « Memory »).
- **Zone 1 — Bandeau** : nombre de candidats à trier, décisions actives, engagements ouverts, âge de
  la consolidation. Un âge absent affiche « Never consolidated », un âge périmé « Consolidation has
  gone quiet (N days) » en orange — c'est le signal de santé de la couche : une file qui cesse de
  grossir veut d'ordinaire dire que le passage de 17h30 ne tourne plus, pas qu'il ne s'est rien
  passé.
- **Zone 2 — Recherche** : `recall`, avec bascule « Include history ». Chaque résultat porte son
  score ; une mémoire invalidée est barrée et nommée « No longer true — replaced by <id> » ; un
  candidat non validé porte le badge « awaiting validation ». Sans l'historique, un candidat
  `pending` n'apparaît pas : le filtre dur du recall est conservé tel quel côté UI (R44).
- **Zone 3 — File de validation** : une carte par candidat (type, date, titre, contexte,
  destinataires d'un engagement, marqueur « replaces an existing memory » quand le candidat propose
  une supersession) et quatre verdicts : Keep / Discard / Merge… / Replace….
  - Un refus pour quasi-doublon n'est **pas** une erreur : la carte bascule en arbitrage, nomme la
    mémoire proche et propose « Merge into it » ou « Add anyway » (`force`). Rien n'a été écrit.
  - Merge… et Replace… ouvrent un sélecteur de mémoire cible (recherche `recall`). Quand le candidat
    nomme déjà ce qu'il contredit (`proposedSupersedes`), Replace… n'ouvre aucun sélecteur : la
    mutation laisse le backend résoudre la cible, comme `aplan inbox supersede` sans `--replaces`.
  - File vide : « Nothing to triage — every candidate has a verdict. »
- **Zone 4 — Import** : champ de répertoire pré-rempli avec le dossier de mémoire du harness
  (surchargeable par `VITE_MEMORY_IMPORT_DIR`), rapport listant les mémoires importées et **chaque
  fichier ignoré avec sa raison** (`no frontmatter`, `already imported`). Une seconde passe sans
  nouveauté affiche « the store is already up to date » au lieu d'un rapport vide. Le chemin est
  résolu par le **backend**, sur son propre système de fichiers : il doit donc être absolu.
- **Création manuelle** : bouton « + New memory » ouvrant la même feuille que la capture, avec la
  case « Validate now (skip the queue) » qui pose `confirmed` (US-090).
- **Capture par sélection (Dashboard)** : toute sélection de plus de trois caractères fait
  apparaître une puce « + Memory » sous la sélection (raccourci Ctrl/Cmd+M). Elle ouvre la feuille
  pré-remplie : le titre reçoit la première phrase qui tient en 120 caractères, le reste part dans
  le contexte, et **le corps conserve la sélection entière** quand le titre a dû être élidé — un
  titre tronqué ne doit jamais être la seule copie survivante de ce que l'utilisateur a sélectionné.
  Si la sélection se trouve dans une carte de tâche, la mémoire est rattachée à cette tâche.
  - Un clic qui **termine une sélection** n'ouvre plus la feuille d'édition de la tâche : sans cette
    garde, la capture était inatteignable, l'ouverture de la feuille recouvrant la puce.
  - **La puce n'apparaît que si la sélection a une géométrie visible.** Une sélection sans rectangle
    exploitable (nœuds remplacés par un re-render, sélection sortie du cadre par défilement) ne
    déclenche aucune puce : positionner à partir d'un rectangle nul plaçait la puce dans le coin
    supérieur haut-gauche de l'écran, loin de ce que l'utilisateur avait sélectionné. La puce est par
    ailleurs maintenue **dans le cadre** — repositionnée au-dessus de la sélection quand le bas
    manque de place — et **disparaît d'elle-même** quand la sélection est perdue sans clic ni frappe.
- **Hors périmètre** : aucun compteur de mémoires actives dans le bandeau (l'API n'expose pas ce
  total), et pas de filtre par projet sur la recherche — `recall --project` est un **bonus de score**,
  pas un filtre.

**Priorité** : Must (lot 6)

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
| **R07b** | **Élagage des tâches obsolètes** : après une synchronisation, une tâche de la source dont l'identifiant n'est plus retourné est supprimée localement — **sauf si elle porte du travail consigné** (au moins une entrée de journal ou un créneau d'activité). Une tâche protégée cesse d'être rafraîchie mais reste présente ; seule une suppression explicite (`aplan rm`) l'enlève. Le travail consigné est une donnée de l'utilisateur, pas une donnée synchronisée : les entrées de journal sont en cascade sur la tâche et un créneau d'activité perd son attribution avec elle. |
| **R07c** | **Aucun élagage sur un lot vide** : une synchronisation qui **réussit** mais ne retourne aucun élément n'autorise aucune suppression. Un lot vide ne dit rien de l'obsolescence — clé de projet mal saisie, droit retiré, filtre « mes tâches uniquement » sur un compte modifié, requête JQL qui ne correspond plus à rien, calendrier entièrement filtré par les motifs d'exclusion. Le lire comme « tout est obsolète » supprimait l'intégralité du périmètre de la source. L'élagage est donc ignoré et la raison est inscrite dans l'état de synchronisation, sans faire échouer la synchronisation : les tâches déjà mises à jour sont conservées. Vaut pour les tâches (Jira, Excel) comme pour les réunions Outlook. |
| **R07d** | **Un élagage en échec est signalé, jamais avalé** : l'échec est ajouté aux erreurs de la synchronisation et remonte dans l'état de synchronisation. Il n'interrompt pas la synchronisation en cours (les tâches déjà créées ou mises à jour restent), mais il ne peut plus être rapporté comme « 0 tâche supprimée ». |

### 7.3 Dédoublonnage

| Règle | Description |
|-------|-------------|
| **R08** | Quand un numéro de ticket Jira est identifié dans une ligne Excel (quelle que soit la colonne), les deux entrées sont fusionnées automatiquement. La source Jira fait foi pour les champs communs (statut, titre, assigné). |
| **R09** | Quand il n'y a pas de clé commune, l'outil propose un rapprochement par similarité (titre, assigné, projet) avec un score de confiance, au-delà d'un seuil de 0,7. L'utilisateur valide ou rejette la fusion. |
| **R09a** | **Le titre décide seul.** Deux tâches portant le même titre atteignent 1,0 même sans assigné ni projet — cas de la quasi-totalité des tâches personnelles. Un assigné identique ou un projet identique ne fait que combler 10 % de l'écart restant : ces attributs confirment un titre déjà proche, ils ne peuvent jamais rapprocher deux titres sans rapport. |
| **R09b** | **La comparaison des titres porte sur les mots, pas sur les caractères.** Les titres sont découpés en mots (minuscules, accents et ponctuation neutralisés) puis appariés un à un ; le score est le coefficient de Dice sur les mots appariés. L'ordre des mots et le style de ponctuation n'ont donc aucun effet (`Azure Assessment` = `Assessment Azure`, `—` = `:`), et une faute de frappe à l'intérieur d'un mot reste absorbée par une distance d'édition appliquée mot à mot. |
| **R09c** | **Les mots que tout le backlog répète pèsent moins.** Chaque mot est pondéré par sa rareté parmi les tâches actives comparées. Un préfixe de projet, un nom de client ou un chemin de composant partagés ne suffisent donc pas à faire passer le seuil à deux tâches qui ne diffèrent que par le mot désignant leur phase (`… - Cadrage technique` vs `… - Développement`). Quand les tâches sont trop peu nombreuses pour distinguer les mots courants des mots rares, la pondération redevient uniforme. |
| **R09d** | **Deux occurrences d'une même récurrence ne sont jamais des doublons.** Une récurrence matérialise une tâche par date d'occurrence, toutes portant le titre de leur modèle : ces lignes sont le même travail replanifié, et les fusionner détruirait l'échéancier. Une paire partageant le même `recurrence_id` est donc écartée avant tout calcul de score. En revanche, une tâche saisie à la main en doublon d'une occurrence, ou deux récurrences distinctes générant le même travail (modèle dupliqué), restent proposées. |
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
| **R23b** | **Réattribution** : déplacer du temps d'une tâche à une autre se fait par les entrées de journal, jamais par les créneaux (US-RE). Les créneaux étant une projection des horodatages des entrées, ils sont supprimés puis redérivés dans les seules demi-journées concernées, pour les deux tâches uniquement. L'opération est en aperçu par défaut et n'écrit qu'avec confirmation explicite : elle réécrit un historique qui alimente la facturation. |
| **R23c** | **Réparation des orphelins** : un créneau qui a perdu son `task_id` (le `ON DELETE SET NULL` déclenché par un `INSERT OR REPLACE INTO tasks`) n'est jamais repointé — il est supprimé, et sa demi-journée réécrite depuis les entrées de journal, seules porteuses de l'attribution survivante (US-SR). La plage de jours est explicite et obligatoire, le périmètre réel est la demi-journée qui contient un orphelin, et un créneau `manual` sans tâche est protégé : ce n'est pas un dégât mais un minuteur lancé à la main. Aperçu par défaut, écriture sur confirmation explicite. |
| **R23d** | **Rédaction après coup** : une entrée de journal peut être placée à l'instant du travail plutôt qu'à celui de sa saisie (`aplan log --at`, ou « Edit timestamp » côté web). Comme la matérialisation ne cherche ses demi-journées que dans sa propre fenêtre — laquelle commence au démarrage de la session —, une entrée antidatée lui est invisible : le jour concerné est donc reconstruit explicitement (`aplan slots rebuild --task <T> --date <J>`, exécuté d'office par `--at`). Cette reconstruction ne réécrit que les demi-journées d'**une** tâche depuis des entrées toujours présentes : elle ne peut pas perdre d'heures, d'où l'absence de confirmation, contrairement à R23b et R23c. Les heures restent celles que l'étalement des entrées justifie (R-WL-13) : antidater ne fabrique pas de durée. |
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
| **R34** | **Conservation de la date d'origine** : une instance récurrente conserve sa date d'origine (`occurrenceDate`). Aucun mécanisme ne réécrit son `plannedStart`. La règle historique de report au lundi courant (`carryForwardTasks`), dont les instances récurrentes étaient exemptées, a été supprimée au profit de la remontée par affichage (R72–R74) : le retard d'une occurrence est désormais signalé sans que sa date bouge, et l'exemption n'a plus d'objet. |
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
| **R55** | **Budget du brief** : le rendu de `aplan brief` est plafonné à **40 lignes** et chaque ligne à **140 caractères**, plafonds appliqués dans le domaine et vérifiés par un test sur une entrée pathologique. Cette sortie entre dans le contexte du modèle à chaque session : un rendu non borné est une fuite de tokens permanente, pas un défaut cosmétique. La troncature est **toujours annoncée** (`(8, 6 affichés)`) et s'applique de la section la moins utile vers la plus utile : les décisions cèdent avant les engagements, qui cèdent avant les échéances, qui cèdent avant les **préférences**. Les préférences sont les dernières coupées : elles sont à la fois les plus utiles — une règle de méthode gouverne toute la session — et les moins chères, cinq lignes au plus. |
| **R56** | **Composition du brief** : sont retenus les souvenirs `preference` (les plus récents d'abord — une règle redite récemment est celle qui vaut ; plafond `MAX_PREFERENCE_ENTRIES` = 4, rendus **en tête** du brief), `commitment` (les plus anciens d'abord — un engagement pris il y a trois mois est celui qu'on a oublié) et `decision` (les plus récents d'abord — la question est « où en est le projet »), filtrés par R45. Le projet courant est celui de la tâche en cours de suivi, sauf `--project` explicite ; sans projet en focus, toutes les décisions actives sont montrées plutôt qu'une section vide. Les échéances sont classées par proximité d'aujourd'hui, dédoublonnées par titre, et purgées des fixtures de test. **Chaque souvenir affiché porte une référence courte réutilisable par `aplan recall`** : c'est tout le mécanisme de récupération à la demande. Le pied de page du brief de session nomme les **deux** verbes de recherche à la demande, jamais un seul : `aplan recall --q` pour les seuls souvenirs, `aplan search --q` pour les tâches, le journal et les réunions en plus des souvenirs — sans quoi le pied de page, seul pont entre le brief poussé à chaque session et une recherche que la session doit penser à faire elle-même, ne pointerait que vers la moitié du magasin. |
| **R58** | **Un seul résolveur d'identifiant de souvenir**. Tout argument d'identifiant — en lecture (`recall`) comme en écriture (`inbox accept`/`reject`/`merge --into`/`supersede --replaces`, `memory supersede --by`) — accepte l'UUID complet **ou** la référence courte affichée (`m:7c1`, `[m:7c1]`, `7c1`), et passe par la **même** résolution. Sans cette règle, le produit affiche une référence courte, laisse *lire* avec elle, puis la refuse dès qu'on veut *agir* : les commandes les plus fréquentes deviennent des recopies de 36 caractères. Un préfixe ambigu est refusé (code 3, candidats listés) et l'ambiguïté est évaluée sur **tout le magasin**, pas sur la seule file — sinon un préfixe unique aujourd'hui désignerait un autre souvenir demain. Un identifiant introuvable est un « non trouvé » (code 2), jamais une erreur générique. Les verbes à deux identifiants résolvent **les deux avant d'écrire** (corollaire de R50). |
| **R57** | **Visibilité de la panne de consolidation** : le brief affiche l'âge du dernier passage de consolidation dès qu'il dépasse **3 jours**, et « jamais exécutée » si aucun passage n'est enregistré. L'horodatage vit dans la table `configuration` (clé `memory.consolidation.last_run`) et non dans `sync_status`, dont la colonne `source` est sous une contrainte `CHECK` fermée. Une clé absente ou invalide se lit comme « jamais exécutée » : le brief rend la panne visible, il ne tombe pas avec elle. |
| **R59** | **Filigrane de consolidation PAR ENTRÉE, jamais curseur horodaté**. Une entrée de journal porte son propre marqueur (`worklog_entries.consolidated_at`) ; la consolidation lit exactement les entrées dont il est nul, de la plus ancienne à la plus récente. Un curseur horodaté sauterait **définitivement** toute entrée insérée tardivement avec un `loggedAt` antérieur au curseur, et rien ne signalerait la perte. Le marqueur est **idempotent** (`consolidated_at IS NULL` fait partie de la condition de mise à jour, le premier marquage gagne) et **posé après les écritures réussies** : un souvenir en double se rejette (R53), une entrée marquée sans souvenir est perdue. Marquer un lot est **atomique** (une transaction), pour qu'un lot ne puisse pas être marqué à moitié. |
| **R60** | **Garde de joignabilité de la consolidation** : la session planifiée vérifie que l'API répond **avant toute autre chose** et, si elle ne répond pas, **ne fait rien et ne pose aucun marqueur** — l'exécution suivante rattrape tout. La CLI étant un client GraphQL, sans cette garde une API arrêtée produirait une suite d'échecs silencieux. Corollaire : une consolidation à moitié faite n'existe pas ; soit le lot est traité et marqué, soit rien ne bouge. |
| **R61** | **Une supersession proposée est une donnée, pas de la prose**. Quand la consolidation juge qu'un candidat contredit un souvenir actif, elle enregistre l'ancien identifiant dans un champ dédié (`--contradicts`) et non dans le texte du `--why`. C'est ce qui permet à `aplan inbox` d'**afficher** le conflit — référence courte *et* titre du souvenir contredit — et à `aplan inbox supersede <id>` de s'en servir comme valeur par défaut de `--replaces`. Une proposition **n'invalide rien** : R46 reste entier, `invalidatedAt` n'a que ses trois écrivains humains. |
| **R62** | **Une proposition n'existe que sur un candidat en attente, et tout verdict la consomme**. Elle est refusée sur un souvenir qui saute la file (`--confirm`), et `accept`, `reject`, `merge` et `supersede` l'effacent tous — accepter répond « non, c'est un fait nouveau », `supersede` l'honore et la fin de validité porte alors le même fait sous forme structurée. Conséquence : un souvenir qui n'est plus en attente ne porte **jamais** de proposition, donc une proposition trouvée est toujours une question ouverte et jamais un vestige qui égarerait un lecteur. Un `merge` **n'hérite jamais** de la proposition du candidat : la porte anti-doublon (R52) offre `merge` et `supersede` sur la *même* paire, si bien que la proposition nomme d'ordinaire la cible du merge — l'hériter ferait proposer au survivant de se superséder lui-même. |
| **R63** | **Les valeurs autorisées d'un type d'alerte suivent le domaine**. La colonne `alert_type` est sous contrainte fermée en base : une variante ajoutée au domaine sans migration ne produit aucune erreur de type, seulement un échec d'insertion levé au fond du job d'arrière-plan qui émet cette alerte. C'est ainsi que l'alerte « feuille de temps prête » a fait **avorter la reconstruction de fin de journée à chaque passage** depuis sa mise en service, sans autre trace que le journal du service. La contrainte reste fermée — c'est un vrai garde-fou d'intégrité — mais un test insère une alerte de **chaque** variante, la liste étant écrite de façon exhaustive : ajouter une variante sans migration devient une erreur de compilation. |
| **R64** | **Recherche transverse** : `aplan search --q` cherche dans les tâches, les entrées de journal, les réunions et les mémoires. Pour une tâche, le titre et la description sont concaténés en **un seul texte à matcher** — comme une mémoire n'est qu'un seul document titre + corps — si bien qu'un terme trouvé dans le titre et un autre trouvé dans la description comptent ensemble ; les apparier séparément (tous les termes dans le titre, *ou* tous dans la description) ferait dépendre le résultat de l'entité touchée, exactement le défaut que cette fonctionnalité doit faire disparaître. Les résultats sont **groupés par entité**, jamais fusionnés en un classement unique : mélanger le score BM25 d'une mémoire à une correspondance de titre de tâche produirait un ordre qui ne veut rien dire. Les mémoires gardent l'ordre du rappel (pertinence), les autres entités sont triées par récence. Plafond de **5 résultats par groupe** (`SEARCH_MAX_PER_GROUP`), relevable par `--limit`, toute troncature annoncée — y compris pour les mémoires, dont le nombre trouvé doit pouvoir dépasser le nombre montré ; un groupe sans résultat est omis, jamais affiché vide. Les accents énumérés (aigus, carons, ogonek, ring, cédille, et les lettres à cédille du letton `ģ ķ ļ ņ`) sont pliés comme le fait `memories_fts` (`unicode61 remove_diacritics 2`), pour que la même requête se comporte pareil sur les quatre entités. Les lettres à barre ou ligature (`ł`, `ø`, `đ`, `æ`, `œ`, `ß`) ne sont pliées par **aucun des deux moteurs** et doivent être saisies telles quelles ; tout diacritique non énuméré reste un écart non mesuré — il peut plier sous FTS5 et donc se trouver dans les mémoires sans se trouver dans les tâches, le journal ou les réunions. Une requête vide ou blanche ne ramène **rien** — jamais tout : le magasin compte aujourd'hui 642 tâches, une saisie blanche ne doit pas en devenir le déversement. |
| **R61** | **La consolidation propose, elle n'applique jamais**. Tout ce qu'elle écrit est `PENDING` ; elle n'exécute aucun verbe de la file (`accept`, `merge`, `supersede`, `reject`) et n'emploie ni `--confirm` ni `--force`. Pour une décision qui contredit une décision active du même projet, elle **soumet une supersession en nommant l'ancien identifiant** et laisse l'utilisateur trancher entre supersession (le fait a changé) et fusion (même fait, mieux écrit) — cette distinction est un jugement sémantique, et le backend n'a aucun modèle (R50, R52). |
| **R62** | **Code de sortie 4 pour une précondition refusée**. Un état que le magasin refuse de quitter — candidat déjà `ACTIVE` ou `REJECTED`, cible de fusion non active, souvenir déjà invalidé, cycle de supersession, saisie sans rien de recherchable — sort en **4**, jamais en 1. Le 1 est réservé à « l'appel n'a pas abouti » (réseau, base). Un appelant automatisé doit distinguer les deux : le premier se saute, le second impose de reprendre tout le passage sans poser de marqueur. |

### 7.10 Routine de pauses

| Règle | Description |
|-------|-------------|
| **R65** | **Routine composée de plusieurs cadences superposées, éditée par l'utilisateur** : une règle de pause (`break_rules`) porte son propre rythme — soit un **intervalle** en minutes, soit une **heure quotidienne** —, sa durée annoncée, son intitulé et le corps de la notification (ce qu'il faut faire concrètement) ; l'utilisateur les édite dans l'écran de réglages. La migration en seed quatre, issues des recommandations ergonomiques (INRS : 5 min par heure d'écran intensif, pause active toutes les 30 min, pause visuelle ~20 min ; Cornell 20-8-2 pour la charge posturale statique ; renfort actif ciblé — une étude EMG montre qu'une pause passive seule ne change rien à la charge musculaire de l'épaule, alors qu'un renfort actif bref, lui, la réduit) : (1) **Pause visuelle** — toutes les 15 min, 30 s, priorité 1 ; (2) **Changement de posture** — toutes les 30 min, 2 min, priorité 2 ; (3) **Pause franche** — toutes les 60 min, 5 min, priorité 3 ; (4) **Renfo épaule** — quotidienne à 14:00, 2 min, priorité 4. La pause visuelle est seedée à **15 min** et non aux ~20 min de la recommandation 20-20-20, parce que ce qui compte ici est le rythme d'ensemble : 20/30/60 s'entrelacent — sur une heure les échéances tombent à :20, :30, :40, :00, soit des écarts de 10, 10 puis 20 minutes —, alors que 15/30/60 **coïncident** à :30 et à :00, où la fusion des collisions (R68) n'en laisse qu'une. L'utilisateur perçoit un quart d'heure régulier au lieu d'un rythme boiteux, et deux minutes d'écart avec la recommandation ergonomique valent bien une routine qu'on garde. |
| **R66** | **Horloge murale ancrée sur la fenêtre de travail, jamais sur le dernier déclenchement**. Les échéances d'une règle à intervalle sont `début_fenêtre + k × intervalle` : pour une fenêtre 08:00–12:00 et un intervalle de 20 min, cela donne 08:20, 08:40, 09:00… et la fenêtre de l'après-midi repart de son propre ancrage. Une pause manquée, reportée ou absorbée ne décale donc jamais la grille des suivantes. Aucune détection de présence ou d'inactivité : le moteur ne sait pas si l'utilisateur est au clavier, seulement quelle heure il est à l'intérieur des fenêtres `workday.*` configurées — un signal de présence a été explicitement écarté à la conception, hors du modèle d'aplan. |
| **R67** | **Suppression pendant réunion, report à sa fin plus une grâce**. Une échéance qui tombe pendant une réunion (filtrée sur son `show_as` Outlook via `aplan.breaks.suppressing_show_as`, défaut `busy,oof,tentative,free`) est reportée à la fin de la réunion plus une grâce configurable (`aplan.breaks.meeting_grace_minutes`, défaut 3 min) plutôt que d'être perdue ou déclenchée en pleine réunion — une réunion d'une heure ne doit pas coûter deux pauses, et sortir d'une réunion est un bon moment pour bouger. Des réunions dos à dos re-reportent simplement le réveil sur la nouvelle réunion plutôt que de le déclencher par-dessus. Un report **expire** quand il ne peut plus battre la prochaine échéance naturelle de sa propre règle : après une réunion d'une heure, le report de la pause visuelle (15 min) s'efface parce que sa propre échéance suivante serait déjà passée au moment où il se déclencherait, tandis que le report de la pause horaire survit et se déclenche. C'est ce qui empêche les reports de s'accumuler sans avoir à les compter. |
| **R68** | **Une seule notification par passage ; le report par « Plus tard » n'est pas un cas particulier**. Les cadences se recoupent par construction : à la 60ᵉ minute, les trois règles à intervalle (15/30/60) arrivent à échéance ensemble — et à la 30ᵉ, deux d'entre elles. `priority` existe pour trancher cette collision : le moteur déclenche au plus une notification par passage — la priorité la plus haute — et marque le reste `absorbed` (l'utilisateur ne les voit jamais). Sans cette règle, l'utilisateur prendrait trois popups par heure et désactiverait toute la routine en deux jours. Une mise en veille de la machine entre deux passages se comporte pareil : plusieurs échéances manquées ne produisent jamais de rafale de rattrapage, une seule se déclenche et les autres sont absorbées. Cliquer *Plus tard*, **là où le bouton est proposé** (R71), arme un report ordinaire, avec un autre motif que celui d'une réunion — il emprunte le même chemin que R67, expiration comprise, sans que le moteur de décision ait besoin de connaître la notion de snooze. |
| **R69** | **Deux ou trois boutons, six issues, dont deux volontairement distinctes**. La notification porte toujours *Pris* et *Passer*, et *Plus tard* seulement quand la cadence de la règle l'autorise (R71) ; chaque pause aboutit à l'une des six issues suivantes : (1) `taken` — l'utilisateur a cliqué *Pris* ; (2) `snoozed` — l'utilisateur a cliqué *Plus tard*, un report de suivi est armé (issue impossible sur une règle infra-horaire, qui n'offre pas le bouton) ; (3) `skipped` — l'utilisateur a cliqué *Passer*, refus délibéré de cette occurrence ; (4) `ignored` — la notification s'est fermée sans qu'un choix soit fait ; (5) `absorbed` — effacée par la fusion des collisions (R68), jamais vue par l'utilisateur ; (6) `expired` — ne pouvait plus utilement se déclencher (R67), ou n'a pas pu être affichée du tout, faute d'écran (API sans session graphique) : dans les deux cas la pause n'a jamais atteint l'utilisateur, et l'enregistrer en `ignored` ferait dire à la statistique qu'il a ignoré une pause qu'il n'a jamais vue. `skipped` et `ignored` sont délibérément distincts : ignorer systématiquement la pause de 15 minutes signale une cadence mal réglée, alors que la passer explicitement signale un timing mal choisi pour cette occurrence précise — deux corrections différentes. |
| **R70** | **Statistique d'adhérence, deux issues exclues des deux côtés**. Le taux d'adhérence par règle est `pris / vus`, où « vus » ne compte que les issues que l'utilisateur a effectivement pu voir (`taken` + `snoozed` + `skipped` + `ignored`) ; `absorbed` et `expired` sont exclus **du numérateur et du dénominateur** — la pause n'a jamais atteint un écran, la compter noierait un signal réel dans le bruit de planification. Le cas « rien de vu » rend un taux **absent**, jamais zéro, pour ne pas afficher une adhérence nulle là où la règle n'a simplement pas encore eu l'occasion de se manifester. |
| **R71** | **Une pause qui revient plus souvent que toutes les heures ne se reporte pas** : elle se prend ou elle se passe. En dessous de l'heure, la notification ne porte que *Pris* et *Passer* ; à partir de 60 min, et pour toute règle quotidienne, les trois boutons restent. La raison est un effet de composition, pas une question de goût : reporter une cadence courte ne déplace pas la pause, cela en ajoute une. Le report (10 min par défaut) retombe à l'intérieur d'un intervalle déjà court — sur une règle de 15 min, l'utilisateur reçoit le report puis l'échéance suivante de la grille dans les cinq minutes qui suivent —, là où sur une règle horaire il reste cinquante minutes de marge avant l'échéance suivante. C'est ce cumul qui a rendu la routine inutilisable dès son premier après-midi : neuf notifications en soixante-quatre minutes, dont deux dans la même minute, une seule pause horaire ayant produit à elle seule quatre notifications à force d'être reportée. Une règle quotidienne n'a qu'une échéance dans la journée et ne peut se cumuler avec rien : elle garde le report. La restriction vaut aussi côté écriture — une action `snoozed` qui arriverait malgré tout sur une règle infra-horaire (démon de notification rejouant une action périmée) est enregistrée en `skipped`, sans report de suivi, plutôt que de ressusciter le comportement qu'on vient de retirer. |

### 7.11 Retard et remontée au jour courant

| Règle | Description |
|-------|-------------|
| **R72** | **Le retard ne déplace plus aucune date**. Le mécanisme historique de report au lundi (`carryForwardTasks`), déclenché à chaque ouverture du dashboard, réécrivait le `plannedStart` de toute tâche non terminée antérieure au lundi courant en « lundi 08:00 UTC ». Il est supprimé — mutation, cas d'usage et déclencheur côté client. La raison n'est pas cosmétique : le dashboard n'affichant la date d'une tâche que par la colonne où elle est posée, réécrire `plannedStart` effaçait la seule trace visible du retard. Une tâche traînant depuis trois semaines et une tâche planifiée ce lundi se ressemblaient trait pour trait. Le retard devient donc un **fait constaté à l'affichage**, jamais une écriture : `plannedStart` et `deadline` sont désormais la propriété exclusive de l'utilisateur et des systèmes amont. |
| **R73** | **Deux niveaux de retard, l'échéance l'emporte**. Une tâche active (statut ni `Done` ni `Cancelled`) est en retard dès que `plannedStart` **ou** `deadline` est antérieur à aujourd'hui. Les deux cas ne disent pas la même chose et ne sont pas peints pareil : un `plannedStart` dépassé signale un **décalage de planification** — le travail n'a pas été fait le jour prévu, ce qui n'engage que l'utilisateur ; une `deadline` dépassée signale un **engagement rompu**, ce qui engage un tiers. Quand les deux sont vrais, c'est le niveau `Deadline` qui est retenu : le plus grave absorbe le moindre, plutôt que d'empiler deux marqueurs sur une même carte. Le nombre de jours de retard est compté depuis la date la plus ancienne du niveau retenu. Une tâche sans aucune des deux dates n'est jamais en retard — elle est *non planifiée*, ce qui est un autre état, traité par la colonne dédiée. |
| **R74** | **Remontée au jour courant, fondue dans la colonne**. Toute tâche en retard est affichée dans la colonne d'aujourd'hui, quel que soit le jour où ses dates la situent, et **le dashboard la charge même si elle est antérieure à la semaine affichée** — sans cet élargissement, supprimer le report ferait simplement disparaître les tâches en retard, la requête du dashboard ne balayant que la semaine courante. Les tâches en retard ne forment pas un bloc séparé : elles sont mêlées aux tâches réellement prévues ce jour, triées en tête (niveau `Deadline`, puis `Planned`, puis urgence décroissante), parce que la question posée au dashboard est « qu'est-ce que je fais aujourd'hui », et non « qu'est-ce que j'ai raté ». Le retard est porté par la carte elle-même : un anneau et un fond teintés — rouge pour `Deadline`, ambre pour `Planned` — **superposés à la bordure gauche qui, elle, continue de coder l'urgence**, plus une pastille indiquant l'ancienneté (`⚠ -5j`). Les deux signaux se lisent donc ensemble sans que l'un écrase l'autre. Corollaire de charge : une tâche en retard reste comptée dans la charge de la semaine courante, exactement comme le report l'y amenait auparavant — le travail est réel et doit peser. |
| **R75** | **La carte compacte affiche son échéance**. La carte de tâche possède deux rendus ; seul le rendu plein affichait l'échéance, alors que le dashboard — le seul écran où la date importe pour décider — utilise le rendu compact. L'échéance y était donc transmise puis jetée. Le rendu compact l'affiche désormais, avec la même icône de calendrier que le rendu plein. Sans cela, une pastille « -5j » n'aurait aucun référentiel lisible. |
| **R76** | **Édition de l'échéance réservée aux tâches personnelles**. L'échéance devient modifiable depuis le panneau d'édition et le panneau de création, mais uniquement pour les tâches `Source::Personal`. Sur une tâche Jira, Excel ou Outlook, elle reste en lecture seule, accompagnée de la mention de sa source : la synchronisation réécrit `deadline` sans condition à chaque passage, donc une saisie manuelle y serait silencieusement détruite au cycle suivant — un comportement pire que l'absence de champ. Poser ou modifier une échéance recalcule l'urgence selon R10–R14, sauf si l'urgence a été fixée manuellement (R15). L'échéance peut aussi être **effacée**, ce que l'API ne permettait pas d'exprimer : le champ y était optionnel au sens « ne change rien », si bien qu'un envoi de `null` était indistinguable d'une omission. |
| **R77** | **Le panneau d'édition enregistre en continu, sans bouton**. Le bouton *Save* du panneau d'édition de tâche est supprimé et *Cancel* devient *Fermer* : il n'y a plus qu'un seul chemin d'écriture, déclenché par la saisie elle-même. Deux régimes de déclenchement, selon ce que le champ signifie : un **choix arrêté** (statut, urgence, impact, date planifiée, échéance) s'enregistre **immédiatement**, parce que sortir d'un sélecteur ou d'un champ date est un acte complet ; un **texte en cours** (description, notes, heures estimées, surcharges Jira, délégué) s'enregistre **700 ms après la dernière frappe**, un seul appel pour toute une salve de touches. Le champ « Delegated to » est du texte malgré ses suggestions — c'est un `input` doublé d'une `datalist`, non un sélecteur —, donc il est différé comme les autres : l'enregistrer immédiatement produirait une mutation par touche. Fermer le panneau — bouton, Escape, clic sur le backdrop ou ✕ de l'en-tête — pousse d'abord l'édition en attente, puis ferme **si et seulement si l'écriture a abouti** : sur échec le panneau reste ouvert, avec son message et son action *Réessayer*. Fermer malgré tout perdrait la saisie sans le dire, ce qui serait plus mauvais que l'ancien bouton, lequel laissait au moins le panneau ouvert. **Changer de tâche sans fermer obéit à la même règle** : la recherche globale peut réaffecter le panneau à une autre tâche sans passer par la fermeture (le raccourci de recherche est lié à la fenêtre, donc atteignable panneau ouvert), et la bascule pousse l'édition en attente puis est **annulée si cette écriture échoue** — le panneau reste sur la tâche d'origine. Sans cela l'hydratation de la nouvelle tâche annulait le report en attente et la saisie disparaissait sans trace. « Ignorer cette occurrence » pousse également avant d'agir. **Ce que l'auto-enregistrement impose ailleurs dans le panneau** : le calcul du delta ne se fait plus contre la tâche rechargée mais contre un **instantané de ce que le serveur est connu détenir**, mis à jour après chaque écriture réussie, et l'hydratation des champs du formulaire ne se produit plus qu'au **changement d'identité de la tâche**. Sans ces deux points le dispositif est inutilisable : chaque mutation est suivie d'un rechargement `network-only`, qui réécrivait tous les champs — donc la frappe en cours — et rendait le delta faux pendant la fenêtre entre la mutation et l'arrivée du rechargement. Les champs dérivés (quadrant, heures effectives, statut Jira, journal) continuent, eux, de lire la tâche rechargée. **L'état d'échec n'est pas décoratif** : sans bouton d'enregistrement, un échec silencieux est une perte de données invisible — le pied du panneau affiche donc l'échec en clair avec une action *Réessayer*. Il fallait pour cela que l'échec existe : les mutations du panneau se résolvaient sans jamais signaler `result.error`, si bien que l'état d'échec était inatteignable — une promesse de retour que le panneau ne pouvait pas tenir. **Échec partiel** : une même écriture peut porter jusqu'à trois mutations, et si la deuxième échoue la première est déjà passée ; l'instantané de référence avance donc **par mutation réussie**, jamais en bloc — ce qui a abouti n'est pas renvoyé, ce qui a échoué n'est pas comptabilisé comme enregistré. De même, « Ignorer cette occurrence » qui échoue laisse le panneau ouvert et le dit, au lieu de fermer sur une écriture qui n'a pas eu lieu. **Conséquences assumées** : il n'y a plus d'annulation — quitter un champ, c'est l'avoir écrit ; et sur une tâche récurrente les champs du gabarit (description, urgence, impact, heures estimées) réécrivent la série entière dès la fin du délai, avec le même routage qu'avant (R-REC), mais sans le clic délibéré qui le rendait explicite. |

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

#### Règle de pause

Un rythme de la routine. C'est ce que l'écran de réglages liste et édite. Voir R65 à R70.

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| genre | Enum | Oui | `visual`, `posture`, `long`, `strength` — pilote l'icône et l'intitulé par défaut |
| intitulé | Texte | Oui | Titre de la notification |
| corps | Texte | Oui | Corps de la notification : ce qu'il faut faire concrètement |
| cadence | Enum | Oui | `interval` \| `daily`, exclusifs entre eux |
| intervalleMinutes | Entier | Non | Renseigné **seulement si** `cadence = interval` |
| heure | Heure (HH:MM) | Non | Renseigné **seulement si** `cadence = daily`, lue dans `aplan.timezone` |
| duréeSecondes | Entier | Oui | Durée annoncée de la pause, entre 1 et 3600 s ; le passage attend la notification en ligne, une valeur absurde immobiliserait la routine |
| priorité | Entier | Oui | Tranche les collisions de cadences (R68) et ordonne l'affichage |
| actif | Booléen | Oui | Défaut : vrai |
| urgence | Enum | Oui | `low`, `normal`, `critical` — transmise telle quelle à la notification système |
| créé_le, modifié_le | Date/heure | Oui | ISO 8601 |

#### Événement de pause

Une échéance. C'est la trace de ce qui s'est passé, et ce qui permet à un report de survivre à un redémarrage de l'API. Voir R65 à R70.

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| règle | Référence Règle de pause | Oui | Supprimée en cascade avec la règle |
| échéance | Date/heure | Oui | L'instant que la cadence a désigné |
| déclenchéLe | Date/heure | Non | Quand la notification est réellement partie ; `null` tant que reportée ou en échec de livraison |
| reportéJusqu'à | Date/heure | Non | Fin de réunion + grâce, ou cible du « Plus tard » |
| motifReport | Enum | Non | `meeting` \| `snooze` |
| réunionResponsable | Texte | Non | Trace d'audit : pourquoi la pause ne s'est pas déclenchée |
| issue | Enum | Oui | `pending`, `taken`, `snoozed`, `skipped`, `ignored`, `absorbed`, `expired` (R69) |
| réponduLe | Date/heure | Non | |
| créé_le | Date/heure | Oui | |

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
| aplan.session_idle_timeout_hours | Entier (heures) | 12 | Durée d'inactivité au-delà de laquelle le job de fond ferme automatiquement une session, après avoir matérialisé son temps de worklog. Relue à chaque passage. Valeur absente, illisible ou hors de `1..=8760` (un an) → repli sur le défaut, pour qu'une valeur corrompue ne ferme pas toutes les sessions ouvertes au tour suivant ni ne fasse déborder le calcul de la fenêtre |
| memory.consolidation.last_run | Texte (ISO 8601) | — | Date du dernier passage de la consolidation, écrite par `aplan consolidate record-run` et lue par le brief (R57, R59). Absente, illisible ou invalide → « jamais exécutée », sans faire échouer le brief. Stockée ici et non dans `sync_status`, dont la colonne `source` est sous contrainte `CHECK` fermée. |
| excelSharepointPath | Texte | — | Chemin du fichier Excel sur SharePoint |
| excelMappingConfig | Objet | — | Mapping colonnes Excel → champs de l'outil |
| obsidianVaultPath | Texte | — | Chemin du vault Obsidian (v2) |
| obsidianTaskTags | Liste de textes | `['#task']` | Tags Obsidian identifiant les tâches (v2) |
| aplan.breaks.enabled | Booléen | `true` | Interrupteur maître de la routine de pauses |
| aplan.breaks.meeting_grace_minutes | Entier (minutes) | `3` | Délai après la fin d'une réunion avant qu'une pause reportée se déclenche |
| aplan.breaks.snooze_minutes | Entier (minutes) | `10` | Horizon du bouton *Plus tard* |
| aplan.breaks.suppressing_show_as | Liste de textes (CSV) | `busy,oof,tentative,free` | Valeurs Outlook `show_as` qui suppriment une pause. Le défaut couvre toute entrée réelle de l'agenda : `tentative` marque une invitation sans réponse, pas une réunion non honorée, et les points internes récurrents sont couramment marqués `free`. Restreindre la liste est un choix explicite. |
| aplan.breaks.last_tick | Texte (ISO 8601) | — | Écrit par le job de fond ; sert de borne de départ (`since`) au passage suivant |

---

## 9. Cas particuliers / Cas limites

### 9.1 Sources de données

| Cas | Comportement attendu |
|-----|---------------------|
| **Source indisponible** (Jira down, réseau coupé) | L'outil utilise le cache local. Un indicateur montre que les données sont périmées avec la date de dernière synchronisation réussie. |
| **Excel modifié (structure changée)** | L'outil détecte les changements de structure et alerte l'utilisateur. Les données non mappables sont ignorées avec un avertissement. |
| **Tâche supprimée dans Jira** | La tâche disparaît de la vue après synchronisation (R07b). Si elle a des données locales (priorisation, tags), l'utilisateur est notifié. Si elle porte du **travail consigné**, elle n'est pas supprimée : elle cesse simplement d'être rafraîchie, et l'utilisateur seul décide de l'enlever. |
| **Synchronisation qui réussit mais ne retourne rien** | Aucune suppression (R07c) : le lot vide est traité comme une absence d'information, pas comme une obsolescence généralisée. L'élagage est ignoré et la raison est visible dans l'état de synchronisation. |
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
| **Pas de commande pour vider le bucket non-attribué** | Il n'existe pas de verbe CLI dédié pour affecter en masse les heures non attribuées d'un jour à un projet. Solution actuelle : épingler des heures quart par quart via `aplan timesheet set --quarter <1-4> <tâche> <heures>` jusqu'à atteindre le total, ou créer des règles de mappage via `aplan map add` pour suggérer automatiquement. |
| **`aplan map add` n'applique pas de priorité de sélecteurs** | La commande `aplan map add` accepte actuellement une seule règle (kind + pattern + gryzzlyProjectId) mais ne rejetse pas les collisions de sélecteurs distincts (p. ex., deux règles `repository` avec patterns différents). La logique de priorité (repository > subject > organizer > internal_project) sera appliquée et documentée dans une prochaine version afin de traiter élégamment les cas d'ambiguïté. |

---

## 11 bis. Apparence de l'application

### L'application entière porte la langue visuelle du HUD

Les douze onglets ne sont plus un outil clair posé à côté d'un overlay sombre :
ils lisent la **même palette que le bureau**. Repeindre le thème du poste
repeint l'application et le HUD ensemble — il n'y a pas deux identités à tenir
à jour.

Concrètement : fond sombre, police à chasse fixe, libellés en capitales
espacées, filets d'un pixel plutôt qu'ombres portées, angles nets. L'onglet
courant est marqué par une arête allumée dans la barre latérale, le même
langage que le panneau dominant du HUD — une seule chose est vive à la fois.

### Les arrivées

Ouvrir l'overlay et changer d'onglet déclenchent chacun une brève animation
d'entrée. Elle ne retarde rien : le contenu est affiché et lisible dès la
première image, l'animation ne fait que l'amener. Quelqu'un qui a demandé moins
de mouvement à son système reçoit la page immédiatement et immobile.

### La séquence de démarrage

Le petit texte de démarrage du cockpit ne joue qu'**une fois par session**, au
premier `SUPER+B`. Il a d'abord rejoué à chaque ouverture ; à l'usage, une
seconde et demie entre la frappe et les données ne le valait pas.

### Lisibilité

Le contraste a été mesuré onglet par onglet plutôt qu'apprécié à l'œil. Le pire
contraste de l'application est passé de 1,69:1 à 3,27:1, et six onglets sur onze
ne présentent plus aucun texte sous le seuil AA. Ce qui reste au-dessus de 3:1
sans atteindre 4,5:1 est exclusivement du libellé court et gras — pastilles,
compteurs, mots d'état — pour lesquels 3:1 est le seuil applicable.

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
| **Consolidation** | Relecture planifiée du journal de bord qui en extrait des souvenirs candidats. Assurée par une session Claude Code planifiée, pas par le backend : le dépôt ne contient aucun client de modèle. Écrit le `fact` actif d'emblée (`--confirm`) ; les autres (`decision`, `commitment`, `preference`) entrent en `pending`, dans la file de validation |
| **Filigrane de consolidation** | Marqueur porté par **chaque** entrée de journal (`consolidatedAt`), qui dit si la consolidation l'a déjà lue. Distinct d'un curseur horodaté, qui sauterait définitivement une entrée insérée tardivement mais datée du passé (R59) |
