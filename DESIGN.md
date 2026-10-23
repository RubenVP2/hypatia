---
name: Hypatia Design System
description: Système visuel habillage de régie esport pour observatoire local Riot Games
colors:
  bg: "#0B1220"
  surface: "#111A2C"
  raised: "#172238"
  line: "#24314B"
  text: "#E8ECF4"
  text-muted: "#8C98B0"
  text-faint: "#5F7091"
  src-riot: "#FF5A4F"
  src-lcu: "#3FA7FF"
  src-live: "#F2B33D"
  src-system: "#A98BFF"
  on-air: "#FF3B47"
  focus: "#FFD166"
  kind-create: "#3DD68C"
  kind-update: "#B9C3D6"
  kind-delete: "#FF7A6B"
typography:
  display:
    fontFamily: "'Barlow Condensed', system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 700
    lineHeight: "1.1"
    letterSpacing: "0.02em"
  ui:
    fontFamily: "'Barlow', system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 500
    lineHeight: "1.4"
    letterSpacing: "normal"
  data:
    fontFamily: "'JetBrains Mono', ui-monospace, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: "1.5"
    letterSpacing: "normal"
rounded:
  sm: "2px"
  md: "4px"
  chamfer: "8px"
spacing:
  gap-1: "4px"
  gap-2: "8px"
  gap-3: "12px"
  gap-4: "16px"
  gap-6: "24px"
components:
  top-bar:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    height: "48px"
  source-chip:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    padding: "4px 10px"
  phase-card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.chamfer}"
    padding: "12px 16px"
  ticker:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text-muted}"
    height: "32px"
---

# Système de Design Hypatia

## Overview

Hypatia est un tableau de bord et observatoire local pour les API de Riot Games (Riot Client, League Client, Live Client Data :2999 et connexions système). L'application s'exécute en mode **Operate** : conçue pour le joueur technophile sur un second écran dans une pièce sombre, elle présente l'état de la machine et du jeu à la manière d'un **habillage de diffusion esport (esports broadcast overlay)**.

Le système refuse expressément :
- Le panneau générique d'outil de développement gris avec un accent bleu unique.
- L'ornementation Hextech / fan-art League of Legends (pas de filigranes dorés, pas d'art de champions, aucun logo propriétaire de Riot Games).

La signature visuelle repose sur :
1. Les **cartons de phase biseautés** (`PhaseCard`) annonçant les transitions majeures (Salon, Recherche, Sélection des champions, En jeu).
2. La **frise temporelle proportionnelle** (`PhaseTimeline`) traduisant la durée réelle passée dans chaque phase.
3. Le **ticker d'antenne défilant** (`Ticker`) matérialisant le flux continu sous l'étiquette `EN DIRECT ●`.
4. L'attribution stricte des **couleurs d'équipe** à chaque source locale (Riot Client, League Client, Live Game, Système).

---

## Colors

### Rampe de neutres régie (stricte)
L'ambiance est celle d'un environnement régie de cast esport :

| Jeton | Code Hex | Rôle principal |
|---|---|---|
| `--n-0` | `#070B14` | Noir profond de fond de toile |
| `--n-1` / `--bg` | `#0B1220` | Toile de fond principale de l'application |
| `--n-2` / `--surface` | `#111A2C` | Panneaux, en-têtes, cartes et inspecteur |
| `--n-3` / `--raised` | `#172238` | Conteneurs surélevés, champs de saisie, chips |
| `--n-4` / `--line` | `#24314B` | Séparateurs, bordures et filets structurels |
| `--n-5` | `#3A4A6B` | Bordures actives et survols secondaires |
| `--n-6` / `--text-faint` | `#5F7091` | Métadonnées discrètes, séparateurs de chemin |
| `--n-7` / `--text-muted` | `#8C98B0` | Libellés secondaires, timestamps, unités |
| `--n-8` | `#B9C3D6` | Clés JSON, texte d'interface standard |
| `--n-9` / `--text` | `#E8ECF4` | Titres, données actives, texte à fort contraste |

### Couleurs d'équipe (Identité des sources)
Chaque source locale possède une teinte identitaire unique :

- **Riot Client** : `--src-riot: #FF5A4F` (Crimson)
- **League Client (LCU)** : `--src-lcu: #3FA7FF` (Bleu esport)
- **Partie en direct (:2999)** : `--src-live: #F2B33D` (Or compétitif)
- **Système (processus & ports)** : `--src-system: #A98BFF` (Violet surveillance)

### Rôles uniques et sémantiques
- **Point à l'antenne** : `--on-air: #FF3B47` (réservé exclusivement à l'indicateur EN DIRECT du ticker).
- **Focus clavier** : `--focus: #FFD166` (réservé au contour visible d'accessibilité clavier).
- **Types d'événements** :
  - Création : `--kind-create: #3DD68C` (Vert menthe)
  - Mise à jour : `--kind-update: #B9C3D6` (Neutre clair)
  - Suppression : `--kind-delete: #FF7A6B` (Corail)
  - Sondage actif : `--src-live: #F2B33D`
  - Appel direct : `--src-system: #A98BFF`

---

## Typography

Le système typographique combine trois familles distinctes sans redondance :

1. **Intitulés et chiffres d'impact (`Barlow Condensed`)** :
   - Utilisé pour le logo `HYPATIA`, les noms de phase en majuscules dans le `PhaseCard` (28px bold), les débits de la sparkline et les badges de statut.
   - Poids : 600, 700.
2. **Interface utilisateur (`Barlow`)** :
   - Utilisé pour la navigation, les boutons d'action, les onglets, les libellés de tableau et l'aide.
   - Poids : 400, 500, 600.
3. **Données et code (`JetBrains Mono`)** :
   - Utilisé pour les URIs d'API, les JSON Pointers, les représentations numériques et les payloads de l'inspecteur.
   - Règle absolue : `font-variant-numeric: tabular-nums` sur tous les nombres, timestamps et chronomètres pour éviter tout tremblement lors du rafraîchissement à haute fréquence.

---

## Layout

Le viewport de référence est **1440×900**, optimisé pour les écrans 1080p et 1440p secondaires :

- **TopBar (48px)** : Marque, indicateurs d'état des 4 sources, sparkline 60s, commutateur de masquage des secrets, recherche rapide `Ctrl+K` et onglets de vue.
- **SourceRail (280px)** : Arborescence hiérarchique latérale gauche permettant de filtrer par namespace ou endpoint, avec points d'activité instantanés.
- **Centre fluide** :
  - Vue *En direct* : Carton de phase signature, frise chronologique, et tableau virtualisé d'événements (28px par ligne).
  - Vues alternatives : *Endpoints* (catalogue exploratoire), *Système* (surveillance sockets et processus), *À propos* (garanties de sécurité).
- **Inspecteur latéral (440px)** : Volet droit d'inspection détaillée avec arborescence `JsonTree`, comparateur `JsonDiff` et affichage brut.
- **Ticker inférieur (32px)** : Bandeau continu en bas d'écran projetant les 30 derniers événements reçus.

---

## Elevation & Depth

Le mode Operate privilégie la clarté et la rapidité d'analyse sur les ombres décoratives :
- Séparation des plans par des **filets nets de 1px** (`--line: #24314B`) plutôt que des ombres portées lourdes.
- Ombres douces décalées avec flou progressif réservées exclusivement à la fenêtre modale de la `CommandPalette` (`box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6)`).
- Flou d'arrière-plan (`backdrop-filter: blur(8px)`) réservé au volet modal `Ctrl+K`.

---

## Shapes

- **Biseau signature (chamfer 8px)** : Réalisé par `clip-path: polygon(...)`. Strictement réservé aux cartons de phase (`.phase-card`) et aux étiquettes de source (`.chamfer-badge`).
- **Coins arrondis standards** : `--radius: 4px` pour les conteneurs et contrôles généraux ; `--radius-sm: 2px` pour les touches clavier (`<kbd>`).
- **Refus de l'antipattern "side-tab"** : Aucune bordure latérale épaisse (pas de `border-left: 4px/6px`) sur les cartes ou alertes. Les touches d'accentuation se font par le liseré de contour biseauté, des puces d'état colorées et des teintes douces en `color-mix`.

---

## Components

### 1. PhaseCard (Carton de phase)
- Biseau chamfer sur l'angle supérieur droit.
- Affiche le nom de la phase en capitales `Barlow Condensed` 28px.
- Intègre un chronomètre en direct de la durée passée dans la phase en cours et rappelle discrètement la phase précédente.

### 2. PhaseTimeline (Frise temporelle)
- Bande horizontale segmentée dont chaque bloc a une largeur **strictement proportionnelle à la durée réelle** passée dans cette phase.
- Infobulle informative affichant l'heure de début et la durée totale en secondes/minutes.

### 3. EventTable (Tableau virtualisé)
- Hauteur de ligne constante de 28px pour une virtualisation sans à-coups.
- Moins de 60 éléments DOM présents simultanément à l'écran, supportant sans saturation 300 événements par seconde.
- Raccourci `Espace` pour figer le flux et accumuler les nouveaux événements dans un compteur `+N nouveaux`.

### 4. JsonTree & JsonDiff
- `JsonTree` : Arborescence dépliable paresseuse avec coloration sémantique des types et copie de JSON Pointer au clic sur une clé.
- `JsonDiff` : Recalage clé par clé comparant la charge utile courante à la précédente pour mettre en exergue ajouts, suppressions et modifications.

### 5. SecretValue
- Masque tout mot de passe, jeton ou chaîne de signature sous la forme `🔒 •••• (N octets)`.
- Révélation sécurisée individuelle sur demande avec compte à rebours dynamique de **30 secondes** avant re-masquage automatique.

### 6. CommandPalette
- Ouverture globale sur `Ctrl+K` ou `Cmd+K`.
- Recherche prédictive sur l'ensemble des vues, des commandes et des endpoints découverts.

---

## Do's and Don'ts

### Do
- Conserver les chiffres tabulaires (`tabular-nums`) pour tout affichage de données, timestamps et chronomètres.
- N'utiliser les couleurs d'équipe (`--src-riot`, `--src-lcu`, etc.) que pour identifier la provenance des paquets.
- Réserver le rouge vif `--on-air` (`#FF3B47`) exclusivement au signal de transmission directe du ticker.
- Assurer une navigation complète au clavier sans recours obligatoire à la souris (`1-4`, `Espace`, `Ctrl+K`, `/`, Flèches).
- Respecter scrupuleusement la préférence utilisateur `prefers-reduced-motion` (remplacement des glissements par des fondus simples).

### Don'ts
- Ne jamais ajouter de bordure latérale épaisse type "side-tab" (`border-left: 4px+`) sur des conteneurs ou listes.
- Ne jamais intégrer d'images de champions, d'icônes ou de logos protégés de Riot Games.
- Ne jamais utiliser de dégradés sur le texte : la hiérarchie doit s'établir par la taille et la graisse typographique.
- Ne jamais afficher de secret en clair sans action volontaire et temporisée de l'utilisateur.
- Ne jamais exécuter de requête mutante (`POST`, `PUT`, `DELETE`) vers les API locales de Riot Games : l'observatoire demeure strictement en lecture seule.
