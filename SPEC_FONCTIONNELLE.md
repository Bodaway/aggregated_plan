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
| Alertes intelligentes | Détection de deadlines proches, surcharge, conflits de planning, retards |
| Dédoublonnage | Réconciliation des tâches présentes dans Jira ET Excel |
| Suivi d'activité | Journal d'activité par micro-interactions (sélection de la tâche en cours) |
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
| Authentification complexe (OAuth/EntraID) | Pas nécessaire pour un utilisateur unique |
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
- L'outil propose la liste des tâches en cours (agrégées de toutes les sources)
- L'utilisateur sélectionne la tâche active en un ou deux clics
- Le journal enregistre : tâche sélectionnée, heure de début, heure de fin (quand il change)
- L'interaction doit être rapide et non-intrusive (popup léger ou barre latérale)

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
  - Tâche associée (optionnel, sélectionnée dans la liste des tâches connues)
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
| **R16** | Une alerte de surcharge est émise lorsque la charge totale en heures (tâches planifiées + réunions) dépasse la capacité hebdomadaire. |
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

### 7.7 Configuration du fichier Excel

| Règle | Description |
|-------|-------------|
| **R24** | La structure du fichier Excel (nom des colonnes, onglet actif, plage de données) est configurable via les paramètres de l'application. |
| **R25** | La colonne contenant le numéro de ticket Jira est configurable (si elle existe). |
| **R26** | Le mapping entre les colonnes Excel et les champs de l'outil est configurable. |

---

## 8. Données et informations manipulées

### 8.1 Entités principales

#### Tâche (agrégée)

L'entité centrale de l'outil. Une tâche peut provenir de plusieurs sources.

| Attribut | Type | Obligatoire | Description |
|----------|------|-------------|-------------|
| id | Identifiant unique | Oui | Généré par l'outil |
| titre | Texte | Oui | Titre de la tâche |
| description | Texte | Non | Description détaillée |
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
