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
| Large pages (WIN-09) rejetées : pages de 4 Kio partout, privilège « Verrouiller les pages en mémoire » jamais demandé. | Décision de Matteo (2026-11-22). Les activer demande un privilège accordé par stratégie et `OpenProcessToken`, interdit hors `open_target` par le test de source ; gain nul pour le produit (seule la courbe de cache des sondes en profitait) et le repli 4 Kio fonctionne. |
| Parité PresentMon du lot 5 mesurée sur Opera et Chrome (page WebGL ou vidéo 60 images/s), pas sur un jeu. | Décision de Matteo (2026-11-22). Chrome est le navigateur ouvert pendant ses parties ; la boucle D3D11 du Lab n'arrive qu'au lot 6. |
| Spike ETW lot 5 (build 26300) : les adresses noyau arrivent non nulles à une session administrateur sans `SeDebugPrivilege`, et une session privée « system logger » suffit (pas de NT Kernel Logger). | Mesuré le 2026-11-22 sur 15 s : ISR (opcode 67) 4 265 événements, DPC (68) 78 522 et TimerDPC (69) 1 771, tous avec une `Routine` en adresse noyau (0 nulle) ; 632 561 mots de piles noyau, 0 nul ; bases d'images du rundown : 270 images noyau sur 20 977 au démarrage et 270 sur 21 897 à l'arrêt ; horodatages bruts cohérents avec le TSC (19 864 885 991 356 ticks à 2,688 GHz = 7 390 s, proche de l'uptime). `StartTraceW`, `TraceSetInformation` (piles) et `ProcessTrace` : code 0. Repli de design D-1 (adresses nulles) inutile : il reste écrit si une future version de Windows les masque. **Non vérifié** : que chaque adresse tombe dans une image connue (tâche de la table des modules), que les piles contiennent aussi des trames utilisateur (aucune vue : 0 « autre »), ce que sont les 31 001 événements PerfInfo d'opcode 50. Correction du dump TDH : les opcodes sont 67 ISR, 68 DPC, 69 TimerDPC (les étiquettes « DPC 66 » et « ThreadedDPC 69 » du spike étaient fausses ; 66 n'a jamais été vu). |

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
| 2026-11-22 | 3 | Réveil du timer haute résolution (`driverless-probes`) | p99 > 500 µs (brouillon du lot 4) | p99 > 5 ms (lot 4) | Machine au calme, 2000 attentes de 1 ms : p50 environ 520 µs, p99 2 à 3,3 ms (résolution du minuteur). 500 µs aurait déclenché à chaque run. Runs `fa check` : p99 1,29 ms (secteur), 2,08 ms (batterie), 4,62 ms (essai avec un build en arrière-plan : la marge de 5 ms est faible face à un PC chargé). |
| 2026-11-22 | 3 | Charge de `fa check` et sonde de fréquence (`driverless-probes`) | un thread de charge sur chaque autre CPU logique | un thread sur un CPU logique de chaque autre cœur physique, jamais le jumeau SMT du CPU mesuré | Avec le jumeau chargé la sonde lit 3,71 GHz contre 4,08 sur PDH (−9 %) ; jumeau libre : 4,089 contre 4,082 (0,17 %). Sans cette règle, une machine saine paraîtrait bridée. |
| 2026-11-22 | 4 | Seuil de bridage (`health-check`) | > 10 % sous les 5 premières s | inchangé (10 %) | Deux runs : secteur 4,09 → 3,79 GHz au pire (−7 %), pas de verdict ; batterie 2,49 GHz plat. Marge de 3 points côté secteur. |
| 2026-11-22 | 4 | Verdict de plafonnement (`health-check`) | absent | fréquence médiane tenue sous charge > 15 % sous le pic mono-cœur | Décision de Matteo. Sur batterie la machine tient 2,49 GHz plat (pic 3,39, −26 %) : la règle de bridage, qui compare aux 5 premières secondes, ne voit rien. Secteur : 4,09 contre 4,49 (−9 %). Calé sur deux runs d'une seule machine. |
| 2026-11-22 | 4 | Seuil RAM mono-canal (`health-check`) | bande passante < 60 % du théorique | nombre de modules WMI d'abord ; 60 % de la lecture tous cœurs en confirmation | 2 × 16 Gio DDR4-3200 (51,2 Go/s en double canal). Lecture mesurée 37,7 à 44,9 Go/s (74 à 88 %), écriture 34 à 35 Go/s (66 à 68 %), copie 36 à 38 Go/s : la bande passante seule ne tranche pas. Sans WMI : pas de verdict. |
| 2026-11-22 | 4 | Ordre des phases de `fa check` | charge puis mémoire et réveil (brouillon) | pic mono-cœur, mémoire, réveil, puis charge de 60 s | Les phases à froid d'abord : nos propres threads de charge ne peuvent pas fausser le p99 de réveil ni la bande passante. Durées mesurées : 3,6 + 2,1 à 3,3 + 3,1 à 3,4 + 60,5 s, total 69,4 à 71,1 s pour 90 s autorisées. Balayage de cache limité à 64 Mio, 2 runs par point. |
| 2026-11-22 | 4 | Rapport de `fa check` | lignes de cache affichées, décimales à point | lignes de cache seulement dans le JSON, décimales à virgule | Sur batterie le balayage grossier signalait trois « écarts » (L1 24 contre 48 Kio, L2 768 contre 1280, L3 4096 contre 12288) sur une machine saine. |
| 2026-11-22 | 4 | Comparaison HWiNFO (`health-check`) | prévue (D10 du lot 3) | non faite | HWiNFO se ferme au lancement du mode capteurs sur la machine cible (pilote noyau probablement refusé, cause non investiguée). La précision de la sonde repose sur PDH seul (0,15 à 0,2 % d'écart, jumeau SMT libre). |
| 2026-11-22 | 5 | Parité des frametimes avec PresentMon 2.6.0 (`etw-capture`) | au moins 99 % des frames à ±0,1 ms | inchangé (99 %), atteint sur Opera, 98,8 % sur Chrome | Opera : 1 037 frames sur 1 037 concordantes, écart médian 0,2 µs, max 0,004 ms (vidéo 60 images/s, modes vus : Hardware Composed Independent Flip 935, Composed Flip 102). Chrome : 738 frames sur 747 (98,8 %), écart médian 0,3 µs ; les 9 écarts forment 3 épisodes où les lignes de PresentMon sont dans le désordre (valeurs 12,0 / 0,0 / 12,2 ms contre 6,05 / 5,96 / 6,29 ms chez nous) ; ses propres temps de début de présentation donnent nos 6,0 ms. Une exécution par navigateur, mode de présentation seulement lu dans le CSV de PresentMon (non calculé par nous). À rejouer sur la boucle du Lab (lot 6). |
| 2026-11-23 | 6 | Budget de gigue de la boucle du Lab (`stutter-lab`) | `(calibrate)`, non chiffré | gigue p99 ≤ 1,6 ms à 144 FPS (cible 6,944 ms) ; un stall injecté doit dépasser nettement ce budget | Décision de Matteo (« mesuré puis figé »). 3 runs de 60 s à 144 FPS, release, machine au repos, adaptateur Intel UHD (défaut Optimus) : gigue p99 1,597 / 1,555 / 1,539 ms, médiane 6,945 ms, max de frame 15,4 à 16,1 ms (deux périodes). Un run antérieur plus chargé a donné 2,16 ms : le budget n'est valable que machine au repos. À 60 FPS : médiane 16,665 ms, gigue p99 1,72 ms (un run). Écran à 144 Hz (vsync : 6,944 ms). `uncapped` exige le tearing (sinon plafonné au rafraîchissement) : médiane 3,3 ms. |
| 2026-11-23 | 6 | Parité des frametimes avec PresentMon sur la boucle du Lab (`stutter-lab`) | critère du lot 5 : au moins 99 % des frames à ±0,1 ms | inchangé (99 %), atteint : 100,0 % | Boucle du Lab à 144 FPS avec 3 `frame-stall` (50, 100, 30 ms), PresentMon 2.6.0 sur `fa.exe` : 3288 frames chez nous, 3287 chez PresentMon, 3287 concordantes (décalage 0), écart médian 0,0003 ms, p99 0,005 ms, max 0,018 ms. Vérité terrain : la capture voit chaque stall à la bonne frame, écart de +0,22 à +0,78 ms sur 2 runs (budget 1,6 ms). Perte 0 sur les files chaude et froide. Le Lab présente sur l'Intel UHD (adaptateur par défaut d'Optimus). |
| 2026-11-23 | 6 | Charge par frame de la boucle du Lab (`stutter-lab`) | scène triviale (un `ClearView`) | par frame : parcours d'un anneau de 2 Mio (pointer chase) puis environ 1 ms de calcul à travail fixe | Avec la scène triviale, aucun injecteur sauf `frame-stall` ne touchait les frametimes (p99 pendant ≈ hors injection). Décision de Matteo : ajouter une charge. Un anneau de 3 Mio coûtait environ 5 ms par frame (surtout de la RAM : capacité effective 3 à 4 Mio) ; 2 Mio, au-dessus des 1,25 Mio de L2, coûte environ 0,5 ms depuis le cache partagé. Gigue de base avec la charge : p99 1,56 à 2,45 ms sur 11 mesures de 3 s (0,3 ms sans charge, 1,6 ms sur 60 s) : le budget de 1,6 ms ci-dessus reste celui de la précision capture contre injection, pas celui de la boucle. |
| 2026-11-23 | 6 | Effet de chaque injecteur sur les frametimes du Lab à 144 FPS (`stutter-lab`) | à déterminer | `frame-stall` : cause exacte par construction. `mem-bandwidth` (5 threads, stores non temporels) : médiane 20 ms contre 6,94 ms. `cache-thrash`, `tlb-thrash` (sur le jumeau SMT du rendu), `page-fault-storm`, `cpu-burn` (avec ou sans FMA, hors du cœur de rendu) : aucun effet visible (médiane à 0,04 ms près) | Mesuré par `fa lab run` sur ses propres frames, machine au repos, un run chacun (deux pour `tlb-thrash`). Effets mesurés autrement : `cache-thrash` +41 % sur la latence d'une victime de 2 Mio (ASM-07, 6 runs), `page-fault-storm` 16,4 millions de défauts de page en 10 s dans le processus du Lab contre 19 pour `cpu-burn`. Un TLB appartient à un cœur et un brûleur hors du cœur de rendu ne le gêne pas : `tlb-thrash` et `cpu-burn` n'ont d'effet que sur ce cœur (priority-contention) ou par contention de ressources que cette scène ne sollicite pas. |
| 2026-11-24 | 6 | Effet de `timer-resolution`, `priority-contention` et `disk-burst` sur les frametimes du Lab (`stutter-lab`) | à déterminer | `timer-resolution` (le Lab lâche `timeBeginPeriod(1)` et dort sur un timer ordinaire) : médiane 5,1 ms, p99 19,4 à 20,1 ms contre 6,94 ms, 1327 frames en 10 s au lieu de 1440. `priority-contention` : seule, elle ne fait rien (le thread de rendu migre vers un cœur libre en quelques microsecondes) ; avec le rendu épinglé sur le cœur du travailleur à priorité time-critical : 19 à 22 frames en 10 s, max 2,6 s. `disk-burst` (512 Mio, sans cache, write-through) : médiane inchangée, max 15,5 ms contre 7,3 ms | Mesures de Claude, release, `fa lab run`, un run chacun (priority-contention : deux). La contention épingle le thread de rendu du Lab (son propre thread) pour la durée de l'injection. Un seul run de `timer-resolution` a montré une frame de 848 ms hors injection, non reproduite en deux runs |
