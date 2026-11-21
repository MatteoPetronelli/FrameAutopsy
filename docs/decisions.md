# FrameAutopsy : décisions

> Journal des décisions. Source : session de brainstorming du 2026-11-20 (Matteo + Claude). Chaque lot ajoute ici ses
> décisions et ses mesures de calibration.

## 1. Décisions prises par Matteo

| # | Décision | Conséquence |
|---|---|---|
| D1 | **L'asm et l'`unsafe` se méritent.** La règle d'or du handoff est conservée et étendue à l'`unsafe` : un module bat sa référence (Rust sûr, intrinsics, crate existante, compteur Windows) sur une mesure déclarée, a une version de secours choisie au lancement selon le CPU et des tests de parité, sinon il est retiré. | Pas de crate « labo » d'apprentissage pur. Registre : `docs/asm-unsafe-catalog.md`. |
| D2 | Familles de features retenues : sondes sans pilote, Stutter Lab, analyse SIMD, attribution aux pilotes, en plus de la base du handoff (capture ETW, actions annulables, Minecraft JVM). | 17 lots (`roadmap.md`). |
| D3 | Premier résultat visible : le **bilan de santé** `fa check`. | Lots 0 à 4 sans admin, sans ETW, hors jeu. |
| D4 | Asm écrit avec `core::arch::asm!` (syntaxe Intel) ; `global_asm!` avec directives `.seh_*` seulement si une fonction entière l'exige. | Pas de MASM, de NASM ni de `build.rs` pour l'asm. |
| D5 | Collecteur sans fenêtre lancé en admin par une tâche planifiée, interface sans droits ; mémoire partagée en lecture seule pour l'UI, commandes par named pipe en liste blanche. | Frontière de confiance à modéliser (lots 7 et 12). |
| D6 | Valorant est une cible à part entière : observation système et actions sur les autres processus, aucune donnée de partie. | Colonne `valorant` dans la matrice. |
| D7 | Interface en egui, ouverte hors jeu. | Démarrage instantané, faible mémoire. |
| D8 | Overlay au dernier lot, sous condition : métriques système seulement, activé jeu par jeu après un test anti-triche réussi. | Lot 16. |
| D9 | Minecraft : client seulement (CurseForge). | Serveurs hors périmètre. |
| D10 | Une change OpenSpec par lot. | Specs écrites dans `specifications.md`, copiées au moment de créer chaque change. |
| D11 | Trois crates `unsafe`, découpées selon leur vérification : `fa-asm` (parité + criterion), `fa-win` (FFI, tests d'intégration), `fa-sync` (Miri). `fa-pe` est abandonnée : `goblin` et `pdb` font le travail sans `unsafe`. | `#![forbid(unsafe_code)]` partout ailleurs. |
| D12 | L'A/B se fait par **blocs de 10 min appariés pendant que tu joues** : la règle bascule aux frontières de blocs, y compris en pleine partie, dans un ordre tiré au hasard. | Verdict après environ 8 paires (≈ 2 h 40 de jeu) ; mesures proches dans le temps. |

## 2. Choix faits par Claude (à relire, tu peux les changer)

| Choix | Raison |
|---|---|
| Autonomie par règle et par jeu : `proposer` par défaut ; `auto` seulement après un A/B réussi sur ta machine, retiré si une expérience échoue ensuite. | Ton profil : l'autonomie se mérite par la preuve. |
| Les sondes servent de contrôle pour l'A/B : une session est exclue si la fréquence, l'état thermique, la source d'alimentation ou la limite de puissance du GPU diffèrent. | Sinon un portable chaud fausse la preuve. |
| A/B : la paire de blocs est l'unité ; test exact par inversion de signe ; deux points de décision (8 et 12 paires, α = 0,025 chacun) ; IC par bootstrap à deux niveaux ; désactivation après 12 paires sans preuve. | Avec 5 paires, le plus petit p bilatéral possible est 0,0625 : rien ne pourrait être prouvé. Avec 8 paires, il est d'environ 0,008. Répartir α sur les deux regards garde le taux de faux positifs sous 5 %. |
| Installation : binaires dans `%ProgramFiles%\FrameAutopsy`, données du collecteur dans `%ProgramData%\FrameAutopsy` modifiables par l'admin seulement ; fichiers modifiables par l'utilisateur validés comme des entrées non fiables. | Sinon n'importe quel programme pourrait remplacer le collecteur et obtenir les droits admin au démarrage. |
| L'UI peut tourner pendant le jeu (vue en direct, overlay) mais rien ne l'exige ; le mode `confirmer` met les propositions en file, relue après la partie ; aucune fenêtre ne prend le focus en jeu. | Ne jamais gêner une partie. |
| Symboles : le collecteur stocke module + décalage ; téléchargement et lecture des PDB seulement dans `fa-ui`/`fa`, sans droits admin. | Ne jamais lire un fichier venu d'Internet avec les droits admin. |
| Lab : `fa lab campaign` (terminal admin) enregistre des fixtures versionnées ; la gate les note hors ligne avec `fa lab score`. | La gate reste rapide, sans admin ni GPU. |
| Le watchdog détecte la mort du collecteur par son mutex abandonné. | Pas besoin d'ouvrir un handle de processus. |
| Actions v1 : EcoQoS + priorité basse, CPU Sets, overlay d'alimentation. | Les trois sont annulables à 100 % ; les autres viennent ensuite, avec une preuve chacune. |
| Observateurs de base actifs sur tous les jeux (sauf `forbidden`), confirmés par un test anti-triche au lot 7 ; le reste exige `allowed`. | Le handoff dit à la fois « observation seule sur tout jeu » et « à vérifier avant usage » ; ce compromis les réconcilie (`docs/compliance.md` §4). |
| Le Stutter Lab refuse de tourner si un jeu de la matrice est lancé. | Ne jamais gêner une vraie partie. |
| Attribution : toujours module + décalage ; nom de fonction seulement avec des symboles publics (Microsoft, NVIDIA), téléchargés sur accord et gardés en cache. | Seul accès réseau du produit, désactivé par défaut. |
| Capture ETW faite main (algorithme de PresentMon comme référence et parité). | Zéro allocation et horodatage TSC : `ferrisetw` sert de point de comparaison. |
| Texte de l'interface en français, identifiants en anglais. | Tu es le premier utilisateur. |
| Pas d'icône dans la zone de notification en v1. | Le collecteur n'a pas de fenêtre et l'UI ne tourne pas pendant le jeu ; à revoir si besoin. |
| GPU : NVML pour NVIDIA d'abord ; les autres fabricants via les événements ETW DxgKrnl, communs à tous. | Ta machine est NVIDIA ; les sondes CPU couvrent Intel et AMD. |
| Rétention de 30 jours et 2 Go, aucune télémétrie, export à la demande. | Valeurs du handoff conservées. |
| Seuils chiffrés (déclencheur, verdicts, budgets) marqués « (calibrate) ». | Points de départ, recalibrés par le Lab et tes sessions. |
| Horloge (lot 2) : maths pures dans `fa-core::clock`, lectures TSC dans `fa-asm::tsc`, QPC et type `Clock` dans `fa-win::clock` (qui dépend de `fa-asm` et `fa-core`). | `fa-core` ne peut pas dépendre de `fa-win` (cycle via `TargetProcess`) ; pas de crate `fa-clock` pour quelques centaines de lignes. |
| Épinglage du thread courant (WIN-07) créé au lot 2 au lieu du lot 3. | Décision de Matteo (2026-11-21) : base commune pour les tests d'horloge et les sondes. |
| Checkpoint d'explication : base théorique, pas un checkpoint par groupe `unsafe` ; un seul pour l'asm au lot 2. | Décision de Matteo (2026-11-21). |

## 3. Corrections apportées pendant le brainstorming

- **La sonde de fréquence doit battre un compteur Windows.** Le compteur PDH `% Processor Performance` donne déjà la
  fréquence effective, sans pilote. La sonde n'est gardée que si elle apporte une information en plus (mesure de 50 µs
  par cœur à l'instant d'une saccade) ; décision prise sur mesure au lot 3.
- **Lecture de `hsperfdata`** : la lecture simple du fichier est la référence ; le mapping partagé ne reste que s'il
  gagne (probablement pas).
- **Arène d'allocation abandonnée** : des `Vec` dimensionnés à l'avance et réutilisés suffisent.
- **Risque Windows 11 24H2** pour l'attribution aux pilotes : les adresses noyau sont moins accessibles. Un essai court
  ouvre le lot 5 (pas le lot 11, car le moteur de causes du lot 10 en dépend déjà), et `SeDebugPrivilege` reste exclu.

### Corrections issues de la relecture indépendante (2026-11-20)

- **Ta machine** : i5-11400H (Tiger Lake-H, 6 cœurs identiques, AVX-512 présent, ni WAITPKG à confirmer ni MWAITX),
  2 × 16 Go DDR4-3200, RTX 3060 Laptop, Windows 11 26H2. Conséquence : les attentes `tpause`/`mwaitx` (ASM-05) sont
  retirées ; les variantes AVX-512 deviennent testables ; les cas hybride et double CCD sont testés sur des données
  synthétiques.
- **Dépendances** : l'échantillonnage des sondes en jeu passe du lot 3 au lot 8 ; la décision sur la sonde de fréquence
  passe au lot 10 ; la table des modules et l'essai 24H2 passent au lot 5 ; le lot 15 dépend du lot 10 et le lot 16 du
  lot 9.
- **Faits Windows** : l'horodatage TSC brut exige aussi `PROCESS_TRACE_MODE_RAW_TIMESTAMP` ; les événements noyau
  classiques tronquent le nom des processus à 15 octets (on utilise Microsoft-Windows-Kernel-Process, et le garde-fou
  refuse un nom tronqué) ; depuis Windows 10 2004 la résolution du timer ne vaut que pour le processus qui la demande,
  donc l'action correspondante est retirée.
- **Unsafe** : l'allocateur de test passe dans `fa-sync` (il exige `unsafe impl`) ; le seqlock utilise des atomiques
  relâchés et des barrières, jamais `volatile` ; la « combustion de cycles exacts » en asm est remplacée par une boucle
  jusqu'à une échéance TSC en Rust sûr et par une charge FMA (ASM-06) qui doit battre les intrinsics.
- **Une seule règle pour `ferrisetw`** : le consommateur maison reste seulement s'il fait 0 allocation par événement et
  au moins le même débit.
- **Réseau** : `fa-attrib` est la seule crate qui joint Internet ; le client LoL est limité à `127.0.0.1`.
- **Valeur p** : avec 3 + 3 sessions non appariées, le plus petit p est 0,05 en unilatéral et 0,10 en bilatéral
  (l'ancienne phrase oubliait cette distinction).

## 4. Questions du handoff (§7) : état

| # | Question | État |
|---|---|---|
| 1 | Tray unique ou service + UI | Tranché : D5 (collecteur élevé + UI) ; pas de tray en v1 (choix de Claude). |
| 2 | Interface | Tranché : D7 (egui). |
| 3 | 3 corrections prioritaires | Choix de Claude : A1 à A3. |
| 4 | Niveau d'autonomie | Choix de Claude : proposer → auto gagné par A/B. |
| 5 | Overlay | Tranché : D8. |
| 6 | ETW maison ou PresentMon | Choix de Claude : maison, PresentMon en référence et parité. |
| 7 | Modèle de privilèges | Tranché : D5 (tâche planifiée élevée). |
| 8 | Matériel visé | Choix de Claude : CPU Intel et AMD, NVML d'abord, portables et fixes. |
| 9 | Méthode A/B | Choix de Claude (spec `ab-experiment`). |
| 10 | Tester sans jeu | Tranché : Stutter Lab (D2). |
| 11 | Rétention, export, télémétrie | Choix de Claude : valeurs du handoff. |
| 12 | Base communautaire | **Ouvert**, hors de cette feuille de route. |
| 13 | Petit modèle local pour expliquer | **Ouvert**, hors de cette feuille de route. |
| 14 | Autres jeux, priorité entre les jeux | Valorant ajouté (D6). **Priorité entre les jeux : ouvert.** |
| 15 | Minecraft : périmètre, lanceur | Tranché : D9, CurseForge. |
| 16 | Documents de conformité déjà lus pour RiftMind | **Ouvert** (utile au lot 1). |
| 17 | Licence, dépôt public, nom définitif | **Ouvert.** |
| 18 | Objectif de visibilité | Vidéo de démo du Lab prévue au lot 6 ; le reste est **ouvert**. |
| 19 | Pistes asm, forme | Tranché : D2, D4. |
| 20 | Budget d'apprentissage asm | **Ouvert.** |

## 5. Mesures et calibrations

| Date | Lot | Valeur | Avant | Après | Mesure |
|---|---|---|---|---|---|
| 2026-11-21 | 2 | Écart TSC/QPC sur 10 s (`tsc-clock`) | < 10 µs | < 100 µs | Fréquence CPUID `0x15` = 2 688 000 000 Hz (nominale). 3 runs sur secteur, i5-11400H : 45,8 / 45,8 / 45,0 µs (4,58 / 4,58 / 4,50 ppm), écart systématique : le TSC réel tourne environ 4,6 ppm plus vite que la valeur nominale. Une calibration QPC courte (5 × 200 ms) est trop bruitée pour corriger (1,7 ppm trouvés). Décision de Matteo (option A) : garder CPUID comme source et relâcher le seuil ; 4,6 ppm est négligeable pour des frametimes de quelques ms. |
