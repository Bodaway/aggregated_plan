# Spécification Fonctionnelle

## Table des matières

1. [Vue d'ensemble et contexte](#1-vue-densemble-et-contexte)
2. [Acteurs et rôles](#2-acteurs-et-rôles)
3. [Authentification et autorisation](#3-authentification-et-autorisation)
4. [Gestion de portfolio de projets](#4-gestion-de-portfolio-de-projets)
5. [Planning de projet](#5-planning-de-projet)
6. [Staffing et affectation](#6-staffing-et-affectation)
7. [Intégration Teams](#7-intégration-teams)
8. [Persistance des données](#8-persistance-des-données)
9. [Interfaces utilisateur](#9-interfaces-utilisateur)
10. [Règles métier](#10-règles-métier)
11. [Cas d'usage détaillés](#11-cas-dusage-détaillés)
12. [Contraintes et exigences non-fonctionnelles](#12-contraintes-et-exigences-non-fonctionnelles)
13. [Glossaire](#13-glossaire)

---

## 1. Vue d'ensemble et contexte

### 1.1 Description du projet

**Aggregated Plan** est une application web de gestion de planning et de staffing pour équipes de développement logiciel. L'application permet de planifier les projets, gérer les affectations des développeurs et optimiser l'utilisation des ressources sur un portfolio de projets.

L'application est conçue pour être intégrée à Microsoft Teams, permettant aux équipes de gérer leur planning directement depuis leur environnement de travail collaboratif.

### 1.2 Objectifs métier

Les objectifs principaux de l'application sont :

- **Optimisation du staffing** : Répartir efficacement les développeurs sur les projets en fonction de leurs compétences et disponibilités
- **Visibilité du planning** : Offrir une vue claire et centralisée du planning de tous les projets du portfolio
- **Prévention des conflits** : Détecter automatiquement les surcharges et chevauchements d'affectations
- **Gestion de la capacité** : Suivre la charge de travail de chaque développeur et s'assurer qu'elle reste dans les limites acceptables
- **Traçabilité** : Conserver un historique des affectations et modifications de planning
- **Intégration Teams** : Faciliter l'accès et l'utilisation depuis l'environnement Teams

### 1.3 Contexte d'utilisation

L'application est destinée aux équipes de développement logiciel qui doivent :

- Gérer plusieurs projets simultanément (portfolio)
- Affecter des développeurs à différents projets avec des allocations variables (temps partiel, temps plein)
- Planifier le travail à la demi-journée (matin/après-midi)
- Suivre la charge de travail et détecter les surcharges
- Consulter et modifier le planning depuis Teams

### 1.4 Intégration Teams

L'application est intégrée à Microsoft Teams de deux manières :

- **Onglet Teams** : Interface principale accessible depuis un onglet dans Teams, permettant d'utiliser toutes les fonctionnalités de l'application
- **Bot Teams** : Bot conversationnel permettant d'interagir avec l'application via des commandes textuelles et de recevoir des notifications

L'intégration utilise le contexte Teams pour l'authentification et la synchronisation avec les calendriers des utilisateurs.

---

## 2. Acteurs et rôles

### 2.1 Administrateur

**Description** : Rôle avec les permissions les plus élevées, permettant la gestion complète de l'application.

**Permissions** :
- Gestion complète des projets (création, modification, suppression)
- Gestion des utilisateurs et des rôles
- Affectation de développeurs à des projets
- Consultation de tous les plannings et staffings
- Configuration de l'application
- Export des données
- Accès à toutes les vues et fonctionnalités

**Cas d'usage principaux** :
- Créer et configurer des projets
- Affecter des développeurs aux projets
- Gérer les permissions des utilisateurs
- Consulter les rapports et indicateurs

### 2.2 Développeur

**Description** : Rôle pour les membres de l'équipe de développement qui travaillent sur les projets.

**Permissions** :
- Consultation du planning des projets auxquels il est affecté
- Consultation de sa propre charge de travail
- Saisie de ses disponibilités
- Consultation de la vue intégrée planning/staffing (limité à ses projets)
- Modification de ses propres informations de disponibilité

**Cas d'usage principaux** :
- Consulter son planning et ses affectations
- Saisir ses disponibilités (congés, indisponibilités)
- Consulter la charge de travail
- Voir les projets auxquels il est affecté

### 2.3 Viewer (Lecteur)

**Description** : Rôle en lecture seule pour les personnes qui ont besoin de consulter le planning sans pouvoir le modifier.

**Permissions** :
- Consultation de tous les plannings et staffings
- Consultation de la vue intégrée planning/staffing
- Consultation des indicateurs et rapports
- Aucune permission de modification

**Cas d'usage principaux** :
- Consulter le planning global
- Visualiser la répartition des ressources
- Consulter les indicateurs de charge

---

## 3. Authentification et autorisation

### 3.1 Authentification OAuth 2.0 avec EntraID

L'application utilise l'authentification OAuth 2.0 avec Microsoft EntraID (anciennement Azure AD) pour sécuriser l'accès.

**Flux d'authentification** :

1. L'utilisateur accède à l'application depuis Teams ou directement via l'URL
2. Si non authentifié, redirection vers la page de connexion Microsoft
3. L'utilisateur s'authentifie avec ses identifiants EntraID
4. L'application reçoit un token d'accès et les informations de l'utilisateur
5. L'utilisateur est redirigé vers l'application avec une session active

**Informations récupérées depuis EntraID** :
- Identifiant unique (Object ID)
- Nom et prénom
- Adresse email
- Photo de profil (optionnel)
- Groupes de sécurité (pour déterminer les rôles)

### 3.2 Gestion des rôles et permissions

Les rôles sont attribués aux utilisateurs selon les règles suivantes :

- **Attribution automatique** : Basée sur les groupes de sécurité EntraID
- **Attribution manuelle** : Par un administrateur depuis l'interface
- **Rôle par défaut** : Viewer si aucun rôle n'est défini

**Matrice des permissions** :

| Fonctionnalité | Administrateur | Développeur | Viewer |
|----------------|----------------|-------------|--------|
| Créer un projet | ✅ | ❌ | ❌ |
| Modifier un projet | ✅ | ❌ | ❌ |
| Supprimer un projet | ✅ | ❌ | ❌ |
| Affecter un développeur | ✅ | ❌ | ❌ |
| Consulter tous les projets | ✅ | ❌ | ✅ |
| Consulter ses projets | ✅ | ✅ | ✅ |
| Saisir ses disponibilités | ✅ | ✅ | ❌ |
| Consulter la charge globale | ✅ | ❌ | ✅ |
| Consulter sa charge | ✅ | ✅ | ✅ |
| Exporter les données | ✅ | ❌ | ❌ |
| Gérer les utilisateurs | ✅ | ❌ | ❌ |

### 3.3 Intégration avec le contexte Teams

Lorsque l'application est utilisée depuis Teams :

- Le contexte Teams (tenant ID, user ID) est automatiquement récupéré
- L'authentification utilise le token Teams existant si disponible
- Les informations de l'utilisateur sont synchronisées avec EntraID
- Les permissions sont vérifiées à chaque requête

---

## 4. Gestion de portfolio de projets

### 4.1 Création d'un projet

Un projet peut être créé par un administrateur avec les informations suivantes :

- **Nom du projet** : Identifiant unique et descriptif
- **Description** : Description détaillée du projet
- **Dates** : Date de début et date de fin prévues
- **Statut** : En planification, En cours, En pause, Terminé, Annulé
- **Équipe** : Liste des développeurs affectés (peut être vide à la création)
- **Client/Porteur** : Personne ou entité responsable du projet (optionnel)
- **Priorité** : Haute, Moyenne, Basse (optionnel)

### 4.2 Modification d'un projet

Un administrateur peut modifier toutes les informations d'un projet existant, sauf :
- L'historique des affectations (non modifiable directement)
- Les dates passées (avec restrictions selon les règles métier)

### 4.3 Suppression d'un projet

Un projet peut être supprimé par un administrateur. La suppression :
- Supprime toutes les affectations associées
- Conserve un historique dans les logs (optionnel, selon configuration)
- Affiche un avertissement si le projet contient des affectations actives

### 4.4 Vue d'ensemble du portfolio

La vue d'ensemble affiche tous les projets du portfolio avec les informations suivantes :

- Liste ou grille de projets
- Informations essentielles : nom, statut, dates, nombre de développeurs affectés
- Indicateurs visuels : statut (couleur), priorité, charge de travail
- Filtres et tri : par statut, date, priorité, équipe

### 4.5 Filtres et recherche

Les utilisateurs peuvent filtrer et rechercher les projets selon :

- **Statut** : En planification, En cours, En pause, Terminé, Annulé
- **Date** : Projets dans une période donnée
- **Équipe** : Projets avec un développeur spécifique
- **Recherche textuelle** : Par nom ou description
- **Priorité** : Haute, Moyenne, Basse

### 4.6 Métadonnées projet

Chaque projet contient les métadonnées suivantes :

- **Identifiant unique** : Généré automatiquement
- **Nom** : Obligatoire, unique
- **Description** : Optionnel
- **Dates** : Début et fin (obligatoires)
- **Statut** : Obligatoire
- **Équipe** : Liste des développeurs affectés
- **Client/Porteur** : Optionnel
- **Priorité** : Optionnel
- **Date de création** : Automatique
- **Dernière modification** : Automatique
- **Créateur** : Utilisateur qui a créé le projet

---

## 5. Planning de projet

### 5.1 Vue calendrier

Le planning d'un projet peut être visualisé selon trois vues :

- **Vue jour** : Affichage détaillé d'une journée avec répartition matin/après-midi
- **Vue semaine** : Affichage d'une semaine avec les jours et demi-journées
- **Vue mois** : Vue d'ensemble mensuelle avec indicateurs de charge

### 5.2 Granularité : jour avec répartition matin/après-midi

Le planning utilise une granularité à la demi-journée :

- **Matin** : 8h00 - 12h00 (ou configuration personnalisée)
- **Après-midi** : 13h00 - 17h00 (ou configuration personnalisée)

Chaque affectation peut être :
- Sur une demi-journée spécifique (matin ou après-midi)
- Sur une journée complète (matin + après-midi)
- Sur plusieurs jours consécutifs

### 5.3 Création et modification de tâches/activités

Les tâches peuvent être créées et modifiées dans le planning :

- **Nom de la tâche** : Description de l'activité
- **Développeur affecté** : Un ou plusieurs développeurs
- **Période** : Dates de début et fin, avec sélection des demi-journées
- **Type de tâche** : Développement, Test, Documentation, Réunion, etc.
- **Statut** : À faire, En cours, Terminé
- **Estimation** : Temps estimé (optionnel)

### 5.4 Jalons et dates importantes

Les jalons peuvent être ajoutés au planning :

- **Nom du jalon** : Description
- **Date** : Date précise du jalon
- **Type** : Livraison, Revue, Démo, etc.
- **Projet associé** : Lien vers le projet
- **Visibilité** : Affichage dans les différentes vues

---

## 6. Staffing et affectation

### 6.1 Assignation détaillée (demi-journée)

Les développeurs peuvent être affectés à des projets avec une granularité à la demi-journée :

- **Sélection du projet** : Choix parmi les projets disponibles
- **Sélection du développeur** : Choix parmi les développeurs de l'équipe
- **Période** : Dates de début et fin de l'affectation
- **Demi-journées** : Sélection précise des matins et après-midis
- **Validation** : Vérification des conflits et de la capacité

**Exemple** : Affecter Jean Dupont au projet "API Backend" du 15/01/2024 au 20/01/2024, les matins uniquement.

### 6.2 Affectation par allocation hebdomadaire

Les développeurs peuvent être affectés à un projet avec une allocation hebdomadaire :

- **Sélection du projet** : Choix parmi les projets disponibles
- **Sélection du développeur** : Choix parmi les développeurs de l'équipe
- **Allocation** : Temps par semaine exprimé en :
  - **Jours/semaine** : Ex. 2 jours/semaine, 3 jours/semaine
  - **Pourcentage** : Ex. 50% du temps, 75% du temps
  - **Demi-journées/semaine** : Ex. 6 demi-journées/semaine
- **Période** : Dates de début et fin de l'allocation
- **Répartition** : Automatique ou manuelle (quels jours de la semaine)

**Exemples d'allocations** :
- 2 jours/semaine = 4 demi-journées/semaine = 40% du temps (sur 5 jours)
- 50% du temps = 2.5 jours/semaine = 5 demi-journées/semaine
- 3 demi-journées/semaine = 1.5 jours/semaine = 30% du temps

**Conversion automatique** : L'application convertit automatiquement les allocations hebdomadaires en affectations détaillées (demi-journées) selon la répartition configurée.

### 6.3 Gestion des capacités

Chaque développeur a une capacité définie :

- **Capacité par défaut** : 5 jours/semaine (100% du temps)
- **Capacité personnalisée** : Peut être modifiée selon les contrats (temps partiel, etc.)
- **Disponibilités** : Périodes où le développeur n'est pas disponible (congés, formations, etc.)
- **Charge actuelle** : Calcul automatique de la charge de travail actuelle

**Calcul de la capacité disponible** :
```
Capacité disponible = Capacité totale - Congés - Affectations actuelles
```

### 6.4 Détection de conflits

L'application détecte automatiquement les conflits suivants :

- **Surcharge** : Un développeur est affecté à plus de 100% de sa capacité
- **Chevauchement** : Un développeur est affecté à plusieurs projets sur la même demi-journée
- **Dépassement de capacité** : Les affectations dépassent la capacité disponible
- **Conflit avec disponibilités** : Affectation pendant une période d'indisponibilité

**Alertes** :
- Affichage visuel des conflits (couleurs, icônes)
- Messages d'erreur lors de la création d'affectations conflictuelles
- Notifications pour les administrateurs

### 6.5 Vue de charge par personne

Cette vue affiche pour chaque développeur :

- **Charge totale** : Pourcentage de temps affecté
- **Répartition par projet** : Détail des affectations
- **Capacité disponible** : Temps restant disponible
- **Période** : Vue sur une semaine, un mois, ou une période personnalisée
- **Graphiques** : Visualisation de la charge (barres, camembert)

### 6.6 Vue de répartition par projet

Cette vue affiche pour chaque projet :

- **Équipe affectée** : Liste des développeurs et leur allocation
- **Charge totale** : Nombre de jours-homme ou pourcentage
- **Répartition temporelle** : Charge par semaine/mois
- **Besoins** : Besoins en ressources vs ressources affectées
- **Graphiques** : Visualisation de la répartition

### 6.7 Calcul automatique des allocations

L'application calcule automatiquement :

- **Conversion allocation ↔ affectations** : Conversion entre allocation hebdomadaire et affectations détaillées
- **Somme des allocations** : Vérification que la somme des allocations d'un développeur ne dépasse pas 100%
- **Charge par période** : Calcul de la charge sur une période donnée
- **Capacité disponible** : Calcul en temps réel de la capacité restante

---

## 7. Intégration Teams

### 7.1 Onglet Teams

L'application est accessible depuis un onglet Teams :

- **Installation** : L'onglet peut être ajouté à un canal ou à une conversation
- **Interface complète** : Toutes les fonctionnalités de l'application sont disponibles
- **Authentification automatique** : Utilise le contexte Teams pour l'authentification
- **Responsive** : S'adapte à la taille de l'onglet dans Teams

**Fonctionnalités disponibles dans l'onglet** :
- Toutes les vues (portfolio, planning, staffing, vue intégrée)
- Création et modification de projets (selon permissions)
- Affectation de développeurs
- Consultation des indicateurs

### 7.2 Bot Teams

Le bot Teams permet d'interagir avec l'application via des commandes :

**Commandes principales** :
- `/planning [projet]` : Affiche le planning d'un projet
- `/charge [développeur]` : Affiche la charge d'un développeur
- `/affectations [développeur]` : Liste les affectations d'un développeur
- `/projets` : Liste tous les projets
- `/aide` : Affiche l'aide avec toutes les commandes

**Notifications** :
- Alertes de conflits de staffing
- Rappels de jalons importants
- Notifications de nouvelles affectations
- Résumés hebdomadaires de charge

### 7.3 Synchronisation avec calendriers Teams

L'application peut synchroniser avec les calendriers Teams via Microsoft Graph API :

- **Lecture des calendriers** : Récupération des événements pour détecter les indisponibilités
- **Écriture dans les calendriers** : Création d'événements pour les affectations (optionnel)
- **Synchronisation bidirectionnelle** : Mise à jour automatique des disponibilités

**Données synchronisées** :
- Congés et absences
- Réunions et événements bloquants
- Disponibilités mises à jour automatiquement

---

## 8. Persistance des données

### 8.1 Base de données locale

Les données suivantes sont stockées localement :

- **Projets** : Toutes les informations des projets
- **Affectations** : Historique complet des affectations (détaillées et allocations)
- **Utilisateurs** : Informations des utilisateurs (cache local)
- **Disponibilités** : Disponibilités saisies manuellement
- **Configuration** : Paramètres de l'application
- **Historique** : Logs des modifications

**Avantages** :
- Performance : Accès rapide aux données
- Contrôle : Gestion complète des données
- Historique : Conservation de l'historique complet

### 8.2 Microsoft Graph API

Les données suivantes sont récupérées depuis Microsoft Graph API :

- **Utilisateurs EntraID** : Informations des utilisateurs (nom, email, photo)
- **Calendriers** : Événements et disponibilités depuis les calendriers Teams/Outlook
- **Groupes** : Groupes de sécurité pour la gestion des rôles
- **Contexte Teams** : Informations du contexte Teams (tenant, canal, etc.)

**Avantages** :
- Synchronisation : Données toujours à jour
- Intégration : Utilisation des données Microsoft existantes
- Cohérence : Données unifiées avec l'écosystème Microsoft

### 8.3 Synchronisation hybride

La synchronisation hybride combine les deux approches :

- **Données locales** : Projets, affectations, planning (source de vérité locale)
- **Données Graph API** : Utilisateurs, calendriers (synchronisation périodique)
- **Cache** : Mise en cache des données Graph API pour améliorer les performances
- **Synchronisation** : Mise à jour automatique ou manuelle des données externes

**Stratégie de synchronisation** :
- Synchronisation automatique : Toutes les heures ou selon configuration
- Synchronisation manuelle : Bouton de rafraîchissement dans l'interface
- Synchronisation à la demande : Lors de certaines actions (affectation, consultation)

---

## 9. Interfaces utilisateur

### 9.1 Vue portfolio

La vue portfolio affiche tous les projets sous forme de liste ou de grille :

**Informations affichées** :
- Nom du projet
- Statut (avec code couleur)
- Dates (début - fin)
- Nombre de développeurs affectés
- Charge totale (jours-homme ou pourcentage)
- Priorité (si définie)

**Fonctionnalités** :
- Filtres : Par statut, date, équipe, priorité
- Recherche : Par nom ou description
- Tri : Par nom, date, statut, charge
- Actions : Créer, modifier, supprimer (selon permissions)

### 9.2 Vue calendrier (planning)

La vue calendrier affiche le planning d'un projet ou de plusieurs projets :

**Vues disponibles** :
- **Jour** : Détail d'une journée avec matin/après-midi
- **Semaine** : Vue hebdomadaire avec jours et demi-journées
- **Mois** : Vue mensuelle avec indicateurs

**Informations affichées** :
- Tâches et activités
- Développeurs affectés
- Jalons
- Charge par période

**Fonctionnalités** :
- Navigation : Précédent/Suivant, Aller à une date
- Zoom : Ajustement de la période affichée
- Création : Ajout de tâches directement sur le calendrier
- Modification : Glisser-déposer pour modifier les dates

### 9.3 Vue staffing (matrice personne/projet)

La vue staffing affiche une matrice développeurs × projets :

**Informations affichées** :
- Lignes : Développeurs
- Colonnes : Projets
- Cellules : Allocation (pourcentage, jours/semaine, ou indicateur visuel)

**Fonctionnalités** :
- Filtres : Par développeur, par projet
- Tri : Par nom, par charge
- Indicateurs visuels : Couleurs pour les niveaux de charge
- Détails : Clic sur une cellule pour voir les détails de l'affectation

### 9.4 Vue capacité (charge de travail)

La vue capacité affiche la charge de travail de chaque développeur :

**Informations affichées** :
- Nom du développeur
- Capacité totale
- Charge actuelle (par projet)
- Capacité disponible
- Graphique de charge (barres, camembert)

**Fonctionnalités** :
- Filtres : Par période, par développeur
- Détails : Clic pour voir le détail des affectations
- Alertes : Indication visuelle des surcharges

### 9.5 Vue intégrée planning/staffing

La vue intégrée combine le planning des projets et le staffing des développeurs :

**Affichage** :
- **Axe horizontal** : Timeline (dates)
- **Axe vertical** : Projets et développeurs (groupés par projet)
- **Barres Gantt** : Planning des projets avec jalons
- **Affectations** : Lignes indiquant les affectations des développeurs aux projets
- **Indicateurs** : Charge, conflits, disponibilités

**Fonctionnalités** :
- **Zoom** : Ajustement de la période (semaine, mois, trimestre)
- **Filtres** : Par projet, par développeur, par statut
- **Navigation** : Défilement horizontal et vertical
- **Détails** : Clic sur une barre ou une affectation pour voir les détails
- **Création** : Ajout d'affectations directement sur la vue
- **Légende** : Codes couleur pour les statuts, charges, conflits

**Avantages** :
- Vision globale : Voir tous les projets et affectations en même temps
- Détection visuelle : Identifier rapidement les conflits et surcharges
- Planification : Faciliter la planification globale du portfolio

### 9.6 Dashboard avec indicateurs

Le dashboard affiche des indicateurs clés :

**Indicateurs principaux** :
- Nombre de projets actifs
- Nombre de développeurs affectés
- Charge moyenne par développeur
- Projets en retard (si dates définies)
- Conflits de staffing en cours
- Capacité disponible totale

**Graphiques** :
- Répartition des projets par statut
- Charge par projet (camembert)
- Évolution de la charge dans le temps (courbe)
- Répartition des développeurs par projet (barres)

**Fonctionnalités** :
- Période : Sélection de la période d'analyse
- Export : Export des indicateurs (PDF, Excel)
- Actualisation : Rafraîchissement manuel ou automatique

---

## 10. Règles métier

### 10.1 Validation des affectations

**Règles de validation** :

1. **Capacité maximale** : Un développeur ne peut pas être affecté à plus de 100% de sa capacité
   - Exception : Surcharge temporaire autorisée avec alerte (selon configuration)

2. **Chevauchements** : Un développeur ne peut pas être affecté à plusieurs projets sur la même demi-journée
   - Exception : Si autorisé explicitement (selon configuration)

3. **Périodes valides** : Les affectations doivent être dans des périodes valides
   - Date de début ≤ Date de fin
   - Période dans les limites du projet (si définies)

4. **Disponibilités** : Les affectations ne peuvent pas chevaucher des périodes d'indisponibilité
   - Exception : Si autorisé explicitement

### 10.2 Validation des allocations hebdomadaires

**Règles de validation** :

1. **Somme des allocations** : La somme des allocations hebdomadaires d'un développeur ne doit pas dépasser 100%
   - Calcul : Somme de toutes les allocations actives sur une période
   - Validation : Vérification à chaque création/modification d'allocation

2. **Cohérence avec disponibilités** : Les allocations doivent être cohérentes avec les disponibilités
   - Si un développeur est en congé une semaine, son allocation pour cette semaine doit être réduite

3. **Conversion automatique** : Les allocations hebdomadaires sont converties en affectations détaillées
   - Répartition : Selon la configuration (jours de la semaine, répartition uniforme ou personnalisée)
   - Validation : Les affectations générées doivent respecter les règles de validation

### 10.3 Calcul automatique de la charge

**Calculs effectués** :

1. **Charge par développeur** :
   ```
   Charge = (Somme des affectations) / Capacité totale × 100
   ```

2. **Charge par projet** :
   ```
   Charge projet = Somme des allocations des développeurs affectés
   ```

3. **Charge par période** :
   - Calcul de la charge sur une semaine, un mois, ou une période personnalisée
   - Prise en compte des disponibilités et congés

4. **Capacité disponible** :
   ```
   Capacité disponible = Capacité totale - Congés - Affectations actuelles
   ```

### 10.4 Conversion entre allocation hebdomadaire et affectations détaillées

**Règles de conversion** :

1. **Allocation → Affectations** :
   - Une allocation de 2 jours/semaine génère 4 demi-journées par semaine
   - Répartition selon la configuration (jours fixes ou répartition uniforme)
   - Génération automatique des affectations pour chaque semaine de la période

2. **Affectations → Allocation** :
   - Calcul de l'allocation moyenne à partir des affectations détaillées
   - Utilisé pour l'affichage et les calculs

3. **Synchronisation** :
   - Modification d'une allocation → Mise à jour des affectations détaillées
   - Modification d'affectations détaillées → Recalcul de l'allocation moyenne

### 10.5 Alertes et notifications

**Types d'alertes** :

1. **Surcharge** : Un développeur est affecté à plus de 100% de sa capacité
   - Niveau : Erreur (bloque la création) ou Avertissement (selon configuration)

2. **Chevauchement** : Un développeur est affecté à plusieurs projets sur la même demi-journée
   - Niveau : Erreur (bloque la création) ou Avertissement

3. **Dépassement de capacité** : Les affectations dépassent la capacité disponible
   - Niveau : Avertissement

4. **Conflit avec disponibilités** : Affectation pendant une période d'indisponibilité
   - Niveau : Avertissement

**Notifications** :
- Notifications en temps réel dans l'interface
- Notifications Teams (via le bot)
- Emails (optionnel, selon configuration)

### 10.6 Historique et traçabilité

**Éléments tracés** :

1. **Modifications de projets** :
   - Qui a modifié quoi et quand
   - Valeurs avant/après

2. **Affectations** :
   - Création, modification, suppression d'affectations
   - Historique complet des changements

3. **Allocations** :
   - Modifications des allocations hebdomadaires
   - Calculs et conversions effectués

**Consultation** :
- Historique accessible depuis l'interface
- Export de l'historique (optionnel)
- Logs détaillés pour les administrateurs

---

## 11. Cas d'usage détaillés

### UC-001 : Authentification via EntraID

**Acteur** : Tous les utilisateurs

**Description** : Un utilisateur s'authentifie dans l'application en utilisant ses identifiants EntraID.

**Préconditions** :
- L'utilisateur a un compte EntraID valide
- L'application est configurée pour l'authentification EntraID

**Scénario principal** :
1. L'utilisateur accède à l'application (depuis Teams ou directement)
2. Si non authentifié, l'application redirige vers la page de connexion Microsoft
3. L'utilisateur saisit ses identifiants EntraID
4. L'utilisateur s'authentifie avec succès (MFA si configuré)
5. L'application reçoit le token d'accès et les informations de l'utilisateur
6. L'application détermine le rôle de l'utilisateur (depuis EntraID ou la base de données)
7. L'utilisateur est redirigé vers l'interface principale avec une session active

**Scénarios alternatifs** :
- **3a. Échec d'authentification** : L'utilisateur entre des identifiants incorrects → Message d'erreur, retour à l'étape 2
- **3b. Compte non autorisé** : Le compte n'a pas les permissions nécessaires → Message d'erreur, accès refusé

**Postconditions** :
- L'utilisateur est authentifié et a une session active
- Le rôle de l'utilisateur est déterminé et les permissions sont appliquées
- L'utilisateur peut accéder aux fonctionnalités selon son rôle

---

### UC-002 : Créer un projet

**Acteur** : Administrateur

**Description** : Un administrateur crée un nouveau projet dans le portfolio.

**Préconditions** :
- L'utilisateur est authentifié avec le rôle Administrateur
- L'utilisateur accède à la vue portfolio

**Scénario principal** :
1. L'administrateur clique sur le bouton "Créer un projet"
2. Un formulaire de création s'affiche
3. L'administrateur saisit les informations :
   - Nom du projet (obligatoire)
   - Description (optionnel)
   - Date de début (obligatoire)
   - Date de fin (obligatoire)
   - Statut : "En planification" (par défaut)
   - Client/Porteur (optionnel)
   - Priorité (optionnel)
4. L'administrateur clique sur "Créer"
5. L'application valide les données (dates cohérentes, nom unique)
6. Le projet est créé et sauvegardé
7. L'administrateur est redirigé vers la vue du projet créé

**Scénarios alternatifs** :
- **5a. Validation échouée** : Les données sont invalides → Messages d'erreur affichés, retour à l'étape 3
- **5b. Nom déjà existant** : Un projet avec le même nom existe déjà → Message d'erreur, suggestion de modifier le nom

**Postconditions** :
- Un nouveau projet est créé dans le portfolio
- Le projet est visible dans la vue portfolio
- Le projet peut recevoir des affectations de développeurs

---

### UC-003 : Affecter un développeur à une tâche (granularité demi-journée)

**Acteur** : Administrateur

**Description** : Un administrateur affecte un développeur à un projet pour des périodes spécifiques avec une granularité à la demi-journée.

**Préconditions** :
- L'utilisateur est authentifié avec le rôle Administrateur
- Un projet existe
- Un développeur existe dans le système

**Scénario principal** :
1. L'administrateur accède à la vue du projet ou à la vue staffing
2. L'administrateur clique sur "Affecter un développeur" ou sélectionne une période sur le calendrier
3. Un formulaire d'affectation s'affiche
4. L'administrateur sélectionne :
   - Le développeur (liste déroulante)
   - Le projet (pré-sélectionné si depuis la vue projet)
   - La date de début
   - La date de fin
   - Les demi-journées : Matin, Après-midi, ou Journée complète (case à cocher pour chaque jour)
5. L'administrateur clique sur "Valider"
6. L'application valide l'affectation :
   - Vérifie les conflits (surcharge, chevauchements)
   - Vérifie les disponibilités
   - Vérifie la capacité
7. L'affectation est créée et sauvegardée
8. L'application affiche un message de confirmation
9. La vue est mise à jour pour afficher la nouvelle affectation

**Scénarios alternatifs** :
- **6a. Conflit détecté** : Un conflit est détecté → Message d'erreur avec détails, possibilité de forcer (selon configuration) ou modifier l'affectation
- **6b. Disponibilité insuffisante** : Le développeur n'a pas assez de disponibilité → Message d'erreur, suggestion de périodes alternatives

**Postconditions** :
- Le développeur est affecté au projet pour les périodes spécifiées
- L'affectation est visible dans toutes les vues pertinentes
- La charge du développeur est mise à jour
- Les alertes de conflit sont générées si nécessaire

---

### UC-003b : Affecter un développeur à un projet avec allocation hebdomadaire

**Acteur** : Administrateur

**Description** : Un administrateur affecte un développeur à un projet avec une allocation hebdomadaire (temps par semaine) plutôt qu'une affectation détaillée jour par jour.

**Préconditions** :
- L'utilisateur est authentifié avec le rôle Administrateur
- Un projet existe
- Un développeur existe dans le système

**Scénario principal** :
1. L'administrateur accède à la vue du projet ou à la vue staffing
2. L'administrateur clique sur "Affecter avec allocation hebdomadaire"
3. Un formulaire d'allocation s'affiche
4. L'administrateur sélectionne :
   - Le développeur (liste déroulante)
   - Le projet (pré-sélectionné si depuis la vue projet)
   - La date de début de l'allocation
   - La date de fin de l'allocation
   - L'allocation : Saisie en jours/semaine, pourcentage, ou demi-journées/semaine
   - La répartition (optionnel) : Quels jours de la semaine (lundi, mardi, etc.) ou répartition automatique
5. L'administrateur clique sur "Valider"
6. L'application valide l'allocation :
   - Vérifie que la somme des allocations ne dépasse pas 100%
   - Vérifie les conflits avec les autres allocations
   - Vérifie la cohérence avec les disponibilités
7. L'application convertit l'allocation en affectations détaillées selon la répartition
8. L'allocation est créée et sauvegardée
9. Les affectations détaillées sont générées automatiquement
10. L'application affiche un message de confirmation
11. La vue est mise à jour pour afficher la nouvelle allocation

**Scénarios alternatifs** :
- **6a. Somme > 100%** : La somme des allocations dépasse 100% → Message d'erreur, affichage de la charge actuelle, suggestion de réduire l'allocation
- **6b. Conflit avec autres allocations** : Conflit détecté → Message d'erreur avec détails, possibilité de modifier les autres allocations
- **7a. Répartition impossible** : La répartition demandée n'est pas possible (ex: trop de jours demandés) → Message d'erreur, suggestion d'ajuster la répartition

**Postconditions** :
- Le développeur est affecté au projet avec l'allocation hebdomadaire spécifiée
- Les affectations détaillées sont générées automatiquement
- L'allocation est visible dans toutes les vues pertinentes
- La charge du développeur est mise à jour
- Les alertes de conflit sont générées si nécessaire

---

### UC-004 : Consulter le planning d'un projet

**Acteur** : Administrateur, Développeur, Viewer

**Description** : Un utilisateur consulte le planning détaillé d'un projet.

**Préconditions** :
- L'utilisateur est authentifié
- Un projet existe
- L'utilisateur a les permissions pour consulter le projet (tous pour Admin/Viewer, ses projets pour Dev)

**Scénario principal** :
1. L'utilisateur accède à la vue portfolio
2. L'utilisateur sélectionne un projet (clic ou recherche)
3. La vue planning du projet s'affiche
4. L'utilisateur peut choisir la vue :
   - Vue jour : Détail d'une journée avec matin/après-midi
   - Vue semaine : Vue hebdomadaire
   - Vue mois : Vue mensuelle
5. L'utilisateur navigue dans le planning :
   - Précédent/Suivant pour changer de période
   - Aller à une date spécifique
   - Zoom pour ajuster la période
6. L'utilisateur consulte les informations :
   - Tâches et activités
   - Développeurs affectés
   - Jalons
   - Charge par période

**Scénarios alternatifs** :
- **2a. Projet non trouvé** : Le projet n'existe pas ou n'est pas accessible → Message d'erreur, retour à la vue portfolio
- **4a. Aucune donnée** : Le projet n'a pas encore d'affectations → Message informatif, affichage du planning vide

**Postconditions** :
- L'utilisateur a consulté le planning du projet
- Les informations affichées sont à jour

---

### UC-005 : Consulter la charge d'un développeur

**Acteur** : Administrateur, Développeur

**Description** : Un utilisateur consulte la charge de travail d'un développeur.

**Préconditions** :
- L'utilisateur est authentifié
- Un développeur existe dans le système
- L'utilisateur a les permissions (Admin pour tous, Dev pour lui-même)

**Scénario principal** :
1. L'utilisateur accède à la vue capacité ou sélectionne un développeur
2. La vue de charge du développeur s'affiche
3. L'utilisateur consulte les informations :
   - Charge totale (pourcentage)
   - Répartition par projet (liste ou graphique)
   - Capacité disponible
   - Période d'analyse (semaine, mois, personnalisée)
4. L'utilisateur peut :
   - Filtrer par période
   - Voir le détail des affectations
   - Consulter les graphiques (barres, camembert)

**Scénarios alternatifs** :
- **1a. Développeur non autorisé** : Un développeur essaie de consulter la charge d'un autre développeur → Accès refusé, message d'erreur
- **3a. Aucune affectation** : Le développeur n'a pas d'affectations → Affichage de la charge à 0%, message informatif

**Postconditions** :
- L'utilisateur a consulté la charge du développeur
- Les informations affichées sont à jour

---

### UC-006 : Détecter les conflits de staffing

**Acteur** : Administrateur

**Description** : L'application détecte automatiquement les conflits de staffing et les affiche à l'administrateur.

**Préconditions** :
- L'utilisateur est authentifié avec le rôle Administrateur
- Des affectations existent dans le système

**Scénario principal** :
1. L'administrateur accède à la vue staffing ou à la vue intégrée
2. L'application analyse toutes les affectations :
   - Vérifie les surcharges (charge > 100%)
   - Vérifie les chevauchements (même développeur, même demi-journée, projets différents)
   - Vérifie les conflits avec disponibilités
3. L'application affiche les conflits détectés :
   - Indicateurs visuels (couleurs, icônes) sur les affectations en conflit
   - Liste des conflits avec détails
   - Messages d'alerte
4. L'administrateur consulte les conflits :
   - Voir les détails de chaque conflit
   - Identifier les développeurs et projets concernés
   - Voir les périodes en conflit
5. L'administrateur peut corriger les conflits :
   - Modifier les affectations
   - Supprimer des affectations
   - Ajuster les allocations

**Scénarios alternatifs** :
- **2a. Aucun conflit** : Aucun conflit détecté → Message informatif, affichage normal
- **4a. Conflits multiples** : Plusieurs conflits détectés → Affichage groupé, possibilité de corriger en masse

**Postconditions** :
- Les conflits sont détectés et affichés
- L'administrateur est informé des problèmes de staffing
- Les conflits peuvent être corrigés

---

### UC-007 : Saisir ses disponibilités (développeur)

**Acteur** : Développeur

**Description** : Un développeur saisit ses disponibilités (congés, indisponibilités) dans l'application.

**Préconditions** :
- L'utilisateur est authentifié avec le rôle Développeur
- L'utilisateur accède à son profil ou à la vue de ses disponibilités

**Scénario principal** :
1. Le développeur accède à la section "Mes disponibilités"
2. Le développeur clique sur "Ajouter une indisponibilité"
3. Un formulaire s'affiche
4. Le développeur saisit :
   - Type : Congé, Formation, Indisponibilité, Autre
   - Date de début
   - Date de fin
   - Description (optionnel)
5. Le développeur clique sur "Enregistrer"
6. L'application valide les données :
   - Vérifie que les dates sont cohérentes
   - Vérifie les conflits avec les affectations existantes (avertissement)
7. La disponibilité est enregistrée
8. L'application met à jour la capacité disponible du développeur
9. Un message de confirmation s'affiche

**Scénarios alternatifs** :
- **6a. Conflit avec affectation** : Une affectation existe pendant la période d'indisponibilité → Avertissement affiché, possibilité de continuer ou d'annuler
- **6b. Dates invalides** : Date de fin < Date de début → Message d'erreur, correction demandée

**Postconditions** :
- La disponibilité est enregistrée
- La capacité disponible du développeur est mise à jour
- Les affectations futures prennent en compte cette indisponibilité
- Une synchronisation avec le calendrier Teams peut être effectuée (optionnel)

---

### UC-008 : Synchroniser avec le calendrier Teams

**Acteur** : Tous les utilisateurs (automatique) ou Administrateur (manuel)

**Description** : L'application synchronise les disponibilités avec les calendriers Teams via Microsoft Graph API.

**Préconditions** :
- L'utilisateur est authentifié
- L'application a les permissions Graph API nécessaires
- Les calendriers Teams/Outlook sont accessibles

**Scénario principal** :
1. La synchronisation est déclenchée :
   - Automatiquement (selon la configuration, ex: toutes les heures)
   - Manuellement par l'utilisateur (bouton "Synchroniser")
   - Par un administrateur (synchronisation globale)
2. L'application se connecte à Microsoft Graph API
3. L'application récupère les événements des calendriers :
   - Pour tous les utilisateurs (admin) ou pour l'utilisateur connecté
   - Période : Semaine en cours + 4 semaines (ou selon configuration)
4. L'application analyse les événements :
   - Identifie les événements bloquants (congés, réunions importantes)
   - Extrait les périodes d'indisponibilité
5. L'application met à jour les disponibilités :
   - Crée ou met à jour les indisponibilités
   - Synchronise avec les disponibilités saisies manuellement
6. L'application met à jour la capacité disponible de chaque développeur
7. L'application affiche un message de confirmation avec le nombre d'événements synchronisés

**Scénarios alternatifs** :
- **2a. Erreur d'authentification** : Les permissions Graph API ne sont pas disponibles → Message d'erreur, demande de re-authentification
- **3a. Calendrier inaccessible** : Le calendrier d'un utilisateur n'est pas accessible → Avertissement, synchronisation partielle
- **5a. Conflit avec affectations** : Des affectations existent pendant des périodes maintenant marquées comme indisponibles → Alertes générées, notification à l'administrateur

**Postconditions** :
- Les disponibilités sont synchronisées avec les calendriers Teams
- La capacité disponible des développeurs est mise à jour
- Les alertes de conflit sont générées si nécessaire
- Les données sont à jour avec les calendriers externes

---

### UC-009 : Utiliser le bot Teams

**Acteur** : Tous les utilisateurs

**Description** : Un utilisateur interagit avec l'application via le bot Teams en utilisant des commandes textuelles.

**Préconditions** :
- L'utilisateur est dans Teams
- Le bot est installé dans le canal ou la conversation
- L'utilisateur est authentifié

**Scénario principal** :
1. L'utilisateur tape une commande dans le chat Teams (ex: `/planning Projet API`)
2. Le bot reçoit la commande
3. Le bot analyse la commande et les paramètres
4. Le bot exécute l'action demandée :
   - Récupère les données nécessaires
   - Formate la réponse
5. Le bot envoie la réponse dans le chat :
   - Texte formaté
   - Cartes interactives (si supporté)
   - Liens vers l'application web
6. L'utilisateur consulte la réponse

**Commandes disponibles** :
- `/planning [nom_projet]` : Affiche le planning d'un projet
- `/charge [nom_dev]` : Affiche la charge d'un développeur
- `/affectations [nom_dev]` : Liste les affectations d'un développeur
- `/projets` : Liste tous les projets
- `/aide` : Affiche l'aide avec toutes les commandes

**Scénarios alternatifs** :
- **3a. Commande inconnue** : La commande n'est pas reconnue → Message d'erreur, suggestion d'utiliser `/aide`
- **4a. Données non trouvées** : Le projet ou développeur n'existe pas → Message d'erreur, suggestion de vérifier le nom
- **4b. Permissions insuffisantes** : L'utilisateur n'a pas les permissions → Message d'erreur, accès refusé

**Postconditions** :
- L'utilisateur a reçu la réponse à sa commande
- Les informations affichées sont à jour

---

### UC-010 : Exporter le planning

**Acteur** : Administrateur

**Description** : Un administrateur exporte le planning ou les données de staffing dans un format externe (PDF, Excel, CSV).

**Préconditions** :
- L'utilisateur est authentifié avec le rôle Administrateur
- Des données de planning existent

**Scénario principal** :
1. L'administrateur accède à la vue qu'il souhaite exporter :
   - Vue portfolio
   - Vue planning d'un projet
   - Vue staffing
   - Vue capacité
   - Dashboard
2. L'administrateur clique sur le bouton "Exporter"
3. Un menu s'affiche avec les options :
   - Format : PDF, Excel, CSV
   - Période : Toutes les données, Période sélectionnée
   - Filtres : Appliquer les filtres actuels ou exporter tout
4. L'administrateur sélectionne les options
5. L'administrateur clique sur "Générer l'export"
6. L'application génère le fichier :
   - Récupère les données selon les options
   - Formate les données selon le format choisi
   - Génère le fichier
7. Le fichier est téléchargé automatiquement ou un lien de téléchargement est fourni
8. L'administrateur ouvre le fichier

**Scénarios alternatifs** :
- **6a. Erreur de génération** : Une erreur survient lors de la génération → Message d'erreur, suggestion de réessayer
- **6b. Fichier trop volumineux** : Les données sont trop importantes → Message d'erreur, suggestion de filtrer ou de réduire la période

**Postconditions** :
- Le fichier d'export est généré et téléchargé
- Le fichier contient les données demandées dans le format choisi
- Le fichier peut être partagé ou archivé

---

### UC-011 : Consulter la vue intégrée planning/staffing

**Acteur** : Administrateur, Viewer, Développeur (limité)

**Description** : Un utilisateur consulte la vue intégrée qui combine le planning des projets et le staffing des développeurs.

**Préconditions** :
- L'utilisateur est authentifié
- Des projets et affectations existent
- L'utilisateur a les permissions pour consulter les projets (tous pour Admin/Viewer, ses projets pour Dev)

**Scénario principal** :
1. L'utilisateur accède à la vue intégrée depuis le menu principal
2. La vue intégrée s'affiche :
   - Axe horizontal : Timeline (dates)
   - Axe vertical : Projets et développeurs (groupés par projet)
   - Barres Gantt : Planning des projets avec jalons
   - Lignes d'affectation : Affectations des développeurs aux projets
   - Indicateurs : Charge, conflits, disponibilités
3. L'utilisateur navigue dans la vue :
   - Défilement horizontal pour changer de période
   - Défilement vertical pour voir tous les projets
   - Zoom pour ajuster la période (semaine, mois, trimestre)
4. L'utilisateur applique des filtres (optionnel) :
   - Par projet
   - Par développeur
   - Par statut
5. L'utilisateur consulte les détails :
   - Clic sur une barre de projet pour voir les détails du projet
   - Clic sur une ligne d'affectation pour voir les détails de l'affectation
   - Survol pour voir un aperçu rapide
6. L'utilisateur identifie visuellement :
   - Les conflits (couleurs, icônes)
   - Les surcharges
   - Les disponibilités
   - La répartition des ressources

**Scénarios alternatifs** :
- **2a. Aucune donnée** : Aucun projet ou affectation n'existe → Message informatif, affichage de la vue vide
- **4a. Filtres trop restrictifs** : Les filtres ne retournent aucune donnée → Message informatif, suggestion d'ajuster les filtres

**Postconditions** :
- L'utilisateur a consulté la vue intégrée
- Les informations affichées sont à jour
- L'utilisateur a une vision globale du planning et du staffing

---

## 12. Contraintes et exigences non-fonctionnelles

### 12.1 Performance

**Temps de chargement** :
- Page principale : < 2 secondes
- Vue intégrée : < 3 secondes (même avec beaucoup de données)
- Synchronisation Graph API : < 5 secondes

**Optimisations** :
- Mise en cache des données fréquemment consultées
- Pagination pour les grandes listes
- Chargement progressif (lazy loading) pour les vues complexes
- Compression des données

### 12.2 Responsive design

**Support des appareils** :
- Desktop : Résolution minimale 1280x720
- Tablette : Support des tablettes (iPad, Android)
- Mobile : Support des smartphones (responsive, interface adaptée)

**Adaptations** :
- Menu adaptatif selon la taille d'écran
- Vues simplifiées sur mobile
- Touch-friendly : Boutons et zones de clic adaptés au tactile

### 12.3 Accessibilité

**Standards** :
- Conformité WCAG 2.1 niveau AA
- Navigation au clavier
- Support des lecteurs d'écran
- Contraste suffisant des couleurs

**Fonctionnalités** :
- Attributs ARIA pour les éléments interactifs
- Alternatives textuelles pour les images et graphiques
- Focus visible pour la navigation au clavier

### 12.4 Sécurité

**Authentification** :
- OAuth 2.0 avec EntraID (sécurisé)
- Tokens avec expiration
- Refresh tokens pour les sessions longues

**Données** :
- Chiffrement des données sensibles
- Validation des entrées utilisateur
- Protection contre les injections SQL/XSS
- HTTPS obligatoire

**Permissions** :
- Vérification des permissions à chaque requête
- Principe du moindre privilège
- Logs des actions sensibles

### 12.5 Disponibilité

**Objectif** :
- Disponibilité : 99% (environ 7 heures d'indisponibilité par mois)
- Maintenance planifiée : En dehors des heures de travail

**Mesures** :
- Sauvegarde régulière des données
- Monitoring et alertes
- Plan de reprise après sinistre

### 12.6 Compatibilité

**Navigateurs supportés** :
- Chrome (dernières 2 versions)
- Edge (dernières 2 versions)
- Firefox (dernières 2 versions)
- Safari (dernières 2 versions)

**Teams** :
- Teams Desktop (Windows, Mac)
- Teams Web
- Teams Mobile (iOS, Android)

---

## 13. Glossaire

| Terme | Définition |
|-------|------------|
| **Affectation** | Attribution d'un développeur à un projet pour une période spécifique avec une granularité à la demi-journée |
| **Allocation hebdomadaire** | Attribution d'un développeur à un projet exprimée en temps par semaine (jours/semaine, pourcentage, demi-journées/semaine) |
| **Capacité** | Temps de travail disponible d'un développeur, généralement exprimé en jours/semaine ou pourcentage |
| **Charge** | Pourcentage de temps de travail d'un développeur occupé par des affectations |
| **Chevauchement** | Situation où un développeur est affecté à plusieurs projets sur la même demi-journée |
| **Conflit** | Situation problématique détectée par l'application (surcharge, chevauchement, etc.) |
| **Demi-journée** | Unité de temps correspondant à une période de travail (matin ou après-midi) |
| **Disponibilité** | Période où un développeur n'est pas disponible (congés, formations, etc.) |
| **EntraID** | Service d'identité Microsoft (anciennement Azure AD) utilisé pour l'authentification |
| **Jalon** | Événement important dans le planning d'un projet (livraison, revue, démo, etc.) |
| **Portfolio** | Ensemble de tous les projets gérés dans l'application |
| **Projet** | Unité de travail avec un objectif, des dates, et une équipe de développeurs |
| **Staffing** | Processus d'affectation des développeurs aux projets |
| **Surcharge** | Situation où un développeur est affecté à plus de 100% de sa capacité |
| **Vue intégrée** | Vue combinant le planning des projets et le staffing des développeurs dans une seule interface |

---

## Historique des modifications

| Date | Version | Auteur | Description |
|------|---------|--------|-------------|
| 2024-01-XX | 1.0.0 | Équipe projet | Création du document de spécification fonctionnelle initial |

---

*Document maintenu à jour selon l'évolution du projet*
