---
version: 1
slug: "crates-web-index-html"
primary_target: "crates/web/index.html"
related_targets: ["crates/web/src"]
---

# Surface brief — Dashboard principal (crates/web)

Scope : l'application web entière (une seule surface d'outil), en mode Operate.
Audience : le propriétaire du PC, joueur Riot technophile, sur un second écran dans une pièce sombre, pendant qu'il joue ou lors de sessions d'exploration.
Tâches : (1) voir ce qui tourne et ce qui est exposé, (2) suivre le flux d'événements en direct, (3) explorer un endpoint et ses données.
Contraintes : lecture seule, secrets masqués par défaut, coût CPU minimal, aucun asset de marque Riot.

## Direction contract

THESIS: Les surfaces locales de Riot présentées comme une diffusion que l'on observe. Les changements d'état du client arrivent comme des cartons de diffusion, le trafic défile comme le ticker d'un cast. La direction refuse le panneau DevTools gris avec un seul accent bleu, et refuse aussi le fan-art LoL : pas de filigrane Hextech doré, pas d'art de champion, pas de logo Riot.

OWN-WORLD: Fond bleu nuit de régie (#0B1220), surfaces des bandeaux (#111A2C, #172238), filets #24314B. Chaque source a sa couleur d'équipe, posée en liseré gauche biseauté : Riot Client crimson #FF5A4F, League Client bleu #3FA7FF, Partie en cours or #F2B33D, Système violet #A98BFF. Un point rouge « EN DIRECT » #FF3B47. Typo : Barlow Condensed pour les intitulés et les chiffres, Barlow pour l'UI, JetBrains Mono pour les données, avec des chiffres tabulaires. Angle biseauté de 8 px réservé aux cartons et aux étiquettes de source.

STORY: Dès l'ouverture, l'utilisateur voit quelles sources sont à l'antenne et sur quel port. Il voit ensuite la phase actuelle du client, puis le trafic qui défile. Il clique sur un événement ou un endpoint et lit ses données exactes, avec ce qui a changé. Il ne doute jamais que l'outil est en lecture seule.

FIRST VIEWPORT: 1440×900. Une barre haute de 48 px : nom, quatre puces de source (liseré couleur, état, port), sparkline des événements par seconde, interrupteur des secrets, recherche Ctrl+K. À gauche, un rail de 280 px avec l'arbre des endpoints par source et des points d'activité. Au centre, la vue En direct : le carton de phase et la frise des phases, avec des longueurs proportionnelles aux durées réelles, au-dessus du tableau virtualisé des événements. À droite, un inspecteur de 440 px avec l'arbre JSON et le diff. En bas, le ticker de 32 px. Aucune action primaire : l'action principale est de sélectionner.

FORM: Habillage de diffusion esport (IMPECCABLE'S PICK, rang 1 de ma liste). Signature : les cartons de phase et le ticker en direct. Ajouts empruntés : une durée montrée par une longueur exacte (labanotation), le recalage fixe pour le diff (folio botanique), l'accent réservé à un seul rôle (app grand public), des neutres sur une rampe stricte (planche d'exposition). seed:557514a5172f9f2c

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
