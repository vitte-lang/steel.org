import { computed, Injectable, signal } from '@angular/core';

type LangKey = 'en' | 'fr' | 'de' | 'it' | 'ar' | 'zh' | 'ja' | 'pt' | 'es';

type Example = {
  title: string;
  code: string;
};

type I18n = {
  nav: {
    home: string;
    downloads: string;
    docs: string;
    showcase: string;
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
    otherHint: string;
    vscodeTitle: string;
    vscodeDesc: string;
    vscodeButton: string;
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
  'https://github.com/vitte-lang/steel.org/releases';
const DOWNLOAD_LINUX_DEB_URL =
  'https://github.com/vitte-lang/steel.org/releases';
const DOWNLOAD_WINDOWS_URL =
  'https://github.com/vitte-lang/steel.org/releases';
const VSCODE_EXTENSION_URL =
  'https://marketplace.visualstudio.com/items?itemName=steelcommand.steel-command';

const I18N: Record<LangKey, I18n> = {
  en: {
    nav: {
      home: 'Home',
      downloads: 'Downloads',
      docs: 'Docs',
      showcase: 'Showcase',
      security: 'Security',
      status: 'Status'
    },
    hero: {
      eyebrow: 'Private Edition',
      title: 'Steel, delivered direct. No setup drama.',
      lead: 'Minimal text. Maximum examples. Download, run, copy, ship. Clear inputs, explicit outputs.',
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
      lead: 'Direct installer link with no extra steps. No scripts, no account.',
      macTitle: 'All platforms',
      macDesc: 'Open the GitHub releases page for every build.',
      macButton: 'Open Releases',
      macHint: 'macOS, Windows, Linux',
      otherTitle: 'Release page',
      otherDesc: 'Single link for all systems.',
      otherHint: 'GitHub Releases',
      vscodeTitle: 'VS Code Extension',
      vscodeDesc: 'Syntax and steelconf helpers inside VS Code.',
      vscodeButton: 'Open Marketplace',
      quickInstall: 'Quick install',
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
      lead: 'Small teams, mixed stacks, or one language done well. One bake per toolchain keeps builds readable.',
      cards: [
        { title: 'Solo projects', text: 'Start with one file and grow as needed.' },
        { title: 'Multi-language apps', text: 'Keep C, Rust, Swift, and Java together.' },
        { title: 'CI pipelines', text: 'Deterministic outputs and clear inputs.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Real-world steelconf patterns.',
      lead: 'Pick a pattern and adapt it to your stack. Each block shows tools, inputs, outputs.',
      cards: [
        { title: 'CLI app', text: 'One target, fast output, zero surprises.' },
        { title: 'App + lib', text: 'Split build steps with clean outputs.' },
        { title: 'Polyglot repo', text: 'One config per language, one build story.' }
      ],
      examplesTitle: 'Example blocks'
    },
    security: {
      eyebrow: 'Security',
      title: 'Security by default.',
      lead: 'Safe defaults, explicit tools, and clear outputs. Only declared tools run.',
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
      lead: 'Latest build pipelines and download availability. Plain metrics, no fluff.',
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
      lead: 'Short snippets you can drop into a project. Copy, run, see target/out.',
      checkTitle: 'Check install',
      runTitle: 'Run',
      minimalTitle: 'Minimal steelconf',
      editorTitle: 'Vim-style view',
      browseAll: 'Browse all examples'
    },
    docs: {
      eyebrow: 'Docs',
      title: 'Just enough to move fast.',
      lead: 'Short guides for beginners with real commands. Commands and patterns in plain words.',
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
      lead: 'What is new and what to expect next. Focused notes, fewer surprises.',
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
      showcase: 'Showcase',
      security: 'Securite',
      status: 'Statut'
    },
    hero: {
      eyebrow: 'Edition privee',
      title: 'Steel, telecharge direct. Sans prise de tete.',
      lead: 'Moins de texte. Plus d\'exemples. Telechargez, lancez, copiez. Entrees claires, sorties explicites.',
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
      lead: 'Lien direct, pas d\'etapes en plus. Sans script, sans compte.',
      macTitle: 'Toutes les plateformes',
      macDesc: 'Ouvrir la page releases pour tous les builds.',
      macButton: 'Ouvrir les releases',
      macHint: 'macOS, Windows, Linux',
      otherTitle: 'Page releases',
      otherDesc: 'Un seul lien pour tous les systemes.',
      otherHint: 'GitHub Releases',
      vscodeTitle: 'Extension VS Code',
      vscodeDesc: 'Syntaxe et aides steelconf dans VS Code.',
      vscodeButton: 'Ouvrir le Marketplace',
      quickInstall: 'Installation rapide',
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
      lead: 'Petites equipes, stacks mixtes, ou un seul langage. Un bake par toolchain, lecture simple.',
      cards: [
        { title: 'Projets solo', text: 'Un seul fichier pour demarrer vite.' },
        { title: 'Apps multi-langage', text: 'C, Rust, Swift et Java ensemble.' },
        { title: 'Pipelines CI', text: 'Sorties deterministes et entrees claires.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Patrons steelconf en production.',
      lead: 'Choisir un pattern et l\'adapter a votre stack. Chaque bloc montre outils, entrees, sorties.',
      cards: [
        { title: 'CLI', text: 'Un target, sortie rapide, sans surprise.' },
        { title: 'App + lib', text: 'Etapes claires et sorties propres.' },
        { title: 'Depot polyglotte', text: 'Une config par langage, un build.' }
      ],
      examplesTitle: 'Blocs d\'exemple'
    },
    security: {
      eyebrow: 'Securite',
      title: 'Securite par defaut.',
      lead: 'Outils explicites et sorties claires. Seuls les outils declares s\'executent.',
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
      lead: 'Telechargements et pipelines en temps reel. Chiffres clairs, sans marketing.',
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
      lead: 'Petits extraits a poser dans votre projet. Copier, lancer, voir target/out.',
      checkTitle: 'Verifier',
      runTitle: 'Lancer',
      minimalTitle: 'steelconf minimal',
      editorTitle: 'Vue type vim',
      browseAll: 'Voir tous les exemples'
    },
    docs: {
      eyebrow: 'Docs',
      title: 'Le strict minimum pour avancer vite.',
      lead: 'Guides courts avec commandes utiles. Commandes et patterns en clair.',
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
      lead: 'Nouveautes et prochaines etapes. Notes courtes, pas de surprise.',
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
      showcase: 'Showcase',
      security: 'Sicherheit',
      status: 'Status'
    },
    hero: {
      eyebrow: 'Private Edition',
      title: 'Steel, direkt geliefert. Kein Setup-Stress.',
      lead: 'Wenig Text. Viele Beispiele. Download, starten, kopieren. Klare Inputs, explizite Outputs.',
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
      lead: 'Direkter Installer-Link ohne Extra-Schritte. Ohne Skripte, ohne Konto.',
      macTitle: 'Alle Plattformen',
      macDesc: 'Release-Seite fur alle Builds.',
      macButton: 'Releases offnen',
      macHint: 'macOS, Windows, Linux',
      otherTitle: 'Release-Seite',
      otherDesc: 'Ein Link fur alle Systeme.',
      otherHint: 'GitHub Releases',
      vscodeTitle: 'VS Code Erweiterung',
      vscodeDesc: 'Syntax und steelconf-Hilfen in VS Code.',
      vscodeButton: 'Marketplace offnen',
      quickInstall: 'Schnellinstallation',
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
      lead: 'Kleine Teams, gemischte Stacks, oder ein Fokus. Ein Bake pro Toolchain, gut lesbar.',
      cards: [
        { title: 'Solo-Projekte', text: 'Ein File reicht fur den Start.' },
        { title: 'Multi-Language', text: 'C, Rust, Swift und Java zusammen.' },
        { title: 'CI-Pipelines', text: 'Deterministische Outputs und klare Inputs.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Steelconf Muster aus der Praxis.',
      lead: 'Wahle ein Muster und passe es an. Jeder Block zeigt Tools, Inputs, Outputs.',
      cards: [
        { title: 'CLI-App', text: 'Ein Target, schneller Output, keine Uberraschungen.' },
        { title: 'App + Lib', text: 'Klare Schritte und Outputs.' },
        { title: 'Polyglot Repo', text: 'Config pro Sprache, ein Build-Story.' }
      ],
      examplesTitle: 'Beispielbloecke'
    },
    security: {
      eyebrow: 'Sicherheit',
      title: 'Sicherheit by default.',
      lead: 'Explizite Tools und klare Outputs. Nur deklarierte Tools laufen.',
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
      lead: 'Downloads und Pipelines im Blick. Klare Metriken, ohne Marketing.',
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
      lead: 'Kurze Snippets fur dein Projekt. Kopieren, ausfuhren, target/out sehen.',
      checkTitle: 'Installation prufen',
      runTitle: 'Starten',
      minimalTitle: 'Minimales steelconf',
      editorTitle: 'Vim-Ansicht',
      browseAll: 'Alle Beispiele ansehen'
    },
    docs: {
      eyebrow: 'Dokumente',
      title: 'Nur das Notige, um schnell zu starten.',
      lead: 'Kurze Anleitungen mit echten Befehlen. Befehle und Muster in klaren Worten.',
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
      lead: 'Neues und was als nachstes kommt. Kurze Notizen, weniger Uberraschungen.',
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
      showcase: 'Showcase',
      security: 'Sicurezza',
      status: 'Stato'
    },
    hero: {
      eyebrow: 'Edizione privata',
      title: 'Steel, consegnato diretto. Niente complicazioni.',
      lead: 'Poco testo. Tanti esempi. Scarica, avvia, copia. Input chiari, output espliciti.',
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
      lead: 'Link diretto senza passaggi extra. Senza script, senza account.',
      macTitle: 'Tutte le piattaforme',
      macDesc: 'Apri la pagina release per tutti i build.',
      macButton: 'Apri release',
      macHint: 'macOS, Windows, Linux',
      otherTitle: 'Pagina release',
      otherDesc: 'Un link per tutti i sistemi.',
      otherHint: 'GitHub Releases',
      vscodeTitle: 'Estensione VS Code',
      vscodeDesc: 'Sintassi e aiuti steelconf in VS Code.',
      vscodeButton: 'Apri Marketplace',
      quickInstall: 'Installazione rapida',
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
      lead: 'Team piccoli, stack misti, o un solo linguaggio. Un bake per toolchain, piu leggibile.',
      cards: [
        { title: 'Progetti singoli', text: 'Un file basta per partire.' },
        { title: 'Multi-linguaggio', text: 'C, Rust, Swift e Java insieme.' },
        { title: 'Pipeline CI', text: 'Output deterministici e input chiari.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Pattern steelconf reali.',
      lead: 'Scegli un pattern e adattalo al tuo stack. Ogni blocco mostra tool, input, output.',
      cards: [
        { title: 'CLI', text: 'Un target, output rapido, zero sorprese.' },
        { title: 'App + lib', text: 'Passi chiari e output puliti.' },
        { title: 'Repo poliglott', text: 'Una config per linguaggio, un build.' }
      ],
      examplesTitle: 'Blocchi esempio'
    },
    security: {
      eyebrow: 'Sicurezza',
      title: 'Sicurezza di default.',
      lead: 'Tool espliciti e output chiari. Si eseguono solo i tool dichiarati.',
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
      lead: 'Download e pipeline piu recenti. Metriche chiare, senza marketing.',
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
      lead: 'Snippet brevi per il tuo progetto. Copia, esegui, vedi target/out.',
      checkTitle: 'Verifica',
      runTitle: 'Esegui',
      minimalTitle: 'steelconf minimale',
      editorTitle: 'Vista vim',
      browseAll: 'Vedi tutti gli esempi'
    },
    docs: {
      eyebrow: 'Documenti',
      title: 'Solo il necessario per partire.',
      lead: 'Guide brevi con comandi reali. Comandi e pattern in parole semplici.',
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
      lead: 'Novita e prossimi passi. Note concise, meno sorprese.',
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
  },
  ar: {
    nav: {
      home: 'الرئيسية',
      downloads: 'التنزيلات',
      docs: 'الوثائق',
      showcase: 'العروض',
      security: 'الأمان',
      status: 'الحالة'
    },
    hero: {
      eyebrow: 'إصدار خاص',
      title: 'Steel، تسليم مباشر. بلا تعقيد.',
      lead: 'نص قليل. أمثلة كثيرة. حمّل، شغّل، انسخ. مدخلات واضحة ومخرجات صريحة.',
      primary: 'حمّل Steel',
      ghost: 'شاهد الأمثلة',
      pills: ['سهل للمبتدئين', 'تثبيت نظيف', 'متعدد الأنظمة'],
      steps: ['حمّل المثبّت', 'نفّذ أمرًا واحدًا', 'ألصق مثالًا جاهزًا'],
      cardTitle: 'بداية سريعة',
      cardTag: 'v0.1.0_test'
    },
    download: {
      eyebrow: 'التنزيل',
      title: 'زر واحد. ملف واحد.',
      lead: 'رابط مباشر بلا خطوات إضافية. بدون سكربت وبدون حساب.',
      macTitle: 'كل المنصات',
      macDesc: 'افتح صفحة الإصدارات لكل النسخ.',
      macButton: 'افتح الإصدارات',
      macHint: 'macOS و Windows و Linux',
      otherTitle: 'صفحة الإصدارات',
      otherDesc: 'رابط واحد لكل الأنظمة.',
      otherHint: 'GitHub Releases',
      vscodeTitle: 'امتداد VS Code',
      vscodeDesc: 'صياغة ومساعدات steelconf داخل VS Code.',
      vscodeButton: 'فتح Marketplace',
      quickInstall: 'تثبيت سريع',
    },
    about: {
      eyebrow: 'لماذا Steel',
      title: 'دليل بناء واضح، بلا غموض.',
      cards: [
        {
          title: 'مدخلات واضحة',
          text: 'ملف واحد يصف ما يُبنى وأين تذهب المخرجات.'
        },
        {
          title: 'مخرجات ثابتة',
          text: 'Steel ينشئ خطة متوقعة يمكنك إعادة استخدامها.'
        },
        {
          title: 'للمبتدئين أولًا',
          text: 'أمثلة قصيرة يمكنك نسخها دون تعلم كل شيء.'
        }
      ]
    },
    showcase: {
      eyebrow: 'حالات الاستخدام',
      title: 'أين يناسب Steel.',
      lead: 'فرق صغيرة، تقنيات متعددة، أو لغة واحدة بإتقان. وصفة لكل أداة، قراءة أسهل.',
      cards: [
        { title: 'مشاريع فردية', text: 'ابدأ بملف واحد وتوسّع لاحقًا.' },
        { title: 'تطبيقات متعددة اللغات', text: 'اجمع C وRust وSwift وJava.' },
        { title: 'خطوط CI', text: 'مخرجات حتمية ومدخلات واضحة.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'أنماط steelconf حقيقية.',
      lead: 'اختر نمطًا وكيّفه مع تقنيتك. كل كتلة تعرض الأدوات والمدخلات والمخرجات.',
      cards: [
        { title: 'تطبيق CLI', text: 'هدف واحد، مخرجات سريعة، بلا مفاجآت.' },
        { title: 'تطبيق + مكتبة', text: 'قسّم البناء بمخرجات واضحة.' },
        { title: 'مستودع متعدد اللغات', text: 'ملف لكل لغة، قصة بناء واحدة.' }
      ],
      examplesTitle: 'كتل أمثلة'
    },
    security: {
      eyebrow: 'الأمان',
      title: 'أمان افتراضي.',
      lead: 'أدوات صريحة ومخرجات واضحة. تعمل الأدوات المعلنة فقط.',
      cards: [
        { title: 'أدوات صريحة', text: 'تُنفّذ الأدوات المصرّح بها فقط.' },
        { title: 'مدخلات حتمية', text: 'مدخلات معروفة لبناء ثابت.' },
        { title: 'تهيئة قابلة للتدقيق', text: 'Steel يولّد ملفًا موحّدًا.' },
        { title: 'سطح هجوم صغير', text: 'ملف واحد يوجّه البناء.' }
      ],
      policyTitle: 'سياسة الأمان',
      policyText: 'أبلغ عن المشاكل مع أقل إعادة إنتاج وتفاصيل نظام التشغيل/الأدوات.',
      timelineTitle: 'الجدول الزمني',
      timeline: ['اليوم 0: الاستلام', 'اليوم 3: الفرز', 'اليوم 7: الإصلاح']
    },
    status: {
      eyebrow: 'الحالة',
      title: 'الحالة الحالية للخدمات.',
      lead: 'أحدث التنزيلات وخطوط البناء. مقاييس واضحة بلا تسويق.',
      metrics: [
        { label: 'الجاهزية', value: '99.9%' },
        { label: 'زمن البناء', value: '< 2 دقيقة' },
        { label: 'زمن التنزيل', value: '< 1 دقيقة' }
      ],
      services: [
        { name: 'تنزيل macOS', state: 'متاح', text: 'مثبّت .pkg متوفر.' },
        { name: 'تنزيل Windows', state: 'قيد العمل', text: 'التغليف والتوقيع جاريان.' },
        { name: 'تنزيل Linux', state: 'قيد العمل', text: 'تحضير خط .deb.' }
      ],
      historyTitle: 'آخر التحديثات',
      history: [
        { title: 'نشر مثبّت macOS', date: '2026-01-20', text: 'حزمة .pkg شاملة جاهزة.' },
        { title: 'بدء خط Windows', date: '2026-01-18', text: 'التوقيع في مرحلة التجهيز.' }
      ]
    },
    examples: {
      eyebrow: 'أمثلة',
      title: 'انسخ، الصق، شغّل.',
      lead: 'مقاطع قصيرة لمشروعك. انسخ، شغّل، وشاهد target/out.',
      checkTitle: 'تحقق',
      runTitle: 'تشغيل',
      minimalTitle: 'steelconf بسيط',
      editorTitle: 'عرض vim',
      browseAll: 'عرض كل الأمثلة'
    },
    docs: {
      eyebrow: 'الوثائق',
      title: 'فقط ما تحتاجه للبداية.',
      lead: 'أدلة قصيرة مع أوامر حقيقية. أوامر وأنماط بكلمات واضحة.',
      sections: [
        { title: '1) أنشئ ملفًا', text: 'ضع steelconf في الجذر.' },
        { title: '2) عرّف الأدوات', text: 'صرّح بالمُصرّف المستخدم.' },
        { title: '3) أضف وصفة', text: 'صف البناء والمخرجات.' }
      ],
      commandsTitle: 'أوامر أساسية',
      commandList: ['steel --version', 'steel run', 'steel fmt', 'steel doctor'],
      guidesTitle: 'أدلة',
      guides: [
        { title: 'اختر لغة', text: 'ابدأ بمثال قريب.' },
        { title: 'مخرجات بسيطة', text: 'استخدم target/out/ لاختبار سريع.' },
        { title: 'التنسيق لاحقًا', text: 'ابدأ بالبناء أولًا.' }
      ],
      troubleshootingTitle: 'استكشاف الأخطاء',
      troubleshooting: [
        'إذا كانت أداة مفقودة، تحقق من PATH.',
        'إذا كانت ملفات مفقودة، تحقق من glob.',
        'إذا كان بطيئًا، قلّل المدخلات.'
      ],
      bestPracticesTitle: 'أفضل الممارسات',
      bestPractices: [
        'مسارات مخرجات ثابتة.',
        'سمِّ الأدوات بأسماء التنفيذ.',
        'ابدأ بهدف واحد فقط.'
      ]
    },
    faq: {
      eyebrow: 'الأسئلة الشائعة',
      title: 'إجابات سريعة.',
      items: [
        { q: 'هل يناسب المبتدئين؟', a: 'نعم. أمثلة قصيرة وقابلة للنسخ.' },
        { q: 'Windows + Linux؟', a: 'قيد العمل. .exe و .deb.' },
        { q: 'ما اسم الملف الرئيسي؟', a: 'اسم الملف هو steelconf.' },
        { q: 'اختبار سريع؟', a: 'حمّل ثم steel --version.' },
        { q: 'تحتاج مساعدة؟', a: 'ابدأ بالمثال الأدنى.' }
      ]
    },
    changelog: {
      eyebrow: 'سجل التغييرات',
      title: 'ملاحظات الإصدار',
      lead: 'الجديد وما القادم. ملاحظات مختصرة، مفاجآت أقل.',
      items: [
        {
          version: '0.1.0_test',
          date: '2026-01-20',
          notes: ['مثبّت macOS مباشر', 'أمثلة للمبتدئين', 'موقع متعدد اللغات']
        },
        {
          version: '0.1.0_next',
          date: 'قريبًا',
          notes: ['Windows .exe', 'Linux .deb', 'قوالب أكثر']
        }
      ]
    },
    footer: 'Steel إصدار خاص. تنزيلات مباشرة فقط.',
    labels: {
      language: 'اللغة'
    }
  },
  zh: {
    nav: {
      home: '首页',
      downloads: '下载',
      docs: '文档',
      showcase: '示例',
      security: '安全',
      status: '状态'
    },
    hero: {
      eyebrow: '私有版',
      title: 'Steel，直达交付。无需折腾。',
      lead: '文本极简，示例极多。下载、运行、复制、上线。输入清晰，输出明确。',
      primary: '下载 Steel',
      ghost: '查看示例',
      pills: ['新手友好', '安装干净', '多平台'],
      steps: ['下载安装包', '运行一个命令', '粘贴现成示例'],
      cardTitle: '快速开始',
      cardTag: 'v0.1.0_test'
    },
    download: {
      eyebrow: '下载',
      title: '一个按钮，一个文件。',
      lead: '直链安装，无额外步骤。无需脚本或账号。',
      macTitle: '所有平台',
      macDesc: '打开发布页查看所有构建。',
      macButton: '打开发布页',
      macHint: 'macOS / Windows / Linux',
      otherTitle: '发布页',
      otherDesc: '一个链接覆盖所有系统。',
      otherHint: 'GitHub Releases',
      vscodeTitle: 'VS Code 扩展',
      vscodeDesc: '在 VS Code 中提供 steelconf 语法与辅助。',
      vscodeButton: '打开 Marketplace',
      quickInstall: '快速安装',
    },
    about: {
      eyebrow: '为什么选择 Steel',
      title: '构建清晰，不再神秘。',
      cards: [
        {
          title: '输入清楚',
          text: '一个文件说明要构建什么，以及输出到哪里。'
        },
        {
          title: '输出稳定',
          text: 'Steel 生成可复用、可自动化的构建计划。'
        },
        {
          title: '新手优先',
          text: '短小示例，复制即可用。'
        }
      ]
    },
    showcase: {
      eyebrow: '使用场景',
      title: 'Steel 最合适的地方。',
      lead: '小团队、混合技术栈，或单一语言。每个 bake 对应一套工具链，易读可审。',
      cards: [
        { title: '个人项目', text: '用一个文件起步，按需扩展。' },
        { title: '多语言应用', text: '把 C、Rust、Swift、Java 放在一起。' },
        { title: 'CI 流水线', text: '确定性输出与清晰输入。' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: '真实 steelconf 模式。',
      lead: '选择一个模式并适配你的技术栈。每个代码块展示工具、输入、输出。',
      cards: [
        { title: 'CLI 应用', text: '一个目标，快速输出，零惊喜。' },
        { title: '应用 + 库', text: '清晰分步与整洁输出。' },
        { title: '多语言仓库', text: '每种语言一份配置，一个构建故事。' }
      ],
      examplesTitle: '示例模块'
    },
    security: {
      eyebrow: '安全',
      title: '默认安全。',
      lead: '显式工具，清晰输出。只运行已声明的工具。',
      cards: [
        { title: '显式工具', text: '只执行已声明的工具。' },
        { title: '确定性输入', text: '已知输入带来稳定构建。' },
        { title: '可审计配置', text: 'Steel 生成规范化文件。' },
        { title: '最小攻击面', text: '一个文件即可驱动构建。' }
      ],
      policyTitle: '安全政策',
      policyText: '报告问题时请提供最小复现与 OS/工具链细节。',
      timelineTitle: '时间线',
      timeline: ['第 0 天：接收', '第 3 天：分诊', '第 7 天：修复']
    },
    status: {
      eyebrow: '状态',
      title: '当前服务状态。',
      lead: '最新下载与流水线。指标清晰，不绕弯。',
      metrics: [
        { label: '可用性', value: '99.9%' },
        { label: '构建延迟', value: '< 2 分钟' },
        { label: '下载延迟', value: '< 1 分钟' }
      ],
      services: [
        { name: 'macOS 下载', state: '正常', text: '.pkg 安装包可用。' },
        { name: 'Windows 下载', state: '进行中', text: '打包与签名进行中。' },
        { name: 'Linux 下载', state: '进行中', text: '.deb 流水线准备中。' }
      ],
      historyTitle: '最近更新',
      history: [
        { title: '发布 macOS 安装包', date: '2026-01-20', text: '通用 .pkg 已就绪。' },
        { title: '启动 Windows 流水线', date: '2026-01-18', text: '签名在预发布中。' }
      ]
    },
    examples: {
      eyebrow: '示例',
      title: '复制、粘贴、运行。',
      lead: '适合项目的短片段。复制运行，查看 target/out。',
      checkTitle: '检查',
      runTitle: '运行',
      minimalTitle: '最小 steelconf',
      editorTitle: 'vim 视图',
      browseAll: '查看全部示例'
    },
    docs: {
      eyebrow: '文档',
      title: '启动只需这些。',
      lead: '带真实命令的短指南。命令与模式解释直白。',
      sections: [
        { title: '1) 创建文件', text: '在根目录创建 steelconf。' },
        { title: '2) 定义工具', text: '声明要使用的编译器。' },
        { title: '3) 添加配方', text: '描述构建与输出。' }
      ],
      commandsTitle: '核心命令',
      commandList: ['steel --version', 'steel run', 'steel fmt', 'steel doctor'],
      guidesTitle: '指南',
      guides: [
        { title: '选择语言', text: '从接近的示例开始。' },
        { title: '简单输出', text: '用 target/out/ 快速验证。' },
        { title: '稍后格式化', text: '先把构建跑通。' }
      ],
      troubleshootingTitle: '故障排查',
      troubleshooting: [
        '工具缺失时检查 PATH。',
        '文件缺失时检查 glob。',
        '速度慢时减少输入。'
      ],
      bestPracticesTitle: '最佳实践',
      bestPractices: ['输出路径稳定。', '工具名使用真实可执行文件名。', '先从一个 target 开始。']
    },
    faq: {
      eyebrow: 'FAQ',
      title: '快速解答。',
      items: [
        { q: '适合新手吗？', a: '是的。示例短小可复制。' },
        { q: 'Windows + Linux？', a: '进行中。.exe 与 .deb。' },
        { q: '主文件名？', a: '文件名为 steelconf。' },
        { q: '快速测试？', a: '下载后运行 steel --version。' },
        { q: '需要帮助？', a: '从最小示例开始。' }
      ]
    },
    changelog: {
      eyebrow: '变更日志',
      title: '发布说明',
      lead: '新内容与下一步。短说明，少意外。',
      items: [
        {
          version: '0.1.0_test',
          date: '2026-01-20',
          notes: ['macOS 直装包', '新手示例', '多语言站点']
        },
        {
          version: '0.1.0_next',
          date: '即将推出',
          notes: ['Windows .exe', 'Linux .deb', '更多模板']
        }
      ]
    },
    footer: 'Steel 私有版。仅直链下载。',
    labels: {
      language: '语言'
    }
  },
  ja: {
    nav: {
      home: 'ホーム',
      downloads: 'ダウンロード',
      docs: 'ドキュメント',
      showcase: 'ショーケース',
      security: 'セキュリティ',
      status: 'ステータス'
    },
    hero: {
      eyebrow: 'プライベート版',
      title: 'Steel、直送。セットアップの面倒なし。',
      lead: '短いテキスト。豊富な例。ダウンロードして実行、コピー。入力は明確、出力は明示。',
      primary: 'Steelをダウンロード',
      ghost: '例を見る',
      pills: ['初心者向け', 'クリーンインストール', 'マルチOS'],
      steps: ['インストーラを取得', 'コマンド1つ', 'すぐ使える例を貼り付け'],
      cardTitle: 'クイックスタート',
      cardTag: 'v0.1.0_test'
    },
    download: {
      eyebrow: 'ダウンロード',
      title: '1ボタン、1ファイル。',
      lead: '追加手順なしの直リンク。スクリプトやアカウント不要。',
      macTitle: '全プラットフォーム',
      macDesc: '全ビルドはリリースページにあります。',
      macButton: 'リリースを開く',
      macHint: 'macOS / Windows / Linux',
      otherTitle: 'リリースページ',
      otherDesc: '全システム共通のリンク。',
      otherHint: 'GitHub Releases',
      vscodeTitle: 'VS Code 拡張',
      vscodeDesc: 'VS Code で steelconf の構文と補助。',
      vscodeButton: 'Marketplace を開く',
      quickInstall: 'クイックインストール',
    },
    about: {
      eyebrow: 'なぜSteel',
      title: 'ビルドを明確に、謎なし。',
      cards: [
        {
          title: '明確な入力',
          text: '1ファイルで何を作るか、出力先を示します。'
        },
        {
          title: '安定した出力',
          text: 'Steelは再利用可能な計画を作成します。'
        },
        {
          title: '初心者優先',
          text: '短い例をそのままコピーできます。'
        }
      ]
    },
    showcase: {
      eyebrow: 'ユースケース',
      title: 'Steelが合う場面。',
      lead: '小さなチーム、混在スタック、または単一言語。1つのbakeに1つのツールチェーンで読みやすい。',
      cards: [
        { title: '個人プロジェクト', text: '1ファイルで始めて拡張。' },
        { title: '多言語アプリ', text: 'C/Rust/Swift/Java をまとめる。' },
        { title: 'CI パイプライン', text: '決定的な出力と明確な入力。' }
      ]
    },
    showcasePage: {
      eyebrow: 'ショーケース',
      title: '実際の steelconf パターン。',
      lead: 'パターンを選んでスタックに合わせる。各ブロックがツール・入力・出力を示す。',
      cards: [
        { title: 'CLI アプリ', text: '1ターゲット、速い出力、サプライズなし。' },
        { title: 'アプリ + ライブラリ', text: '明確なステップと綺麗な出力。' },
        { title: '多言語リポジトリ', text: '言語ごとに設定、1つのビルド。' }
      ],
      examplesTitle: '例のブロック'
    },
    security: {
      eyebrow: 'セキュリティ',
      title: 'デフォルトで安全。',
      lead: '明示的なツールと明確な出力。宣言したツールだけ実行。',
      cards: [
        { title: '明示的ツール', text: '宣言されたツールだけを実行。' },
        { title: '決定的入力', text: '既知の入力で安定ビルド。' },
        { title: '監査可能な設定', text: 'Steelが正規化ファイルを生成。' },
        { title: '最小攻撃面', text: '1ファイルでビルドを制御。' }
      ],
      policyTitle: 'セキュリティポリシー',
      policyText: '最小再現とOS/ツールチェーン情報を添えて報告してください。',
      timelineTitle: 'タイムライン',
      timeline: ['0日目: 受領', '3日目: トリアージ', '7日目: 修正']
    },
    status: {
      eyebrow: 'ステータス',
      title: 'サービスの現在状況。',
      lead: '最新のダウンロードとパイプライン。指標を分かりやすく表示。',
      metrics: [
        { label: '稼働率', value: '99.9%' },
        { label: 'ビルド遅延', value: '< 2分' },
        { label: 'ダウンロード遅延', value: '< 1分' }
      ],
      services: [
        { name: 'macOS ダウンロード', state: '稼働中', text: '.pkg が利用可能。' },
        { name: 'Windows ダウンロード', state: '準備中', text: 'パッケージングと署名中。' },
        { name: 'Linux ダウンロード', state: '準備中', text: '.deb パイプライン準備中。' }
      ],
      historyTitle: '最近の更新',
      history: [
        { title: 'macOS インストーラ公開', date: '2026-01-20', text: 'ユニバーサル .pkg 完了。' },
        { title: 'Windows パイプライン開始', date: '2026-01-18', text: '署名をステージング中。' }
      ]
    },
    examples: {
      eyebrow: '例',
      title: 'コピーして実行。',
      lead: 'プロジェクト向けの短いスニペット。コピーして実行、target/outを確認。',
      checkTitle: 'チェック',
      runTitle: '実行',
      minimalTitle: '最小 steelconf',
      editorTitle: 'vim ビュー',
      browseAll: 'すべての例を見る'
    },
    docs: {
      eyebrow: 'ドキュメント',
      title: '始めるために必要なだけ。',
      lead: '実際のコマンド付きの短いガイド。コマンドとパターンを平易に説明。',
      sections: [
        { title: '1) ファイル作成', text: 'ルートに steelconf を置く。' },
        { title: '2) ツール定義', text: '使うコンパイラを宣言。' },
        { title: '3) レシピ追加', text: 'ビルドと出力を記述。' }
      ],
      commandsTitle: '必須コマンド',
      commandList: ['steel --version', 'steel run', 'steel fmt', 'steel doctor'],
      guidesTitle: 'ガイド',
      guides: [
        { title: '言語を選ぶ', text: '近い例から始める。' },
        { title: 'シンプル出力', text: 'target/out/ で素早く確認。' },
        { title: '整形は後で', text: 'まず最小ビルドから。' }
      ],
      troubleshootingTitle: 'トラブルシューティング',
      troubleshooting: [
        'ツールが見つからない場合は PATH を確認。',
        'ファイルが見つからない場合は glob を確認。',
        '遅い場合は入力を減らす。'
      ],
      bestPracticesTitle: 'ベストプラクティス',
      bestPractices: ['出力パスを安定させる。', 'ツール名は実行ファイル名に。', 'ターゲットは1つから。']
    },
    faq: {
      eyebrow: 'FAQ',
      title: 'よくある質問。',
      items: [
        { q: '初心者向け？', a: 'はい。短い例をコピーできます。' },
        { q: 'Windows + Linux？', a: '準備中。.exe と .deb。' },
        { q: 'メインファイル名？', a: 'ファイル名は steelconf。' },
        { q: '簡単なテスト？', a: 'ダウンロード後に steel --version。' },
        { q: '助けが必要？', a: '最小例から始めてください。' }
      ]
    },
    changelog: {
      eyebrow: '変更履歴',
      title: 'リリースノート',
      lead: '新機能と次の予定。短く要点だけ。',
      items: [
        {
          version: '0.1.0_test',
          date: '2026-01-20',
          notes: ['macOS 直インストーラ', '初心者向け例', '多言語サイト']
        },
        {
          version: '0.1.0_next',
          date: '近日',
          notes: ['Windows .exe', 'Linux .deb', 'テンプレート追加']
        }
      ]
    },
    footer: 'Steel プライベート版。直接ダウンロードのみ。',
    labels: {
      language: '言語'
    }
  },
  pt: {
    nav: {
      home: 'Inicio',
      downloads: 'Downloads',
      docs: 'Documentacao',
      showcase: 'Showcase',
      security: 'Seguranca',
      status: 'Status'
    },
    hero: {
      eyebrow: 'Edicao privada',
      title: 'Steel, entregue direto. Sem drama de setup.',
      lead: 'Texto minimo. Muitos exemplos. Baixe, rode, copie. Entradas claras, saidas explicitas.',
      primary: 'Baixar Steel',
      ghost: 'Ver exemplos',
      pills: ['Amigavel para iniciantes', 'Instalacao limpa', 'Multi-OS'],
      steps: ['Baixe o instalador', 'Rode um comando', 'Cole um exemplo pronto'],
      cardTitle: 'Inicio rapido',
      cardTag: 'v0.1.0_test'
    },
    download: {
      eyebrow: 'Download',
      title: 'Um botao. Um arquivo.',
      lead: 'Link direto sem etapas extras. Sem scripts, sem conta.',
      macTitle: 'Todas as plataformas',
      macDesc: 'Abrir a pagina de releases para todos os builds.',
      macButton: 'Abrir releases',
      macHint: 'macOS, Windows, Linux',
      otherTitle: 'Pagina de releases',
      otherDesc: 'Um link para todos os sistemas.',
      otherHint: 'GitHub Releases',
      vscodeTitle: 'Extensao do VS Code',
      vscodeDesc: 'Sintaxe e ajudas steelconf no VS Code.',
      vscodeButton: 'Abrir Marketplace',
      quickInstall: 'Instalacao rapida',
    },
    about: {
      eyebrow: 'Por que Steel',
      title: 'Build claro, sem misterio.',
      cards: [
        {
          title: 'Entradas claras',
          text: 'Um arquivo descreve o que construir e onde saem os outputs.'
        },
        {
          title: 'Output estavel',
          text: 'Steel cria um plano previsivel para reutilizar e automatizar.'
        },
        {
          title: 'Primeiro os iniciantes',
          text: 'Exemplos curtos para copiar sem aprender tudo.'
        }
      ]
    },
    showcase: {
      eyebrow: 'Casos de uso',
      title: 'Onde o Steel funciona melhor.',
      lead: 'Times pequenos, stacks mistos ou uma linguagem so. Um bake por toolchain, leitura simples.',
      cards: [
        { title: 'Projetos solo', text: 'Comece com um arquivo e cresca.' },
        { title: 'Apps multi-linguagem', text: 'C, Rust, Swift e Java juntos.' },
        { title: 'Pipelines CI', text: 'Outputs deterministas e entradas claras.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Padroes steelconf reais.',
      lead: 'Escolha um padrao e adapte ao seu stack. Cada bloco mostra ferramentas, entradas, saidas.',
      cards: [
        { title: 'App CLI', text: 'Um target, output rapido, zero surpresa.' },
        { title: 'App + lib', text: 'Divida etapas com outputs limpos.' },
        { title: 'Repo poliglota', text: 'Uma config por linguagem, um build.' }
      ],
      examplesTitle: 'Blocos de exemplo'
    },
    security: {
      eyebrow: 'Seguranca',
      title: 'Seguranca por padrao.',
      lead: 'Ferramentas explicitas e outputs claros. So executa ferramentas declaradas.',
      cards: [
        { title: 'Ferramentas explicitas', text: 'So executa as ferramentas declaradas.' },
        { title: 'Entradas deterministas', text: 'Entradas conhecidas para build estavel.' },
        { title: 'Config auditavel', text: 'Steel gera um arquivo normalizado.' },
        { title: 'Superficie minima', text: 'Um arquivo guia todo o build.' }
      ],
      policyTitle: 'Politica de seguranca',
      policyText: 'Reporte problemas com repro minimo e detalhes de SO/toolchain.',
      timelineTitle: 'Linha do tempo',
      timeline: ['Dia 0: recebimento', 'Dia 3: triagem', 'Dia 7: correcao']
    },
    status: {
      eyebrow: 'Status',
      title: 'Status atual dos servicos.',
      lead: 'Downloads e pipelines mais recentes. Metricas claras, sem marketing.',
      metrics: [
        { label: 'Uptime', value: '99.9%' },
        { label: 'Latencia de build', value: '< 2 min' },
        { label: 'Latencia de download', value: '< 1 min' }
      ],
      services: [
        { name: 'Download macOS', state: 'Operacional', text: 'Instalador .pkg disponivel.' },
        { name: 'Download Windows', state: 'Em andamento', text: 'Packaging e assinatura em curso.' },
        { name: 'Download Linux', state: 'Em andamento', text: 'Pipeline .deb em preparo.' }
      ],
      historyTitle: 'Atualizacoes recentes',
      history: [
        { title: 'Instalador macOS publicado', date: '2026-01-20', text: 'Universal .pkg pronto.' },
        { title: 'Pipeline Windows iniciado', date: '2026-01-18', text: 'Assinatura em staging.' }
      ]
    },
    examples: {
      eyebrow: 'Exemplos',
      title: 'Copie, cole, rode.',
      lead: 'Snippets curtos para seu projeto. Copie, rode, veja target/out.',
      checkTitle: 'Verificar',
      runTitle: 'Executar',
      minimalTitle: 'steelconf minimo',
      editorTitle: 'Vista vim',
      browseAll: 'Ver todos os exemplos'
    },
    docs: {
      eyebrow: 'Documentos',
      title: 'So o necessario para comecar.',
      lead: 'Guias curtos com comandos reais. Comandos e padroes em linguagem simples.',
      sections: [
        { title: '1) Crie um arquivo', text: 'Crie o steelconf na raiz.' },
        { title: '2) Defina as ferramentas', text: 'Declare o compilador usado.' },
        { title: '3) Adicione uma receita', text: 'Descreva build e output.' }
      ],
      commandsTitle: 'Comandos essenciais',
      commandList: ['steel --version', 'steel run', 'steel fmt', 'steel doctor'],
      guidesTitle: 'Guias',
      guides: [
        { title: 'Escolha um idioma', text: 'Comece por um exemplo proximo.' },
        { title: 'Outputs simples', text: 'Use target/out/ para testar rapido.' },
        { title: 'Formatacao depois', text: 'Primeiro faca o build minimo.' }
      ],
      troubleshootingTitle: 'Solucao de problemas',
      troubleshooting: [
        'Se faltar ferramenta, verifique o PATH.',
        'Se faltar arquivo, verifique os globs.',
        'Se estiver lento, reduza as entradas.'
      ],
      bestPracticesTitle: 'Boas praticas',
      bestPractices: [
        'Caminhos de output estaveis.',
        'Ferramentas com nomes reais.',
        'Comece com um unico target.'
      ]
    },
    faq: {
      eyebrow: 'FAQ',
      title: 'Respostas rapidas.',
      items: [
        { q: 'Para iniciantes?', a: 'Sim. Exemplos curtos e copiaveis.' },
        { q: 'Windows + Linux?', a: 'Em andamento. .exe e .deb.' },
        { q: 'Arquivo principal?', a: 'O arquivo se chama steelconf.' },
        { q: 'Teste rapido?', a: 'Baixe e rode steel --version.' },
        { q: 'Precisa de ajuda?', a: 'Comece pelo exemplo minimo.' }
      ]
    },
    changelog: {
      eyebrow: 'Changelog',
      title: 'Notas de release',
      lead: 'Novidades e proximos passos. Notas curtas, menos surpresas.',
      items: [
        {
          version: '0.1.0_test',
          date: '2026-01-20',
          notes: ['Instalador macOS direto', 'Exemplos para iniciantes', 'Site multi-idioma']
        },
        {
          version: '0.1.0_next',
          date: 'Em breve',
          notes: ['Windows .exe', 'Linux .deb', 'Mais templates']
        }
      ]
    },
    footer: 'Steel Edicao privada. Apenas downloads diretos.',
    labels: {
      language: 'Idioma'
    }
  },
  es: {
    nav: {
      home: 'Inicio',
      downloads: 'Descargas',
      docs: 'Docs',
      showcase: 'Showcase',
      security: 'Seguridad',
      status: 'Estado'
    },
    hero: {
      eyebrow: 'Edicion privada',
      title: 'Steel, entrega directa. Sin drama de setup.',
      lead: 'Texto minimo. Maximos ejemplos. Descarga, ejecuta, copia. Entradas claras, salidas explicitas.',
      primary: 'Descargar Steel',
      ghost: 'Ver ejemplos',
      pills: ['Amigable para principiantes', 'Instalacion limpia', 'Multi-OS'],
      steps: ['Descarga el instalador', 'Ejecuta un comando', 'Pega un ejemplo listo'],
      cardTitle: 'Inicio rapido',
      cardTag: 'v0.1.0_test'
    },
    download: {
      eyebrow: 'Descarga',
      title: 'Un boton. Un archivo.',
      lead: 'Link directo sin pasos extra. Sin scripts, sin cuenta.',
      macTitle: 'Todas las plataformas',
      macDesc: 'Abrir la pagina de releases para todos los builds.',
      macButton: 'Abrir releases',
      macHint: 'macOS, Windows, Linux',
      otherTitle: 'Pagina de releases',
      otherDesc: 'Un solo enlace para todos los sistemas.',
      otherHint: 'GitHub Releases',
      vscodeTitle: 'Extension VS Code',
      vscodeDesc: 'Sintaxis y ayudas steelconf en VS Code.',
      vscodeButton: 'Abrir Marketplace',
      quickInstall: 'Instalacion rapida',
    },
    about: {
      eyebrow: 'Por que Steel',
      title: 'Build claro, sin misterios.',
      cards: [
        {
          title: 'Entradas claras',
          text: 'Un archivo describe que construir y donde van los outputs.'
        },
        {
          title: 'Salida estable',
          text: 'Steel crea un plan predecible que puedes reutilizar.'
        },
        {
          title: 'Primero principiantes',
          text: 'Ejemplos cortos para copiar sin aprender todo.'
        }
      ]
    },
    showcase: {
      eyebrow: 'Casos de uso',
      title: 'Donde Steel encaja mejor.',
      lead: 'Equipos pequenos, stacks mixtos o un solo lenguaje. Un bake por toolchain, lectura simple.',
      cards: [
        { title: 'Proyectos solo', text: 'Empieza con un archivo y crece.' },
        { title: 'Apps multi-lenguaje', text: 'C, Rust, Swift y Java juntos.' },
        { title: 'Pipelines CI', text: 'Salidas deterministas e inputs claros.' }
      ]
    },
    showcasePage: {
      eyebrow: 'Showcase',
      title: 'Patrones steelconf reales.',
      lead: 'Elige un patron y adaptalo a tu stack. Cada bloque muestra herramientas, entradas, salidas.',
      cards: [
        { title: 'App CLI', text: 'Un target, salida rapida, cero sorpresas.' },
        { title: 'App + lib', text: 'Divide pasos con salidas limpias.' },
        { title: 'Repo poliglota', text: 'Una config por lenguaje, un build.' }
      ],
      examplesTitle: 'Bloques de ejemplo'
    },
    security: {
      eyebrow: 'Seguridad',
      title: 'Seguridad por defecto.',
      lead: 'Herramientas explicitas y salidas claras. Solo corren herramientas declaradas.',
      cards: [
        { title: 'Herramientas explicitas', text: 'Solo se ejecutan las herramientas declaradas.' },
        { title: 'Entradas deterministas', text: 'Entradas conocidas para builds estables.' },
        { title: 'Config auditable', text: 'Steel genera un archivo normalizado.' },
        { title: 'Superficie minima', text: 'Un archivo guia todo el build.' }
      ],
      policyTitle: 'Politica de seguridad',
      policyText: 'Reporta problemas con repro minimo y detalles de SO/toolchain.',
      timelineTitle: 'Linea de tiempo',
      timeline: ['Dia 0: recepcion', 'Dia 3: triage', 'Dia 7: fix']
    },
    status: {
      eyebrow: 'Estado',
      title: 'Estado actual de los servicios.',
      lead: 'Descargas y pipelines mas recientes. Metricas claras, sin marketing.',
      metrics: [
        { label: 'Uptime', value: '99.9%' },
        { label: 'Latencia build', value: '< 2 min' },
        { label: 'Latencia descarga', value: '< 1 min' }
      ],
      services: [
        { name: 'Descarga macOS', state: 'Operativo', text: 'Instalador .pkg disponible.' },
        { name: 'Descarga Windows', state: 'En progreso', text: 'Packaging y firma en curso.' },
        { name: 'Descarga Linux', state: 'En progreso', text: 'Pipeline .deb en preparacion.' }
      ],
      historyTitle: 'Actualizaciones recientes',
      history: [
        { title: 'Instalador macOS publicado', date: '2026-01-20', text: 'Universal .pkg listo.' },
        { title: 'Pipeline Windows iniciado', date: '2026-01-18', text: 'Firma en staging.' }
      ]
    },
    examples: {
      eyebrow: 'Ejemplos',
      title: 'Copia, pega, ejecuta.',
      lead: 'Snippets cortos para tu proyecto. Copia, ejecuta, mira target/out.',
      checkTitle: 'Verificar',
      runTitle: 'Ejecutar',
      minimalTitle: 'steelconf minimo',
      editorTitle: 'Vista vim',
      browseAll: 'Ver todos los ejemplos'
    },
    docs: {
      eyebrow: 'Documentos',
      title: 'Solo lo necesario para empezar.',
      lead: 'Guias cortas con comandos reales. Comandos y patrones en lenguaje claro.',
      sections: [
        { title: '1) Crea un archivo', text: 'Crea steelconf en la raiz.' },
        { title: '2) Define herramientas', text: 'Declara el compilador a usar.' },
        { title: '3) Agrega una receta', text: 'Describe el build y la salida.' }
      ],
      commandsTitle: 'Comandos esenciales',
      commandList: ['steel --version', 'steel run', 'steel fmt', 'steel doctor'],
      guidesTitle: 'Guias',
      guides: [
        { title: 'Elige un lenguaje', text: 'Empieza con un ejemplo cercano.' },
        { title: 'Salidas simples', text: 'Usa target/out/ para probar rapido.' },
        { title: 'Formato despues', text: 'Primero construye lo minimo.' }
      ],
      troubleshootingTitle: 'Solucion de problemas',
      troubleshooting: [
        'Si falta una herramienta, revisa PATH.',
        'Si faltan archivos, revisa los globs.',
        'Si es lento, reduce los inputs.'
      ],
      bestPracticesTitle: 'Buenas practicas',
      bestPractices: [
        'Rutas de salida estables.',
        'Herramientas con nombres reales.',
        'Empieza con un solo target.'
      ]
    },
    faq: {
      eyebrow: 'FAQ',
      title: 'Respuestas rapidas.',
      items: [
        { q: 'Para principiantes?', a: 'Si. Ejemplos cortos y copiables.' },
        { q: 'Windows + Linux?', a: 'En progreso. .exe y .deb.' },
        { q: 'Archivo principal?', a: 'El archivo se llama steelconf.' },
        { q: 'Prueba rapida?', a: 'Descarga y ejecuta steel --version.' },
        { q: 'Necesitas ayuda?', a: 'Empieza por el ejemplo minimo.' }
      ]
    },
    changelog: {
      eyebrow: 'Changelog',
      title: 'Notas de release',
      lead: 'Novedades y proximos pasos. Notas breves, menos sorpresas.',
      items: [
        {
          version: '0.1.0_test',
          date: '2026-01-20',
          notes: ['Instalador macOS directo', 'Ejemplos para principiantes', 'Sitio multi-idioma']
        },
        {
          version: '0.1.0_next',
          date: 'Pronto',
          notes: ['Windows .exe', 'Linux .deb', 'Mas templates']
        }
      ]
    },
    footer: 'Steel Edicion privada. Solo descargas directas.',
    labels: {
      language: 'Idioma'
    }
  }
};

const CORE_EXAMPLES: Example[] = [
  {
    title: 'Swift',
    code: `!muf 4\n\n[workspace]\n\t.set name "swift_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "debug"\n..\n\n[profile debug]\n\t.set opt 0\n\t.set debug 1\n..\n\n[profile release]\n\t.set opt 2\n\t.set debug 0\n..\n\n[tool swiftc]\n\t.exec "swiftc"\n..\n\n[bake build_debug]\n\t.make swift_src cglob "Sources/**/*.swift"\n\t[run swiftc]\n\t\t.set "-O${'${'}opt}" 1\n\t\t.set "-g" "${'${'}debug}"\n\t\t.takes swift_src as "@args"\n\t\t.emits exe as "-o"\n\t..\n\t.output exe "target/out/swift_app_debug"\n..\n\n[export]\n\t.ref build_debug\n..`
  },
  {
    title: 'Rust',
    code: `!muf 4\n\n[workspace]\n\t.set name "rust_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "debug"\n..\n\n[tool sh]\n\t.exec "sh"\n..\n\n[bake rust_build]\n\t.make rust_src cglob "src/**/*.rs"\n\t[run sh]\n\t\t.set "-c" "cargo build"\n\t..\n\t.output exe "target/debug/steel"\n..\n\n[export]\n\t.ref rust_build\n..`
  },
  {
    title: 'Python',
    code: `!muf 4\n\n[workspace]\n\t.set name "python_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "debug"\n..\n\n[tool python]\n\t.exec "python3"\n..\n\n[bake python_run]\n\t.make py_src cglob "src/**/*.py"\n\t[run python]\n\t\t.set "-u" 1\n\t\t.set "-m" "src.main"\n\t..\n\t.output exe "target/out/python.run"\n..\n\n[export]\n\t.ref python_run\n..`
  },
  {
    title: 'Java',
    code: `!muf 4\n\n[workspace]\n\t.set name "java_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "debug"\n..\n\n[tool javac]\n\t.exec "javac"\n..\n\n[bake java_build]\n\t.make java_src cglob "src/**/*.java"\n\t[run javac]\n\t\t.set "-g" 1\n\t\t.set "-d" "target/classes"\n\t\t.takes java_src as "@args"\n\t..\n\t.output classes "target/classes"\n..\n\n[export]\n\t.ref java_build\n..`
  },
  {
    title: 'OCaml',
    code: `!muf 4\n\n[workspace]\n\t.set name "ocaml_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "debug"\n..\n\n[tool ocamlc]\n\t.exec "ocamlc"\n..\n\n[bake ocaml_build]\n\t.make ml_src cglob "src/**/*.ml"\n\t[run ocamlc]\n\t\t.set "-g" 1\n\t\t.takes ml_src as "@args"\n\t\t.emits exe as "-o"\n\t..\n\t.output exe "target/out/ocaml_app.byte"\n..\n\n[export]\n\t.ref ocaml_build\n..`
  },
  {
    title: 'C',
    code: `!muf 4\n\n[workspace]\n\t.set name "c_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "debug"\n..\n\n[profile debug]\n\t.set opt 0\n\t.set debug 1\n..\n\n[profile release]\n\t.set opt 2\n\t.set debug 0\n..\n\n[tool cc]\n\t.exec "cc"\n..\n\n[bake c_build]\n\t.make c_src cglob "src/**/*.c"\n\t[run cc]\n\t\t.takes c_src as "@args"\n\t\t.set "-O${'${'}opt}" 1\n\t\t.set "-g" "${'${'}debug}"\n\t\t.emits exe as "-o"\n\t..\n\t.output exe "target/out/app_c"\n..\n\n[export]\n\t.ref c_build\n..`
  }
];

const EXTRA_EXAMPLES: Example[] = [
  {
    title: 'Go',
    code: `!muf 4\n\n[workspace]\n\t.set name "go_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "debug"\n..\n\n[tool go]\n\t.exec "go"\n..\n\n[bake go_build]\n\t.make go_src cglob "src/**/*.go"\n\t[run go]\n\t\t.set "build" 1\n\t\t.set "-o" "target/out/app_go"\n\t\t.set "./src" 1\n\t..\n\t.output exe "target/out/app_go"\n..\n\n[export]\n\t.ref go_build\n..`
  },
  {
    title: 'C++',
    code: `!muf 4\n\n[workspace]\n\t.set name "cpp_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "debug"\n..\n\n[profile debug]\n\t.set opt 0\n\t.set debug 1\n..\n\n[profile release]\n\t.set opt 2\n\t.set debug 0\n..\n\n[tool cxx]\n\t.exec "c++"\n..\n\n[bake cpp_build]\n\t.make cpp_src cglob "src/**/*.cpp"\n\t[run cxx]\n\t\t.takes cpp_src as "@args"\n\t\t.set "-O${'${'}opt}" 1\n\t\t.set "-std=c++20" 1\n\t\t.set "-g" "${'${'}debug}"\n\t\t.emits exe as "-o"\n\t..\n\t.output exe "target/out/app_cpp"\n..\n\n[export]\n\t.ref cpp_build\n..`
  },
  {
    title: 'Zig',
    code: `!muf 4\n\n[workspace]\n\t.set name "zig_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "release"\n..\n\n[tool zig]\n\t.exec "zig"\n..\n\n[bake zig_build]\n\t.make zig_src cglob "src/**/*.zig"\n\t[run zig]\n\t\t.set "build-exe" 1\n\t\t.takes zig_src as "@args"\n\t\t.set "-O" "ReleaseFast"\n\t..\n\t.output exe "target/out/app_zig"\n..\n\n[export]\n\t.ref zig_build\n..`
  },
  {
    title: 'C#',
    code: `!muf 4\n\n[workspace]\n\t.set name "csharp_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "release"\n..\n\n[tool dotnet]\n\t.exec "dotnet"\n..\n\n[bake cs_build]\n\t.make csproj cglob "src/**/*.csproj"\n\t[run dotnet]\n\t\t.set "build" 1\n\t\t.takes csproj as "@args"\n\t\t.set "-c" "Release"\n\t..\n\t.output exe "target/out/app_cs"\n..\n\n[export]\n\t.ref cs_build\n..`
  },
  {
    title: 'Kotlin',
    code: `!muf 4\n\n[workspace]\n\t.set name "kotlin_app"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "release"\n..\n\n[tool kotlinc]\n\t.exec "kotlinc"\n..\n\n[bake kt_build]\n\t.make kt_src cglob "src/**/*.kt"\n\t[run kotlinc]\n\t\t.takes kt_src as "@args"\n\t\t.set "-d" "target/out/app.jar"\n\t..\n\t.output jar "target/out/app.jar"\n..\n\n[export]\n\t.ref kt_build\n..`
  }
];

const STORAGE_KEY = 'steel_lang';

@Injectable({ providedIn: 'root' })
export class AppState {
  readonly lang = signal<LangKey>('en');
  readonly t = computed(() => I18N[this.lang()]);
  readonly isRtl = computed(() => this.lang() === 'ar');
  readonly downloadUrl = DOWNLOAD_MAC_URL;
  readonly downloadLinuxUrl = DOWNLOAD_LINUX_DEB_URL;
  readonly downloadWindowsUrl = DOWNLOAD_WINDOWS_URL;
  readonly vscodeExtensionUrl = VSCODE_EXTENSION_URL;
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
    if (detected.startsWith('ar')) {
      this.lang.set('ar');
    } else if (detected.startsWith('es')) {
      this.lang.set('es');
    } else if (detected.startsWith('zh')) {
      this.lang.set('zh');
    } else if (detected.startsWith('ja')) {
      this.lang.set('ja');
    } else if (detected.startsWith('pt')) {
      this.lang.set('pt');
    } else if (detected.startsWith('fr')) {
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
