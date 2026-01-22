import { computed, Injectable, signal } from '@angular/core';

type LangKey = 'en' | 'fr' | 'de' | 'it';

type Example = {
  title: string;
  code: string;
};

type BlogPost = {
  slug: string;
  title: string;
  date: string;
  text: string;
  body: string[];
  takeaways: string[];
};

type I18n = {
  nav: {
    home: string;
    downloads: string;
    docs: string;
    community: string;
    showcase: string;
    blog: string;
    security: string;
    status: string;
  };
  hero: {
    eyebrow: string;
    title: string;
    lead: string;
    primary: string;
    ghost: string;
    pills: string[];
    steps: string[];
    cardTitle: string;
    cardTag: string;
  };
  download: {
    eyebrow: string;
    title: string;
    lead: string;
    macTitle: string;
    macDesc: string;
    macButton: string;
    macHint: string;
    otherTitle: string;
    otherDesc: string;
    windowsButton: string;
    otherHint: string;
    quickInstall: string;
  };
  about: {
    eyebrow: string;
    title: string;
    cards: { title: string; text: string }[];
  };
  showcase: {
    eyebrow: string;
    title: string;
    lead: string;
    cards: { title: string; text: string }[];
  };
  showcasePage: {
    eyebrow: string;
    title: string;
    lead: string;
    cards: { title: string; text: string }[];
    examplesTitle: string;
  };
  blog: {
    eyebrow: string;
    title: string;
    lead: string;
    posts: BlogPost[];
  };
  security: {
    eyebrow: string;
    title: string;
    lead: string;
    cards: { title: string; text: string }[];
    policyTitle: string;
    policyText: string;
    timelineTitle: string;
    timeline: string[];
  };
  status: {
    eyebrow: string;
    title: string;
    lead: string;
    metrics: { label: string; value: string }[];
    services: { name: string; state: string; text: string }[];
    historyTitle: string;
    history: { title: string; date: string; text: string }[];
  };
  examples: {
    eyebrow: string;
    title: string;
    lead: string;
    checkTitle: string;
    runTitle: string;
    minimalTitle: string;
    editorTitle: string;
    browseAll: string;
  };
  docs: {
    eyebrow: string;
    title: string;
    lead: string;
    sections: { title: string; text: string }[];
    commandsTitle: string;
    commandList: string[];
    guidesTitle: string;
    guides: { title: string; text: string }[];
    troubleshootingTitle: string;
    troubleshooting: string[];
    bestPracticesTitle: string;
    bestPractices: string[];
  };
  community: {
    eyebrow: string;
    title: string;
    lead: string;
    cards: { title: string; text: string }[];
    linksTitle: string;
    links: string[];
  };
  faq: {
    eyebrow: string;
    title: string;
    items: { q: string; a: string }[];
  };
  changelog: {
    eyebrow: string;
    title: string;
    lead: string;
    items: { version: string; date: string; notes: string[] }[];
  };
  footer: string;
  labels: {
    language: string;
  };
};

const DOWNLOAD_MAC_URL =
  'https://github.com/vitte-lang/steel.org/releases/download/Steel-0.1.0-1.2026/steel-0.1.0-MacOS_universal_test.pkg';
const DOWNLOAD_WINDOWS_URL =
  'https://github.com/vitte-lang/steel.org/releases/download/Steel-0.1.0-1.2026/Steel-0.1.0-WindowsX86_64-windows.exe';

const I18N: Record<LangKey, I18n> = {
  en: {
    nav: {
      home: 'Home',
      downloads: 'Downloads',
      docs: 'Docs',
      community: 'Community',
      showcase: 'Showcase',
      blog: 'Blog',
      security: 'Security',
      status: 'Status'
    },
    hero: {
      eyebrow: 'Private Edition',
      title: 'Steel, delivered direct. No setup drama.',
      lead: 'Minimal text. Maximum examples. Download, run, copy, ship.',
      primary: 'Download Steel',
      ghost: 'See examples',
      pills: ['Beginner friendly', 'Clean installs', 'Multi-OS'],
      steps: ['Download the installer', 'Run one command', 'Paste a ready example'],
      cardTitle: 'Quick start',
      cardTag: 'v0.1.0_test'
    },
    download: {
      eyebrow: 'Download',
      title: 'One button. One file.',
      lead: 'Direct installer link with no extra steps.',
      macTitle: 'macOS (0.1.0_test)',
      macDesc: 'Direct macOS installer. Open and install.',
      macButton: 'Download Steel for macOS',
      macHint: 'File type: .pkg',
      otherTitle: 'Windows + Linux',
      otherDesc: 'In progress.',
      windowsButton: 'Download Steel for Windows',
      otherHint: 'Windows: .exe — Linux: .deb',
      quickInstall: 'Quick install'
    },
    about: {
      eyebrow: 'Why Steel',
      title: 'A build guide, not a mystery.',
      cards: [
        {
          title: 'Clear inputs',
          text: 'A single file describes what to build and where outputs go.'
        },
        {
          title: 'Stable output',
          text: 'Steel creates a predictable plan you can reuse and automate.'
        },
        {
          title: 'Beginner-first',
          text: 'Short examples you can copy without learning everything first.'
        }
      ]
    },
    showcase: {
      eyebrow: 'Use cases',
      title: 'Where Steel fits best.',
      lead: 'Small teams, mixed stacks, or one language done well.',
      cards: [
        { title: 'Solo projects', text: 'Start with one file and grow as needed.' },
        { title: 'Multi-language apps', text: 'Keep C, Rust, Swift, and Java together.' },
        { title: 'CI pipelines', text: 'Deterministic outputs and clear inputs.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Real-world steelconf patterns.',
      lead: 'Pick a pattern and adapt it to your stack.',
      cards: [
        { title: 'CLI app', text: 'One target, fast output, zero surprises.' },
        { title: 'App + lib', text: 'Split build steps with clean outputs.' },
        { title: 'Polyglot repo', text: 'One config per language, one build story.' }
      ],
      examplesTitle: 'Example blocks'
    },
    blog: {
      eyebrow: 'Blog',
      title: 'Notes from the Steel team.',
      lead: 'Short updates, focused on releases and patterns.',
      posts: [
        {
          slug: 'steel-command-stays-minimal',
          title: 'Why Steel - Command stays minimal',
          date: '2026-01-12',
          text: 'Complexity is a tax on every build. Steel - Command keeps a single source of truth, obvious outputs, and small, readable recipes.',
          body: [
            'Build tools drift toward complexity. Steel - Command does not. We keep it minimal because clarity scales better than cleverness.',
            'Rule one: one file should tell the story. A steelconf must be readable in under a minute or it is too big.',
            'Rule two: outputs must be obvious. Every artifact lands in target/out, so you never hunt for results.',
            'Rule three: examples before features. If a feature cannot be shown in ten lines, it is not ready.'
          ],
          takeaways: [
            'One file is the source of truth.',
            'Outputs go to target/out by default.',
            'Features ship only with small examples.'
          ]
        },
        {
          slug: 'multi-language-pattern-that-scales',
          title: 'The multi-language pattern that scales',
          date: '2026-01-16',
          text: 'Keep each language isolated with its own tool and bake block. Shared rule: output clarity in target/out.',
          body: [
            'Multi-language builds fail when everything blends together. Steel - Command keeps each language in its own block.',
            'Define one tool and one bake per language. Keep outputs predictable. The result is a build you can explain to anyone on the team.',
            'Start small: one language, one output. Add more only when the existing setup is stable.'
          ],
          takeaways: [
            'Isolate each language in its own block.',
            'One tool + one bake per language.',
            'Add languages only after the base is stable.'
          ]
        },
        {
          slug: 'release-notes-that-matter',
          title: 'Release notes that matter',
          date: '2026-01-20',
          text: 'We ship direct installers first and document the path to a first build. Each release has a single focus.',
          body: [
            'Releases should have one story. For Steel - Command, that story is always: faster onboarding and clearer builds.',
            'That is why we ship a direct macOS installer before anything else. The first build matters more than options.',
            'Every release gets a minimal example and a short explanation of what improved.'
          ],
          takeaways: [
            'One release, one clear story.',
            'Direct installers first.',
            'Every release ships with a minimal example.'
          ]
        }
      ]
    },
    security: {
      eyebrow: 'Security',
      title: 'Security by default.',
      lead: 'Safe defaults, explicit tools, and clear outputs.',
      cards: [
        { title: 'Explicit tools', text: 'Only tools you declare are executed.' },
        { title: 'Deterministic inputs', text: 'Known inputs keep builds predictable.' },
        { title: 'Auditable config', text: 'Steel logs a normalized config file.' },
        { title: 'Minimal surface', text: 'One file drives the build.' }
      ],
      policyTitle: 'Security policy',
      policyText: 'Report issues with a minimal reproduction and your OS/toolchain details.',
      timelineTitle: 'Disclosure timeline',
      timeline: ['Day 0: report received', 'Day 3: initial triage', 'Day 7: fix shipped']
    },
    status: {
      eyebrow: 'Status',
      title: 'Current service status.',
      lead: 'Latest build pipelines and download availability.',
      metrics: [
        { label: 'Uptime', value: '99.9%' },
        { label: 'Build latency', value: '< 2 min' },
        { label: 'Download latency', value: '< 1 min' }
      ],
      services: [
        { name: 'macOS Downloads', state: 'Operational', text: 'Direct .pkg download available.' },
        { name: 'Windows Downloads', state: 'In progress', text: 'Packaging and signing underway.' },
        { name: 'Linux Downloads', state: 'In progress', text: 'Deb packaging pipeline in progress.' }
      ],
      historyTitle: 'Recent updates',
      history: [
        { title: 'macOS installer published', date: '2026-01-20', text: 'Universal .pkg ready for download.' },
        { title: 'Windows pipeline queued', date: '2026-01-18', text: 'Signing pipeline in staging.' }
      ]
    },
    examples: {
      eyebrow: 'Examples',
      title: 'Copy, paste, run.',
      lead: 'Short snippets you can drop into a project.',
      checkTitle: 'Check install',
      runTitle: 'Run',
      minimalTitle: 'Minimal steelconf',
      editorTitle: 'Vim-style view',
      browseAll: 'Browse all examples'
    },
    docs: {
      eyebrow: 'Docs',
      title: 'Just enough to move fast.',
      lead: 'Short guides for beginners with real commands.',
      sections: [
        { title: '1) Create a file', text: 'Make a steelconf at project root.' },
        { title: '2) Define tools', text: 'Declare the compiler you want to use.' },
        { title: '3) Add a recipe', text: 'Tell Steel how to build and where to output.' }
      ],
      commandsTitle: 'Essential commands',
      commandList: ['steel --version', 'steel run', 'steel fmt', 'steel doctor'],
      guidesTitle: 'Guides',
      guides: [
        { title: 'Pick a language', text: 'Start from the closest example block.' },
        { title: 'Keep outputs simple', text: 'Use target/out/ for quick feedback.' },
        { title: 'Add formatting later', text: 'Keep the first build minimal.' }
      ],
      troubleshootingTitle: 'Troubleshooting',
      troubleshooting: [
        'If a tool is missing, check your PATH.',
        'If a file is not found, verify your glob patterns.',
        'If builds are slow, reduce the number of inputs.'
      ],
      bestPracticesTitle: 'Best practices',
      bestPractices: [
        'Keep output paths stable.',
        'Name tools after real binaries.',
        'Start with one target, then expand.'
      ]
    },
    community: {
      eyebrow: 'Community',
      title: 'Get help and share examples.',
      lead: 'Short links, no noise.',
      cards: [
        { title: 'Report issues', text: 'Open a GitHub issue with your log and config.' },
        { title: 'Share recipes', text: 'Post a minimal steelconf snippet.' },
        { title: 'Ask questions', text: 'Keep it short, include your OS and toolchain.' }
      ],
      linksTitle: 'Tips for good questions',
      links: ['Include your OS', 'Include your tool version', 'Share a minimal config']
    },
    faq: {
      eyebrow: 'FAQ',
      title: 'Short answers.',
      items: [
        { q: 'Is this for beginners?', a: 'Yes. The examples are short and copyable.' },
        { q: 'Windows + Linux?', a: 'In progress. Shipping .exe and .deb.' },
        { q: 'Where is the main file?', a: 'The main file is named steelconf.' },
        { q: 'Quick test?', a: 'Download, then run steel --version.' },
        { q: 'Need help?', a: 'Start with the minimal example, then adapt.' }
      ]
    },
    changelog: {
      eyebrow: 'Changelog',
      title: 'Release notes',
      lead: 'What is new and what to expect next.',
      items: [
        {
          version: '0.1.0_test',
          date: '2026-01-20',
          notes: ['Direct macOS installer', 'Beginner-first examples', 'Multi-language landing']
        },
        {
          version: '0.1.0_next',
          date: 'Coming soon',
          notes: ['Windows .exe', 'Linux .deb', 'More language templates']
        }
      ]
    },
    footer: 'Steel Private Edition. Direct downloads only.',
    labels: {
      language: 'Language'
    }
  },
  fr: {
    nav: {
      home: 'Accueil',
      downloads: 'Telechargements',
      docs: 'Docs',
      community: 'Communaute',
      showcase: 'Showcase',
      blog: 'Blog',
      security: 'Securite',
      status: 'Statut'
    },
    hero: {
      eyebrow: 'Edition privee',
      title: 'Steel, telecharge direct. Sans prise de tete.',
      lead: 'Moins de texte. Plus d\'exemples. Telechargez, lancez, copiez.',
      primary: 'Telecharger Steel',
      ghost: 'Voir les exemples',
      pills: ['Debutant', 'Install propre', 'Multi-OS'],
      steps: ['Telecharger l\'installeur', 'Lancer une commande', 'Coller un exemple'],
      cardTitle: 'Demarrage',
      cardTag: 'v0.1.0_test'
    },
    download: {
      eyebrow: 'Telecharger',
      title: 'Un bouton. Un fichier.',
      lead: 'Lien direct, pas d\'etapes en plus.',
      macTitle: 'macOS (0.1.0_test)',
      macDesc: 'Installeur macOS direct. Ouvrir et installer.',
      macButton: 'Telecharger Steel pour macOS',
      macHint: 'Type: .pkg',
      otherTitle: 'Windows + Linux',
      otherDesc: 'En cours.',
      windowsButton: 'Telecharger Steel pour Windows',
      otherHint: 'Windows: .exe — Linux: .deb',
      quickInstall: 'Installation rapide'
    },
    about: {
      eyebrow: 'Pourquoi Steel',
      title: 'Un guide de build clair, pas un mystere.',
      cards: [
        {
          title: 'Entrees claires',
          text: 'Un seul fichier decrit quoi construire et ou sortir.'
        },
        {
          title: 'Sortie stable',
          text: 'Steel cree un plan previsible pour l\'automatisation.'
        },
        {
          title: 'Debutant d\'abord',
          text: 'Des exemples courts, copiables tout de suite.'
        }
      ]
    },
    showcase: {
      eyebrow: 'Cas d\'usage',
      title: 'La ou Steel est le plus utile.',
      lead: 'Petites equipes, stacks mixtes, ou un seul langage.',
      cards: [
        { title: 'Projets solo', text: 'Un seul fichier pour demarrer vite.' },
        { title: 'Apps multi-langage', text: 'C, Rust, Swift et Java ensemble.' },
        { title: 'Pipelines CI', text: 'Sorties deterministes et entrees claires.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Patrons steelconf en production.',
      lead: 'Choisir un pattern et l\'adapter a votre stack.',
      cards: [
        { title: 'CLI', text: 'Un target, sortie rapide, sans surprise.' },
        { title: 'App + lib', text: 'Etapes claires et sorties propres.' },
        { title: 'Depot polyglotte', text: 'Une config par langage, un build.' }
      ],
      examplesTitle: 'Blocs d\'exemple'
    },
    blog: {
      eyebrow: 'Blog',
      title: 'Notes de l\'equipe Steel.',
      lead: 'Updates courts sur releases et patterns.',
      posts: [
        {
          slug: 'steel-command-reste-minimal',
          title: 'Pourquoi Steel - Command reste minimal',
          date: '2026-01-12',
          text: 'La complexite coute cher. Un seul fichier, des sorties claires, et des recettes courtes.',
          body: [
            'Les outils de build tendent vers la complexite. Steel - Command fait l\'inverse pour rester lisible.',
            'Regle un: un seul fichier doit raconter l\'histoire. Si ce n\'est pas lisible en une minute, c\'est trop gros.',
            'Regle deux: des sorties evidentes. Chaque artefact va dans target/out.',
            'Regle trois: exemples avant features. Si on ne peut pas le montrer en dix lignes, ce n\'est pas pret.'
          ],
          takeaways: [
            'Un seul fichier pour tout comprendre.',
            'Sorties dans target/out par defaut.',
            'Pas de feature sans exemple court.'
          ]
        },
        {
          slug: 'pattern-multi-langage',
          title: 'Le pattern multi-langage qui tient',
          date: '2026-01-16',
          text: 'Un tool + un bake par langage. Regle commune: sorties claires dans target/out.',
          body: [
            'Les builds multi-langage echouent quand tout est melange. Steel - Command garde chaque langage isole.',
            'Un tool, un bake, un output clair. Le build reste simple a expliquer.',
            'Commencer petit: une langue, un output. Ajouter ensuite.'
          ],
          takeaways: [
            'Un bloc par langage.',
            'Un tool + un bake par langage.',
            'Ajouter apres stabilite.'
          ]
        },
        {
          slug: 'notes-de-release',
          title: 'Des releases qui comptent',
          date: '2026-01-20',
          text: 'Installer direct d\'abord, puis le chemin vers le premier build. Un focus par release.',
          body: [
            'Une release doit raconter une seule histoire. Pour Steel - Command: demarrer vite et comprendre le build.',
            'C\'est pour cela qu\'on livre d\'abord un installeur macOS direct.',
            'Chaque release ajoute un exemple minimal et une explication courte.'
          ],
          takeaways: [
            'Un seul objectif par release.',
            'Installeur direct en premier.',
            'Exemple minimal a chaque release.'
          ]
        }
      ]
    },
    security: {
      eyebrow: 'Securite',
      title: 'Securite par defaut.',
      lead: 'Outils explicites et sorties claires.',
      cards: [
        { title: 'Outils explicites', text: 'Seuls les outils declares sont executes.' },
        { title: 'Entrees deterministes', text: 'Inputs connus pour builds stables.' },
        { title: 'Config auditable', text: 'Steel genere un fichier normalise.' },
        { title: 'Surface minimale', text: 'Un seul fichier pilote le build.' }
      ],
      policyTitle: 'Politique securite',
      policyText: 'Signalez un probleme avec un cas minimal et vos details OS/toolchain.',
      timelineTitle: 'Timeline',
      timeline: ['Jour 0: reception', 'Jour 3: triage', 'Jour 7: correctif']
    },
    status: {
      eyebrow: 'Statut',
      title: 'Etat des services.',
      lead: 'Telechargements et pipelines en temps reel.',
      metrics: [
        { label: 'Disponibilite', value: '99.9%' },
        { label: 'Latence build', value: '< 2 min' },
        { label: 'Latence download', value: '< 1 min' }
      ],
      services: [
        { name: 'Telechargement macOS', state: 'Operationnel', text: 'Installeur .pkg disponible.' },
        { name: 'Telechargement Windows', state: 'En cours', text: 'Packaging et signature.' },
        { name: 'Telechargement Linux', state: 'En cours', text: 'Pipeline .deb en preparation.' }
      ],
      historyTitle: 'Historique recent',
      history: [
        { title: 'Installeur macOS publie', date: '2026-01-20', text: 'Universal .pkg en ligne.' },
        { title: 'Pipeline Windows en cours', date: '2026-01-18', text: 'Signature en staging.' }
      ]
    },
    examples: {
      eyebrow: 'Exemples',
      title: 'Copier, coller, lancer.',
      lead: 'Petits extraits a poser dans votre projet.',
      checkTitle: 'Verifier',
      runTitle: 'Lancer',
      minimalTitle: 'steelconf minimal',
      editorTitle: 'Vue type vim',
      browseAll: 'Voir tous les exemples'
    },
    docs: {
      eyebrow: 'Docs',
      title: 'Le strict minimum pour avancer vite.',
      lead: 'Guides courts avec commandes utiles.',
      sections: [
        { title: '1) Creer un fichier', text: 'Mettre un steelconf a la racine.' },
        { title: '2) Definir les outils', text: 'Declarer le compilateur a utiliser.' },
        { title: '3) Ajouter une recette', text: 'Decrire le build et la sortie.' }
      ],
      commandsTitle: 'Commandes utiles',
      commandList: ['steel --version', 'steel run', 'steel fmt', 'steel doctor'],
      guidesTitle: 'Guides',
      guides: [
        { title: 'Choisir un langage', text: 'Partir d\'un exemple proche.' },
        { title: 'Sorties simples', text: 'Utiliser target/out/ pour tester vite.' },
        { title: 'Formatage plus tard', text: 'Garder le premier build minimal.' }
      ],
      troubleshootingTitle: 'Depannage',
      troubleshooting: [
        'Si un outil manque, verifier le PATH.',
        'Si un fichier manque, verifier les globs.',
        'Si c\'est lent, reduire les inputs.'
      ],
      bestPracticesTitle: 'Bonnes pratiques',
      bestPractices: [
        'Garder des sorties stables.',
        'Nommer les tools comme les binaires.',
        'Commencer avec une seule target.'
      ]
    },
    community: {
      eyebrow: 'Communaute',
      title: 'Aide rapide, exemples partages.',
      lead: 'Liens courts, pas de bruit.',
      cards: [
        { title: 'Signaler un probleme', text: 'Ouvrir un issue avec le log.' },
        { title: 'Partager une recette', text: 'Poster un steelconf minimal.' },
        { title: 'Poser une question', text: 'OS, toolchain, et reproduction.' }
      ],
      linksTitle: 'Conseils utiles',
      links: ['Indiquer votre OS', 'Donner la version des outils', 'Partager un exemple minimal']
    },
    faq: {
      eyebrow: 'FAQ',
      title: 'Reponses rapides.',
      items: [
        { q: 'C\'est pour debutants ?', a: 'Oui. Exemples courts et copiables.' },
        { q: 'Windows + Linux ?', a: 'En cours. .exe et .deb.' },
        { q: 'Le fichier principal ?', a: 'Le fichier s\'appelle steelconf.' },
        { q: 'Test rapide ?', a: 'Telecharger puis steel --version.' },
        { q: 'Besoin d\'aide ?', a: 'Commencez par l\'exemple minimal.' }
      ]
    },
    changelog: {
      eyebrow: 'Changelog',
      title: 'Notes de version',
      lead: 'Nouveautes et prochaines etapes.',
      items: [
        {
          version: '0.1.0_test',
          date: '2026-01-20',
          notes: ['Installeur macOS direct', 'Exemples debutants', 'Page multi-langue']
        },
        {
          version: '0.1.0_next',
          date: 'Bientot',
          notes: ['Windows .exe', 'Linux .deb', 'Plus de templates']
        }
      ]
    },
    footer: 'Steel Edition privee. Telechargement direct.',
    labels: {
      language: 'Langue'
    }
  },
  de: {
    nav: {
      home: 'Start',
      downloads: 'Downloads',
      docs: 'Dokumente',
      community: 'Community',
      showcase: 'Showcase',
      blog: 'Blog',
      security: 'Sicherheit',
      status: 'Status'
    },
    hero: {
      eyebrow: 'Private Edition',
      title: 'Steel, direkt geliefert. Kein Setup-Stress.',
      lead: 'Wenig Text. Viele Beispiele. Download, starten, kopieren.',
      primary: 'Steel herunterladen',
      ghost: 'Beispiele ansehen',
      pills: ['Einsteigerfreundlich', 'Saubere Installation', 'Multi-OS'],
      steps: ['Installer herunterladen', 'Ein Befehl', 'Beispiel einfugen'],
      cardTitle: 'Schnellstart',
      cardTag: 'v0.1.0_test'
    },
    download: {
      eyebrow: 'Download',
      title: 'Ein Knopf. Eine Datei.',
      lead: 'Direkter Installer-Link ohne Extra-Schritte.',
      macTitle: 'macOS (0.1.0_test)',
      macDesc: 'Direkter macOS-Installer. Offnen und installieren.',
      macButton: 'Steel fur macOS herunterladen',
      macHint: 'Dateityp: .pkg',
      otherTitle: 'Windows + Linux',
      otherDesc: 'In Arbeit.',
      windowsButton: 'Steel fur Windows herunterladen',
      otherHint: 'Windows: .exe — Linux: .deb',
      quickInstall: 'Schnellinstallation'
    },
    about: {
      eyebrow: 'Warum Steel',
      title: 'Builds klar statt kompliziert.',
      cards: [
        {
          title: 'Klare Eingaben',
          text: 'Eine Datei beschreibt Build und Ausgabeorte.'
        },
        {
          title: 'Stabiles Ergebnis',
          text: 'Steel erstellt einen planbaren Ablauf.'
        },
        {
          title: 'Einsteiger zuerst',
          text: 'Kurze Beispiele zum direkten Kopieren.'
        }
      ]
    },
    showcase: {
      eyebrow: 'Einsatzfalle',
      title: 'Wo Steel am besten passt.',
      lead: 'Kleine Teams, gemischte Stacks, oder ein Fokus.',
      cards: [
        { title: 'Solo-Projekte', text: 'Ein File reicht fur den Start.' },
        { title: 'Multi-Language', text: 'C, Rust, Swift und Java zusammen.' },
        { title: 'CI-Pipelines', text: 'Deterministische Outputs und klare Inputs.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Steelconf Muster aus der Praxis.',
      lead: 'Wahle ein Muster und passe es an.',
      cards: [
        { title: 'CLI-App', text: 'Ein Target, schneller Output, keine Uberraschungen.' },
        { title: 'App + Lib', text: 'Klare Schritte und Outputs.' },
        { title: 'Polyglot Repo', text: 'Config pro Sprache, ein Build-Story.' }
      ],
      examplesTitle: 'Beispielbloecke'
    },
    blog: {
      eyebrow: 'Blog',
      title: 'Notizen vom Steel-Team.',
      lead: 'Kurze Updates zu Releases und Patterns.',
      posts: [
        {
          slug: 'steel-command-bleibt-minimal',
          title: 'Warum Steel - Command minimal bleibt',
          date: '2026-01-12',
          text: 'Komplexitat kostet. Eine Datei, klare Outputs und kleine Rezepte halten Builds lesbar.',
          body: [
            'Build-Tools werden oft zu komplex. Steel - Command bleibt bewusst minimal.',
            'Regel eins: eine Datei erzahlt die ganze Geschichte. Wenn es nicht in einer Minute klar ist, ist es zu viel.',
            'Regel zwei: klare Outputs. Jeder Build landet in target/out.',
            'Regel drei: Beispiele vor Features. Kein Feature ohne zehn Zeilen Beispiel.'
          ],
          takeaways: [
            'Eine Datei erklart alles.',
            'Outputs in target/out.',
            'Features brauchen kurze Beispiele.'
          ]
        },
        {
          slug: 'multi-language-muster',
          title: 'Das Multi-Language Muster',
          date: '2026-01-16',
          text: 'Ein Tool und ein Bake pro Sprache. Gemeinsame Regel: klare Outputs in target/out.',
          body: [
            'Multi-Language Builds scheitern, wenn alles vermischt wird. Steel - Command trennt sauber.',
            'Ein Tool, ein Bake, ein Output. So bleibt der Build erklarbar.',
            'Starte klein und erweitere Schritt fur Schritt.'
          ],
          takeaways: [
            'Ein Block pro Sprache.',
            'Ein Tool + ein Bake pro Sprache.',
            'Erst stabil, dann erweitern.'
          ]
        },
        {
          slug: 'release-notizen',
          title: 'Releases mit Fokus',
          date: '2026-01-20',
          text: 'Direkte Installer zuerst, dann Beispiele. Jede Release hat ein klares Ziel.',
          body: [
            'Jede Release erzahlt nur eine Geschichte: schneller starten, klarer bauen.',
            'Darum liefern wir zuerst den macOS Installer.',
            'Jede Release kommt mit minimalem Beispiel und kurzer Erklarung.'
          ],
          takeaways: [
            'Eine Release, ein Ziel.',
            'Direkter Installer zuerst.',
            'Beispiel in jeder Release.'
          ]
        }
      ]
    },
    security: {
      eyebrow: 'Sicherheit',
      title: 'Sicherheit by default.',
      lead: 'Explizite Tools und klare Outputs.',
      cards: [
        { title: 'Explizite Tools', text: 'Nur deklarierte Tools werden ausgefuhrt.' },
        { title: 'Deterministische Inputs', text: 'Bekannte Inputs fur stabile Builds.' },
        { title: 'Auditierbare Config', text: 'Steel schreibt eine normalisierte Datei.' },
        { title: 'Minimale Flache', text: 'Eine Datei steuert den Build.' }
      ],
      policyTitle: 'Sicherheitsrichtlinie',
      policyText: 'Melden Sie Probleme mit minimalem Repro und OS/Toolchain.',
      timelineTitle: 'Timeline',
      timeline: ['Tag 0: Eingang', 'Tag 3: Triage', 'Tag 7: Fix']
    },
    status: {
      eyebrow: 'Status',
      title: 'Aktueller Status.',
      lead: 'Downloads und Pipelines im Blick.',
      metrics: [
        { label: 'Uptime', value: '99.9%' },
        { label: 'Build-Latenz', value: '< 2 min' },
        { label: 'Download-Latenz', value: '< 1 min' }
      ],
      services: [
        { name: 'macOS Downloads', state: 'Operational', text: 'Direkter .pkg Download verfugbar.' },
        { name: 'Windows Downloads', state: 'In Arbeit', text: 'Packaging und Signing laufen.' },
        { name: 'Linux Downloads', state: 'In Arbeit', text: 'Deb Pipeline in Arbeit.' }
      ],
      historyTitle: 'Letzte Updates',
      history: [
        { title: 'macOS Installer veroffentlicht', date: '2026-01-20', text: 'Universal .pkg online.' },
        { title: 'Windows Pipeline gestartet', date: '2026-01-18', text: 'Signing in Vorbereitung.' }
      ]
    },
    examples: {
      eyebrow: 'Beispiele',
      title: 'Kopieren, einfugen, starten.',
      lead: 'Kurze Snippets fur dein Projekt.',
      checkTitle: 'Installation prufen',
      runTitle: 'Starten',
      minimalTitle: 'Minimales steelconf',
      editorTitle: 'Vim-Ansicht',
      browseAll: 'Alle Beispiele ansehen'
    },
    docs: {
      eyebrow: 'Dokumente',
      title: 'Nur das Notige, um schnell zu starten.',
      lead: 'Kurze Anleitungen mit echten Befehlen.',
      sections: [
        { title: '1) Datei erstellen', text: 'steelconf im Projekt-Root anlegen.' },
        { title: '2) Tools definieren', text: 'Compiler deklarieren.' },
        { title: '3) Rezept hinzufugen', text: 'Build und Output beschreiben.' }
      ],
      commandsTitle: 'Wichtige Befehle',
      commandList: ['steel --version', 'steel run', 'steel fmt', 'steel doctor'],
      guidesTitle: 'Guides',
      guides: [
        { title: 'Sprache wahlen', text: 'Starte mit dem passenden Beispiel.' },
        { title: 'Outputs simpel halten', text: 'target/out/ fur schnelles Feedback.' },
        { title: 'Formatierung spater', text: 'Erst bauen, dann polieren.' }
      ],
      troubleshootingTitle: 'Troubleshooting',
      troubleshooting: [
        'Wenn ein Tool fehlt, PATH prufen.',
        'Wenn Dateien fehlen, Globs checken.',
        'Wenn es langsam ist, Inputs reduzieren.'
      ],
      bestPracticesTitle: 'Best Practices',
      bestPractices: [
        'Output Pfade stabil halten.',
        'Tools wie echte Binaries benennen.',
        'Mit einem Target beginnen.'
      ]
    },
    community: {
      eyebrow: 'Community',
      title: 'Hilfe finden und Beispiele teilen.',
      lead: 'Kurze Links, kein Larm.',
      cards: [
        { title: 'Problem melden', text: 'Issue mit Log und Config eroffnen.' },
        { title: 'Rezepte teilen', text: 'Minimalen steelconf posten.' },
        { title: 'Fragen stellen', text: 'OS und Toolchain angeben.' }
      ],
      linksTitle: 'Gute Fragen',
      links: ['OS angeben', 'Tool-Version nennen', 'Minimalbeispiel posten']
    },
    faq: {
      eyebrow: 'FAQ',
      title: 'Kurze Antworten.',
      items: [
        { q: 'Fur Einsteiger geeignet?', a: 'Ja. Kurze, kopierbare Beispiele.' },
        { q: 'Windows + Linux?', a: 'In Arbeit. .exe und .deb.' },
        { q: 'Hauptdatei?', a: 'Die Datei heisst steelconf.' },
        { q: 'Schneller Test?', a: 'Download, dann steel --version.' },
        { q: 'Hilfe?', a: 'Mit dem Minimalbeispiel starten.' }
      ]
    },
    changelog: {
      eyebrow: 'Changelog',
      title: 'Release-Notizen',
      lead: 'Neues und was als nachstes kommt.',
      items: [
        {
          version: '0.1.0_test',
          date: '2026-01-20',
          notes: ['Direkter macOS-Installer', 'Einsteigerbeispiele', 'Mehrsprachige Seite']
        },
        {
          version: '0.1.0_next',
          date: 'Bald',
          notes: ['Windows .exe', 'Linux .deb', 'Mehr Templates']
        }
      ]
    },
    footer: 'Steel Private Edition. Nur Direkt-Downloads.',
    labels: {
      language: 'Sprache'
    }
  },
  it: {
    nav: {
      home: 'Home',
      downloads: 'Download',
      docs: 'Documenti',
      community: 'Community',
      showcase: 'Showcase',
      blog: 'Blog',
      security: 'Sicurezza',
      status: 'Stato'
    },
    hero: {
      eyebrow: 'Edizione privata',
      title: 'Steel, consegnato diretto. Niente complicazioni.',
      lead: 'Poco testo. Tanti esempi. Scarica, avvia, copia.',
      primary: 'Scarica Steel',
      ghost: 'Vedi esempi',
      pills: ['Per principianti', 'Installazione pulita', 'Multi-OS'],
      steps: ['Scarica installer', 'Un comando', 'Incolla un esempio'],
      cardTitle: 'Avvio rapido',
      cardTag: 'v0.1.0_test'
    },
    download: {
      eyebrow: 'Download',
      title: 'Un pulsante. Un file.',
      lead: 'Link diretto senza passaggi extra.',
      macTitle: 'macOS (0.1.0_test)',
      macDesc: 'Installer macOS diretto. Apri e installa.',
      macButton: 'Scarica Steel per macOS',
      macHint: 'Tipo file: .pkg',
      otherTitle: 'Windows + Linux',
      otherDesc: 'In corso.',
      windowsButton: 'Scarica Steel per Windows',
      otherHint: 'Windows: .exe — Linux: .deb',
      quickInstall: 'Installazione rapida'
    },
    about: {
      eyebrow: 'Perche Steel',
      title: 'Build chiari, senza misteri.',
      cards: [
        {
          title: 'Input chiari',
          text: 'Un file descrive cosa costruire e dove.'
        },
        {
          title: 'Output stabile',
          text: 'Steel crea un piano prevedibile.'
        },
        {
          title: 'Prima i principianti',
          text: 'Esempi brevi, subito copiabili.'
        }
      ]
    },
    showcase: {
      eyebrow: 'Casi d\'uso',
      title: 'Dove Steel rende di piu.',
      lead: 'Team piccoli, stack misti, o un solo linguaggio.',
      cards: [
        { title: 'Progetti singoli', text: 'Un file basta per partire.' },
        { title: 'Multi-linguaggio', text: 'C, Rust, Swift e Java insieme.' },
        { title: 'Pipeline CI', text: 'Output deterministici e input chiari.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Pattern steelconf reali.',
      lead: 'Scegli un pattern e adattalo al tuo stack.',
      cards: [
        { title: 'CLI', text: 'Un target, output rapido, zero sorprese.' },
        { title: 'App + lib', text: 'Passi chiari e output puliti.' },
        { title: 'Repo poliglott', text: 'Una config per linguaggio, un build.' }
      ],
      examplesTitle: 'Blocchi esempio'
    },
    blog: {
      eyebrow: 'Blog',
      title: 'Note dal team Steel.',
      lead: 'Aggiornamenti brevi su release e pattern.',
      posts: [
        {
          slug: 'steel-command-resta-minimale',
          title: 'Perche Steel - Command resta minimale',
          date: '2026-01-12',
          text: 'La complessita costa. Un solo file, output chiari, ricette brevi.',
          body: [
            'Gli strumenti di build tendono alla complessita. Steel - Command resta essenziale.',
            'Regola uno: un file racconta tutto. Se non e leggibile in un minuto, e troppo.',
            'Regola due: output chiari. Tutto finisce in target/out.',
            'Regola tre: esempi prima delle feature. Nessuna feature senza dieci righe.'
          ],
          takeaways: [
            'Un file per capire tutto.',
            'Output in target/out.',
            'Esempi brevi per ogni feature.'
          ]
        },
        {
          slug: 'pattern-multi-lingua',
          title: 'Il pattern multi-lingua',
          date: '2026-01-16',
          text: 'Un tool e un bake per linguaggio. Output chiari in target/out.',
          body: [
            'I build multi-lingua falliscono quando tutto si mescola. Steel - Command separa.',
            'Un tool, un bake, un output. Il build resta spiegabile.',
            'Inizia piccolo e aggiungi gradualmente.'
          ],
          takeaways: [
            'Un blocco per linguaggio.',
            'Un tool + un bake per lingua.',
            'Aggiungi solo dopo stabilita.'
          ]
        },
        {
          slug: 'note-di-release',
          title: 'Release con un obiettivo',
          date: '2026-01-20',
          text: 'Installer diretto prima, poi esempi. Ogni release ha un focus.',
          body: [
            'Ogni release racconta una sola storia: partire in fretta e capire il build.',
            'Per questo consegniamo prima l\'installer macOS.',
            'Ogni release aggiunge un esempio minimo e una spiegazione corta.'
          ],
          takeaways: [
            'Una release, un obiettivo.',
            'Installer diretto prima.',
            'Esempio minimo per ogni release.'
          ]
        }
      ]
    },
    security: {
      eyebrow: 'Sicurezza',
      title: 'Sicurezza di default.',
      lead: 'Tool espliciti e output chiari.',
      cards: [
        { title: 'Tool espliciti', text: 'Solo i tool dichiarati vengono eseguiti.' },
        { title: 'Input deterministici', text: 'Input noti per build stabili.' },
        { title: 'Config auditabile', text: 'Steel genera un file normalizzato.' },
        { title: 'Superficie minima', text: 'Un solo file guida il build.' }
      ],
      policyTitle: 'Policy sicurezza',
      policyText: 'Segnala problemi con un repro minimo e dettagli OS/toolchain.',
      timelineTitle: 'Timeline',
      timeline: ['Giorno 0: ricezione', 'Giorno 3: triage', 'Giorno 7: fix']
    },
    status: {
      eyebrow: 'Stato',
      title: 'Stato attuale dei servizi.',
      lead: 'Download e pipeline piu recenti.',
      metrics: [
        { label: 'Uptime', value: '99.9%' },
        { label: 'Latenza build', value: '< 2 min' },
        { label: 'Latenza download', value: '< 1 min' }
      ],
      services: [
        { name: 'Download macOS', state: 'Operativo', text: 'Installer .pkg disponibile.' },
        { name: 'Download Windows', state: 'In corso', text: 'Packaging e firma in corso.' },
        { name: 'Download Linux', state: 'In corso', text: 'Pipeline .deb in preparazione.' }
      ],
      historyTitle: 'Aggiornamenti recenti',
      history: [
        { title: 'Installer macOS pubblicato', date: '2026-01-20', text: 'Universal .pkg pronto.' },
        { title: 'Pipeline Windows avviata', date: '2026-01-18', text: 'Firma in staging.' }
      ]
    },
    examples: {
      eyebrow: 'Esempi',
      title: 'Copia, incolla, esegui.',
      lead: 'Snippet brevi per il tuo progetto.',
      checkTitle: 'Verifica',
      runTitle: 'Esegui',
      minimalTitle: 'steelconf minimale',
      editorTitle: 'Vista vim',
      browseAll: 'Vedi tutti gli esempi'
    },
    docs: {
      eyebrow: 'Documenti',
      title: 'Solo il necessario per partire.',
      lead: 'Guide brevi con comandi reali.',
      sections: [
        { title: '1) Crea un file', text: 'Crea steelconf alla radice.' },
        { title: '2) Definisci gli strumenti', text: 'Dichiara il compilatore.' },
        { title: '3) Aggiungi una ricetta', text: 'Descrivi build e output.' }
      ],
      commandsTitle: 'Comandi essenziali',
      commandList: ['steel --version', 'steel run', 'steel fmt', 'steel doctor'],
      guidesTitle: 'Guide',
      guides: [
        { title: 'Scegli un linguaggio', text: 'Parti da un esempio vicino.' },
        { title: 'Output semplici', text: 'Usa target/out/ per test rapidi.' },
        { title: 'Formatting dopo', text: 'Prima costruisci, poi rifinisci.' }
      ],
      troubleshootingTitle: 'Risoluzione problemi',
      troubleshooting: [
        'Se manca un tool, controlla il PATH.',
        'Se mancano file, controlla i glob.',
        'Se e lento, riduci gli input.'
      ],
      bestPracticesTitle: 'Best practices',
      bestPractices: [
        'Percorsi output stabili.',
        'Tool con nomi reali.',
        'Inizia con un solo target.'
      ]
    },
    community: {
      eyebrow: 'Community',
      title: 'Aiuto rapido, esempi condivisi.',
      lead: 'Link brevi, niente rumore.',
      cards: [
        { title: 'Segnala un problema', text: 'Apri una issue con log e config.' },
        { title: 'Condividi ricette', text: 'Pubblica uno steelconf minimale.' },
        { title: 'Fai domande', text: 'Indica OS e toolchain.' }
      ],
      linksTitle: 'Consigli utili',
      links: ['Indica il tuo OS', 'Scrivi la versione tool', 'Condividi config minima']
    },
    faq: {
      eyebrow: 'FAQ',
      title: 'Risposte rapide.',
      items: [
        { q: 'Per principianti?', a: 'Si. Esempi brevi e copiabili.' },
        { q: 'Windows + Linux?', a: 'In corso. .exe e .deb.' },
        { q: 'File principale?', a: 'Il file si chiama steelconf.' },
        { q: 'Test rapido?', a: 'Scarica, poi steel --version.' },
        { q: 'Serve aiuto?', a: 'Parti dall\'esempio minimale.' }
      ]
    },
    changelog: {
      eyebrow: 'Changelog',
      title: 'Note di rilascio',
      lead: 'Novita e prossimi passi.',
      items: [
        {
          version: '0.1.0_test',
          date: '2026-01-20',
          notes: ['Installer macOS diretto', 'Esempi per principianti', 'Pagina multilingua']
        },
        {
          version: '0.1.0_next',
          date: 'Presto',
          notes: ['Windows .exe', 'Linux .deb', 'Piu template']
        }
      ]
    },
    footer: 'Steel Edizione privata. Solo download diretti.',
    labels: {
      language: 'Lingua'
    }
  }
};

const CORE_EXAMPLES: Example[] = [
  {
    title: 'Swift',
    code: `[tool swiftc]\n\t.exec "swiftc"\n..\n\n[bake build_debug]\n\t.make swift_src cglob "Sources/**/*.swift"\n\t[run swiftc]\n\t\t.set "-g" 1\n\t\t.takes swift_src as "@args"\n\t\t.emits exe as "-o"\n\t..\n\t.output exe "target/out/swift_app_debug"\n..`
  },
  {
    title: 'Rust',
    code: `[tool sh]\n\t.exec "sh"\n..\n\n[bake rust_build]\n\t.make rust_src cglob "src/**/*.rs"\n\t[run sh]\n\t\t.set "-c" "cargo build"\n\t..\n\t.output exe "target/debug/steel"\n..`
  },
  {
    title: 'Python',
    code: `[tool sh]\n\t.exec "sh"\n..\n\n[bake python_run]\n\t.make py_src cglob "src/**/*.py"\n\t[run sh]\n\t\t.set "-c" "python3 -u src/main.py"\n\t..\n\t.output exe "target/out/python.run"\n..`
  },
  {
    title: 'Java',
    code: `[tool javac]\n\t.exec "javac"\n..\n\n[bake java_build]\n\t.make java_src cglob "src/**/*.java"\n\t[run javac]\n\t\t.set "-d" "target/classes"\n\t\t.takes java_src as "@args"\n\t..\n\t.output classes "target/classes"\n..`
  },
  {
    title: 'OCaml',
    code: `[tool ocamlc]\n\t.exec "ocamlc"\n..\n\n[bake ocaml_build]\n\t.make ml_src cglob "src/**/*.ml"\n\t[run ocamlc]\n\t\t.set "-g" 1\n\t\t.takes ml_src as "@args"\n\t\t.emits exe as "-o"\n\t..\n\t.output exe "target/out/ocaml_app.byte"\n..`
  },
  {
    title: 'C',
    code: `[tool cc]\n\t.exec "cc"\n..\n\n[bake c_build]\n\t.make c_src cglob "src/**/*.c"\n\t[run cc]\n\t\t.takes c_src as "@args"\n\t\t.set "-O2" 1\n\t\t.set "-g" 1\n\t\t.emits exe as "-o"\n\t..\n\t.output exe "target/out/app_c"\n..`
  }
];

const EXTRA_EXAMPLES: Example[] = [
  {
    title: 'Go',
    code: `[tool go]\n\t.exec "go"\n..\n\n[bake go_build]\n\t.make go_src cglob "src/**/*.go"\n\t[run go]\n\t\t.set "build" 1\n\t\t.set "-o" "target/out/app_go"\n\t\t.set "./src" 1\n\t..\n\t.output exe "target/out/app_go"\n..`
  },
  {
    title: 'C++',
    code: `[tool cxx]\n\t.exec "c++"\n..\n\n[bake cpp_build]\n\t.make cpp_src cglob "src/**/*.cpp"\n\t[run cxx]\n\t\t.takes cpp_src as "@args"\n\t\t.set "-O2" 1\n\t\t.set "-std=c++20" 1\n\t\t.emits exe as "-o"\n\t..\n\t.output exe "target/out/app_cpp"\n..`
  },
  {
    title: 'Zig',
    code: `[tool zig]\n\t.exec "zig"\n..\n\n[bake zig_build]\n\t.make zig_src cglob "src/**/*.zig"\n\t[run zig]\n\t\t.set "build-exe" 1\n\t\t.takes zig_src as "@args"\n\t\t.set "-O" "ReleaseFast"\n\t..\n\t.output exe "target/out/app_zig"\n..`
  },
  {
    title: 'C#',
    code: `[tool dotnet]\n\t.exec "dotnet"\n..\n\n[bake cs_build]\n\t.make csproj cglob "src/**/*.csproj"\n\t[run dotnet]\n\t\t.set "build" 1\n\t\t.takes csproj as "@args"\n\t\t.set "-c" "Release"\n\t..\n\t.output exe "target/out/app_cs"\n..`
  },
  {
    title: 'Kotlin',
    code: `[tool kotlinc]\n\t.exec "kotlinc"\n..\n\n[bake kt_build]\n\t.make kt_src cglob "src/**/*.kt"\n\t[run kotlinc]\n\t\t.takes kt_src as "@args"\n\t\t.set "-d" "target/out/app.jar"\n\t..\n\t.output jar "target/out/app.jar"\n..`
  }
];

const STORAGE_KEY = 'steel_lang';

@Injectable({ providedIn: 'root' })
export class AppState {
  readonly lang = signal<LangKey>('en');
  readonly t = computed(() => I18N[this.lang()]);
  readonly downloadUrl = DOWNLOAD_MAC_URL;
  readonly windowsDownloadUrl = DOWNLOAD_WINDOWS_URL;
  readonly examples = [...CORE_EXAMPLES, ...EXTRA_EXAMPLES];

  setLang(lang: LangKey): void {
    this.lang.set(lang);
    window.localStorage.setItem(STORAGE_KEY, lang);
  }

  initLang(): void {
    const stored = window.localStorage.getItem(STORAGE_KEY) as LangKey | null;
    if (stored && I18N[stored]) {
      this.lang.set(stored);
      return;
    }
    const detected = navigator.language.toLowerCase();
    if (detected.startsWith('fr')) {
      this.lang.set('fr');
    } else if (detected.startsWith('de')) {
      this.lang.set('de');
    } else if (detected.startsWith('it')) {
      this.lang.set('it');
    } else {
      this.lang.set('en');
    }
  }
}
