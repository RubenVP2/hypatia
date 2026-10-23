# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Rust partout, choix de l'utilisateur. Un binaire natif (Axum + tokio) lit les lockfiles, se connecte aux API locales Riot (TLS auto-signé, Basic Auth, WebSocket WAMP) et sert le frontend. Frontend en Leptos (CSR) compilé en WebAssembly. Un seul binaire à lancer, qui tourne en local uniquement (127.0.0.1).

## Users

Un seul utilisateur : le propriétaire du PC, qui joue aux jeux Riot (League of Legends, Valorant) et a une culture technique. Il veut savoir ce que le launcher Riot expose et fait transiter sur sa machine. Usage typique : sur un second écran à côté du jeu, souvent le soir dans une pièce sombre. Il utilise aussi l'outil lors de sessions d'exploration dédiées.

## Product Purpose

Un dashboard local qui rend visible en temps réel tout ce que les services Riot exposent sur la machine :
- les processus Riot et leurs ports en écoute ;
- les API locales (Riot Client, League Client/LCU, Live Client Data et Replay sur le port 2999) ;
- leurs endpoints et leurs données ;
- les événements qui transitent.

C'est réussi si l'utilisateur peut répondre en quelques secondes à « qu'est-ce qui tourne, qu'est-ce qui est exposé, qu'est-ce qui vient de transiter, et quelles données contient-il ».

## Positioning

Ce n'est pas un client alternatif ni un outil d'automatisation. C'est un instrument d'audit et d'observation, en lecture seule, qui couvre toutes les surfaces locales de Riot dans une seule vue en direct. Les outils existants (LCU Explorer, Needlework) n'explorent qu'une API à la fois et ne montrent pas le flux.

## Operating Context

- Trois usages, tous confirmés : auditer et comprendre, surveiller un flux d'événements pendant une partie, explorer les endpoints comme un outil de dev (appels GET et inspection des réponses).
- Les services Riot démarrent, s'arrêtent et changent de port et de mot de passe à chaque lancement. Le dashboard doit suivre ces cycles de vie sans intervention.
- Il tourne pendant que le jeu tourne : il doit rester léger en CPU et en mémoire.

## Capabilities and Constraints

- Lecture seule : seulement des requêtes GET et des abonnements WebSocket. Aucune requête POST, PUT, PATCH ou DELETE n'est envoyée aux API Riot.
- Secrets (mots de passe des lockfiles, access tokens, JWT d'entitlements, en-têtes Authorization) masqués par défaut, avec révélation explicite et temporaire.
- Le serveur n'écoute que sur 127.0.0.1.
- Les API LCU et Riot Client ne sont pas documentées officiellement. Leur liste d'endpoints est découverte à l'exécution (`/help`, schémas OpenAPI quand ils sont disponibles).
- Aucune donnée n'est envoyée hors de la machine.

## Evidence on Hand

Aucune donnée réelle n'est encore capturée. Les fixtures de développement doivent être marquées comme synthétiques.

## Product Principles

1. Tout voir : tout ce qui est local et observable est montré, rien n'est caché « pour simplifier ».
2. Le direct d'abord : l'état présent et ce qui vient de se passer priment sur l'historique.
3. Observer sans toucher : l'outil ne modifie jamais l'état des clients Riot.
4. Discret à côté du jeu : coût en ressources minimal, lisible d'un coup d'œil sur un second écran.

## Accessibility & Inclusion

Pas d'exigence spécifique confirmée. Le plancher par défaut s'applique : WCAG AA, navigation complète au clavier, respect de `prefers-reduced-motion`.
