# Handoff — FrameAutopsy (nom de travail)

> Document de passation destiné à Claude Code. Il est autonome : tu n'as pas accès à la conversation qui l'a produit.
> Langue de travail avec l'utilisateur : français. Code, identifiants, commits et documentation technique : anglais (conventions de ses autres dépôts : Conventional Commits `feat(scope): ...`).

---

## 0. Ton rôle

Tu es le **présentateur et l'initialisateur du projet**. À la première session :

1. Présente le projet à l'utilisateur en français, de façon claire et courte (sections 1 à 3 de ce document, avec tes propres mots, pas un copier-coller). Vérifie qu'il est d'accord sur la vision avant d'aller plus loin.
2. Exécute les tâches d'initialisation de la section 5, dans l'ordre.
3. Pose les questions ouvertes de la section 7, par lots de 4 maximum (outil `AskUserQuestion` si disponible), et consigne les réponses dans `docs/decisions.md`.
4. **N'écris pas de code applicatif tant que les spécifications OpenSpec de la phase 1 ne sont pas validées par l'utilisateur.** Il travaille spec d'abord, et il dirige : tu codes, il relit.

Ne devine pas ce que tu peux vérifier. Si une information de ce document te semble obsolète ou fausse, dis-le.

---

## 1. Contexte

- **Utilisateur :** étudiant en 3e année de bachelor, développeur autodidacte rapide, Windows 10/11 x86-64, portable avec GPU NVIDIA RTX 3060 Laptop (6 Go de VRAM), 16 Go de RAM ou plus, virtualisation activée.
- **Stack maîtrisée :** Python, TypeScript/SvelteKit, Rust niveau application (Tauri, tokio, reqwest, ratatui, rusqlite). Aucune expérience préalable de l'assembleur ni du Rust `unsafe` bas niveau.
- **Projet de référence (privé, lecture seule) :** `D:\Matteo\RiftMind`. C'est son projet le plus abouti : compagnon de League of Legends en Rust (workspace de crates `core`, `rofl`, `storage`, `poller`), avec un ring buffer de 120 échantillons, une machine à états sans allocation, un moteur de règles en TOML, un stockage MessagePack + zstd dans SQLite, 83 scénarios de test et un overlay Win32. Il sert de modèle d'architecture et de conventions. **Ne publie rien de ce dépôt privé.**
- **Contraintes de vie :** budget nul. Tout doit fonctionner avec des outils gratuits et en local. L'utilisateur consacre plus de 20 h par semaine au projet et accepte un horizon de plusieurs mois, mais il veut un premier résultat visible rapidement.

## 2. Vision

Un outil de bureau pour joueurs sur PC, utilisé tous les jours, qui **explique puis corrige les saccades et les pics de latence en jeu**, et **prouve** que ses corrections fonctionnent.

Les outils existants (LatencyMon, PresentMon, CapFrameX, HWiNFO, Process Lasso, le mode jeu de Windows) mesurent une tranche du problème ou appliquent des recettes génériques sans vérifier leur effet. Le projet se distingue par une boucle fermée complète :

1. **Observer** : frametimes, interruptions et DPC des pilotes, ordonnancement CPU, files du GPU, défauts de page, thermique, limites de puissance, E/S disque.
2. **Diagnostiquer** : un moteur de causes à règles déclaratives (TOML), alimenté par un buffer circulaire des dernières secondes, déclenché par un événement de saccade. Même philosophie que `CausalityStateMachine` de RiftMind.
3. **Agir** : corrections réversibles, appliquées uniquement aux **autres processus et au système**, jamais au jeu : priorité et mode économie (EcoQoS) de processus d'arrière-plan, CPU Sets pour les écarter des cœurs du jeu, report de tâches de maintenance, résolution du timer, profil d'alimentation.
4. **Prouver** : A/B par sessions alternées (correction activée ou non), puis comparaison statistique des pires frames (1 % et 0,1 %). Une règle qui ne prouve rien chez l'utilisateur est désactivée. Le rapport est lisible : « cette règle réduit tes saccades de X % sur ce jeu, sur cette machine ».

Cibles de jeux : League of Legends (lien avec RiftMind : croiser les événements de partie et les événements système), Fortnite, Overwatch, Minecraft Java (serveurs entre amis ; réglage de la JVM et diagnostic des pauses du ramasse-miettes). L'outil doit rester utile sur n'importe quel jeu en mode observation seule.

**Positionnement honnête :** sur un PC fixe propre, les gains seront modestes. Sur portable (thermique, limites de puissance, bascule GPU intégré/dédié, mode batterie) ils peuvent être réels. L'outil doit le dire quand il ne trouve rien.

## 3. Architecture cible (hypothèse de départ, à discuter)

- Workspace Rust, une crate par responsabilité (inspiration RiftMind) : `capture` (ETW, compteurs, NVML), `core` (buffer circulaire, moteur de règles, FSM sans allocation), `governor` (actions réversibles, journal, bouton d'arrêt d'urgence), `experiment` (A/B et statistiques), `storage` (SQLite WAL, MessagePack + zstd), `app` (interface bureau).
- Données : **ne jamais enregistrer les événements noyau bruts en continu** (centaines de Mo par minute). Garder la fenêtre en mémoire et ne persister que le contexte d'un incident. Plafond de rétention (30 jours glissants, 2 Go par défaut), configurable.
- Privilèges : la session de trace noyau (ETW) demande les droits administrateur. Le modèle d'élévation est une question ouverte (section 7).
- Hygiène du disque dès le premier commit : dans `Cargo.toml`, `[profile.dev] debug = "line-tables-only"` et `[profile.dev.package."*"] debug = false`. Documenter `cargo sweep --time 14`. (Le `target/` de RiftMind pèse 50,39 Go pour 60 Mo de code ; ne pas reproduire.)

---

## 4. Contraintes non négociables : conformité aux conditions des jeux

L'utilisateur reste **strictement dans les règles** des éditeurs. Cela vaut pour League of Legends (Riot), Fortnite (Epic), Overwatch (Blizzard) et Minecraft (Mojang/Microsoft).

### 4.1 Interdit, quel que soit le jeu

- Injection de DLL, hook de DirectX/Vulkan ou d'API dans le processus du jeu.
- Lecture ou écriture de la mémoire du jeu ; ouverture d'un handle sur le processus du jeu avec des droits de lecture mémoire ou de modification (pas nécessaire pour ETW, donc à éviter totalement).
- Modification des fichiers du jeu, de ses processus ou de sa priorité.
- Pilote noyau (risque de plantage et conflit avec les anti-triche).
- Automatisation des entrées, macros, contournement ou détection de l'anti-triche.
- Interception ou modification du trafic réseau du jeu.
- Tout avantage compétitif en direct (par exemple des conseils de jeu pendant que le joueur est en vie). RiftMind masque déjà le diagnostic tant que le joueur est vivant : conserver ce principe.

### 4.2 Probablement acceptable, **à vérifier avant usage**

- Session ETW système (même famille d'API que PresentMon) ; compteurs de performance ; NVML ; capteurs système.
- Live Client Data API de League of Legends (API locale officielle de Riot sur le port 2999), fichiers de replays `.rofl`.
- Fenêtre overlay **séparée** (topmost, sans injection), comme RiftMind. À confirmer pour chaque jeu.
- Pour Minecraft : journaux de la JVM (`-Xlog:gc`), arguments de la JVM dans le profil du lanceur de l'utilisateur, profilage côté serveur avec l'accord de l'administrateur du serveur.

### 4.3 Ce que tu dois faire

1. Cherche les textes officiels (conditions d'utilisation, règles de l'anti-triche, politiques sur les outils tiers et les API) pour les quatre jeux. N'utilise que des sources officielles ou fiables et cite l'URL et la date de consultation.
2. Écris `docs/compliance.md` : pour chaque jeu, anti-triche utilisé, éléments autorisés, zones grises, éléments interdits, verdict **autorisé / zone grise / interdit / non vérifié**. Ne présume jamais d'une règle de mémoire.
3. Si un point est « zone grise » ou « non vérifié », **ne l'implémente pas** et demande à l'utilisateur.
4. Encode la conformité dans le produit : une **matrice de capacités par jeu** (`compliance.toml`). Chaque observateur et chaque action y est activé ou non pour chaque jeu. **Tout jeu inconnu démarre en observation seule.** Ajoute un test qui échoue si une action est activée pour un jeu sans entrée de conformité.
5. Prévois de faire tester l'outil par l'utilisateur avec l'anti-triche actif sur chaque jeu avant toute diffusion, et note le résultat dans `docs/compliance.md`. Tu ne peux pas le garantir toi-même.

---

## 5. Tâches d'initialisation (dans l'ordre)

### 5.1 Lire les conventions de RiftMind (lecture seule)

Ouvre `D:\Matteo\RiftMind` : `CLAUDE.md`, `.claude/` (skills et réglages), `openspec/`, `ORCHESTRATOR_INSIGHTS.md`, `ROADMAP.md`, `PRODUCT_EVOLUTION_PLAN.md`, `.graphifyignore`, `Cargo.toml`. Reprends ce qui est réutilisable (structure, style de spécification, gardes) sans copier ses données ni ses secrets. Si le dossier n'est pas accessible, dis-le et continue.

### 5.2 Créer le dépôt

Dépôt local, `main` par défaut, `.gitignore` Rust (`target/`), `LICENSE` (question ouverte : MIT comme VibeDiff ?), `README.md` court, `CLAUDE.md` (conventions de la section 8), `Cargo.toml` de workspace avec les réglages de la section 3.

### 5.3 Initialiser OpenSpec

Installe et initialise OpenSpec dans le dépôt (probablement `openspec init`, mais vérifie la commande exacte avec `--help` ou la documentation officielle avant de l'exécuter). Crée ensuite les trois premières *changes*, sous forme de propositions seulement (pas d'implémentation) :

- `bootstrap-workspace` : workspace, CI locale, hygiène du disque.
- `compliance-matrix` : `compliance.toml`, tests de conformité, `docs/compliance.md`.
- `etw-frametime-capture` : phase 1 de la feuille de route, tray qui trace les frametimes et affiche une timeline.

### 5.4 Initialiser Graphify

L'utilisateur utilise déjà Graphify dans RiftMind (fichiers `.graphifyignore` et dossier `graphify-out`, configuré lors d'un commit « Add Claude Code setup and graphify configuration »). Reproduis cette configuration dans le nouveau dépôt, après avoir consulté la documentation ou l'aide de l'outil. Ignore `target/`, `graphify-out/` et les données. Dis à l'utilisateur ce que tu as fait et ce que Graphify produit.

### 5.5 Rechercher et proposer des skills

1. Regarde d'abord ce qui existe dans `RiftMind/.claude` (il contient des skills de garde et de replays) et ce qui est déjà installé.
2. Cherche des skills pertinents pour : Rust (revue, `clippy`, `unsafe`), API Windows et ETW, profilage et benchmarks (`criterion`), tests par scénarios, workflow OpenSpec, Conventional Commits, revue de sécurité, rédaction de README, et selon la décision d'interface, Tauri/Svelte. L'utilisateur admire des skills qui réduisent le code superflu (par exemple Ponytail) : un skill de ce type est un bon candidat.
3. **Audite chaque candidat avant de l'installer** : lis le `SKILL.md` en entier et tous les scripts, vérifie la licence, l'auteur et l'historique, et refuse tout ce qui exécute du code réseau inattendu ou touche aux secrets.
4. Présente 5 à 10 candidats à l'utilisateur (nom, source, utilité, risque), puis installe dans `.claude/skills/` du dépôt **ceux qu'il valide**.

### 5.6 Documenter

Crée `docs/decisions.md` (réponses aux questions et choix) et `docs/compliance.md` (section 4).

### 5.7 Résumé final

Termine par un résumé court : ce qui est fait, ce qui reste, les trois prochaines décisions de l'utilisateur.

---

## 6. L'assembleur : situation et pistes

**État des lieux honnête :** le projet utilise très peu d'assembleur (environ 2 à 5 %, des instructions comme `rdtscp` et `cpuid` pour l'horloge matérielle et la topologie CPU). Sa difficulté réelle est ailleurs (événements noyau, API natives de Windows, code sans allocation, décision en temps réel). L'utilisateur aimerait **approfondir cet aspect** sans enfreindre les conditions des jeux.

**Règle d'or :** tout code assembleur reste dans notre propre processus, ne touche jamais au jeu et ne contourne rien. Chaque module assembleur doit **battre sa version Rust de référence** dans un benchmark `criterion`, sinon il est abandonné. Chaque module a un repli en Rust sûr, activé par détection de fonctionnalités à l'exécution (`cfg(target_arch)`), et des tests de parité entre les deux versions.

Pistes à évaluer et à chiffrer (ne rien décider seul) :

1. **Horloge calibrée** : `rdtscp` calibré contre `QueryPerformanceCounter`, avec détection d'un TSC invariant. Petit, utile, bon point de départ.
2. **Noyaux SIMD (AVX2) pour l'analyse** : histogrammes et percentiles des frametimes, détection de ruptures et corrélation croisée avec décalage entre la série des frametimes et des milliers de séries d'événements système, sur fenêtre glissante. C'est du calcul réel, mesurable face à Rust auto-vectorisé.
3. **Générateur de causes synthétiques** (banc d'essai) : boucles d'occupation précises au cycle, saturation du cache, des TLB et de la bande passante mémoire, pour provoquer des saccades **contrôlées** sur la machine de test. Cela fournit une vérité terrain pour valider le diagnostic et les tests A/B. Légitime et très adapté à l'assembleur.
4. **Topologie CPU** : `cpuid` (feuilles de topologie et type de cœur sur les processeurs hybrides) pour guider les CPU Sets. Attention : des API Windows le font déjà ; l'intérêt est la validation et l'apprentissage, à documenter honnêtement.
5. **Déroulement de piles x86-64** : les événements de pile d'ETW donnent des adresses brutes ; les attribuer à des fonctions de pilotes demande de parser les tables d'exceptions `.pdata` et les codes de déroulement (ABI x64). C'est de la compréhension profonde de l'architecture, sans écrire d'assembleur, à envisager pour la phase de diagnostic avancé.
6. **Structures sans verrou** : buffer circulaire SPSC avec ordre mémoire explicite et préchargement, éventuellement des écritures non temporelles, comparé à l'implémentation Rust.

Choix de forme à poser à l'utilisateur : assembleur en ligne (`core::arch::asm!`, stable, simple) ou fichiers externes MASM/NASM assemblés par `build.rs` (plus formateur, plus lourd). VibeDiff a déjà un `build.rs`.

**À exclure explicitement :** pilote noyau, lecture des MSR par un pilote tiers (les anti-triche bloquent les pilotes vulnérables), tout ce qui interagit avec le processus du jeu.

---

## 7. Questions ouvertes (à poser à l'utilisateur)

**Périmètre et produit**
1. Faut-il un seul exécutable en tray avec fenêtre de tableau de bord, ou un service d'arrière-plan plus une interface séparée ?
2. Quelle interface : Tauri + Svelte (comme NanoDrop et RiftMind), `egui`, ou natif ? Contrainte de l'utilisateur : démarrage instantané et faible mémoire.
3. Quelles sont les 3 corrections prioritaires de la première version du pilote d'actions ? Lesquelles sont réversibles à 100 % ?
4. Quel niveau d'autonomie par défaut : proposer, demander confirmation, ou appliquer seul avec journal ?
5. Que doit afficher l'overlay en jeu, si on en prévoit un ? Est-il compatible avec chaque jeu et chaque mode d'affichage (plein écran exclusif) ?

**Technique**
6. Capture des frames : événements ETW de présentation faits maison ou réutilisation de la bibliothèque de PresentMon ?
7. Modèle de privilèges : administrateur à chaque lancement, tâche planifiée élevée, ou service Windows ?
8. Matériel visé : NVIDIA seulement (NVML) ou AMD et Intel aussi ? Portables et fixes ?
9. Méthode statistique du test A/B : nombre minimal de sessions, test choisi, traitement des sessions bruitées ?
10. Comment tester sans jeu ? (fixtures et scénarios synthétiques à la RiftMind, plus le générateur de la section 6.3.)

**Données et vie privée**
11. Rétention par défaut (30 jours, 2 Go) ? Export des rapports ? Aucune télémétrie envoyée sans consentement explicite ?
12. Base de connaissances communautaire anonyme (hybride) : oui, non, plus tard ?
13. Rôle d'un petit modèle local pour expliquer les diagnostics en français : indispensable, optionnel ou hors périmètre ?

**Jeux et conformité**
14. Autres jeux joués et modés par l'utilisateur ? Priorité entre les quatre jeux cités ?
15. Minecraft : client seulement, ou aussi le serveur de ses amis (avec l'accord de l'administrateur) ? Quel lanceur utilise-t-il (CurseForge, officiel, autre) ?
16. Quels documents de conformité l'utilisateur a-t-il déjà lus pour RiftMind ? (pour ne pas refaire le travail)

**Diffusion**
17. Licence (MIT ?), dépôt public dès le départ, nom définitif du projet ?
18. Objectif de visibilité : démonstration (captures, vidéo, rapport A/B) pour son portfolio ?

**Assembleur**
19. Quelles pistes de la section 6 l'attirent le plus ? Assembleur en ligne ou fichiers externes ?
20. Quel budget d'apprentissage accepte-t-il (heures par semaine) avant que l'assembleur ne devienne un frein à l'avancement ?

---

## 8. Conventions de travail

- **Spec d'abord** : une *change* OpenSpec par fonctionnalité, validée par l'utilisateur avant le code.
- **Tests** : fixtures et scénarios (modèle : 83 scénarios de RiftMind), test d'absence d'allocation sur le chemin chaud, tests de parité pour tout module assembleur.
- **Commits** : Conventional Commits en anglais, petits et fréquents.
- **Documentation** : README de production (comme VibeDiff), schémas Mermaid si utile.
- **Sécurité** : aucun secret dans le dépôt, `.env.example` uniquement ; aucun téléchargement ni exécution de code tiers non audité.
- **`unsafe` et assembleur** : l'utilisateur apprend vite mais n'a pas encore écrit de Rust `unsafe` ni d'assembleur. Les erreurs de ce type ne se voient pas à la compilation, donc :
  - isole tout le code `unsafe` dans un module ou une crate dédiée (par exemple `sys`) et expose une interface sûre ; mets `#![forbid(unsafe_code)]` dans toutes les autres crates ;
  - active `#![deny(unsafe_op_in_unsafe_fn)]` et le lint Clippy `undocumented_unsafe_blocks` ;
  - chaque bloc `unsafe` porte un commentaire `// SAFETY:` qui énonce l'invariant qui le rend correct ;
  - avant de fusionner un bloc `unsafe` ou assembleur, explique-le en français à l'utilisateur (ce qu'il fait, pourquoi il est correct, ce qui casserait) et attends qu'il dise l'avoir compris ;
  - teste le code pur avec Miri quand c'est possible (Miri ne couvre pas les appels Windows), et par des tests de parité pour l'assembleur.
- **Fichiers volumineux** : ne commite jamais `target/`, enregistrements ou traces.
- **Communication** : explique ce que tu fais et pourquoi, brièvement. L'utilisateur relit tout ; pas de jargon inutile, mais il veut comprendre le bas niveau. Quand tu touches à `unsafe` ou à l'assembleur, explique le raisonnement.
- **Honnêteté** : dis quand une piste est risquée, quand un gain est incertain, et quand tu n'as pas pu vérifier une information.

## 9. Livrables attendus de cette première session

- `docs/compliance.md` avec sources et verdicts.
- Dépôt initialisé, OpenSpec et Graphify configurés, trois *changes* en proposition.
- Liste de skills candidats auditée et skills validés installés.
- `docs/decisions.md` avec les réponses de l'utilisateur.
- Résumé final (section 5.7).

## 10. Hors périmètre

Pilote noyau, interaction avec les processus des jeux, automatisation des entrées, toute fonction donnant un avantage compétitif en direct, collecte de données personnelles, dépendance à des services payants.
