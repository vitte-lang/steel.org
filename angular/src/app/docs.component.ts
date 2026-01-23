import { CommonModule } from '@angular/common';
import { Component, OnInit, AfterViewInit, OnDestroy, ViewChild, ElementRef } from '@angular/core';
import { EditorState, Extension } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { autocompletion, Completion, CompletionContext, completionKeymap } from '@codemirror/autocomplete';
import { HighlightStyle, StreamLanguage, StringStream, syntaxHighlighting } from '@codemirror/language';
import { Diagnostic as CmdDiagnostic, linter } from '@codemirror/lint';
import { tags } from '@lezer/highlight';
import { cpp } from '@codemirror/lang-cpp';

import { AppState } from './app.state';

type DiagnosticLevel = 'error' | 'warning' | 'suggestion';

type Diagnostic = {
  level: DiagnosticLevel;
  message: string;
  line?: number;
};

const GEANY_THEME = EditorView.theme({
  '&': {
    backgroundColor: '#f9f6f1',
    color: '#2b2722',
    borderRadius: '12px'
  },
  '.cm-content': {
    padding: '12px 14px',
    fontFamily:
      '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
    fontSize: '0.92rem'
  },
  '.cm-gutters': {
    backgroundColor: '#efe6dc',
    color: '#6e5c52',
    borderRight: '1px solid #e0d2c6'
  },
  '.cm-activeLine': {
    backgroundColor: 'rgba(120, 97, 86, 0.12)'
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'rgba(120, 97, 86, 0.18)'
  },
  '.cm-cursor': {
    borderLeftColor: '#2b2722'
  },
  '.cm-selectionBackground': {
    backgroundColor: 'rgba(166, 95, 68, 0.22)'
  },
  '.cm-tooltip-autocomplete': {
    backgroundColor: '#fffaf6',
    border: '1px solid #e3d4c7',
    boxShadow: '0 12px 24px rgba(52, 36, 22, 0.12)'
  },
  '.cm-tooltip-autocomplete ul li[aria-selected]': {
    backgroundColor: '#f2d9c7',
    color: '#3a271a'
  }
});

const GEANY_HIGHLIGHT = HighlightStyle.define([
  { tag: tags.comment, color: '#8a7f74' },
  { tag: tags.keyword, color: '#7b2d26', fontWeight: '600' },
  { tag: [tags.string, tags.special(tags.string)], color: '#1b6d4c' },
  { tag: [tags.number, tags.bool], color: '#2b4f9b' },
  { tag: tags.function(tags.variableName), color: '#6b3f1c' },
  { tag: tags.operator, color: '#5e3a2c' },
  { tag: tags.definition(tags.variableName), color: '#2d3f6b' },
  { tag: tags.typeName, color: '#4c2a6b' }
]);

const STEELCONF_LANGUAGE = StreamLanguage.define({
  startState() {
    return {};
  },
  token(stream: StringStream) {
    if (stream.sol() && stream.match(/\s*!muf\b/)) {
      return 'keyword';
    }
    if (stream.sol() && stream.match(/\s*\[[^\]]+\]/)) {
      return 'tag';
    }
    if (stream.match(/^\s*#.*$/)) {
      return 'comment';
    }
    if (stream.match(/"[^"]*"/)) {
      return 'string';
    }
    if (stream.match(/\.[a-z_]+/)) {
      return 'keyword';
    }
    if (stream.match(/\b\d+\b/)) {
      return 'number';
    }
    stream.next();
    return null;
  }
});

@Component({
  selector: 'app-docs',
  imports: [CommonModule],
  templateUrl: './docs.component.html'
})
export class DocsComponent implements OnInit, AfterViewInit, OnDestroy {
  @ViewChild('cEditor', { static: true }) cEditorRef!: ElementRef<HTMLDivElement>;
  @ViewChild('steelEditor', { static: true }) steelEditorRef!: ElementRef<HTMLDivElement>;

  private cView?: EditorView;
  private steelView?: EditorView;

  private readonly defaultCSource = `#include <stdio.h>

int main(void) {
  printf("Hello Steel\\n");
  fflush(stdout);
  return 0;
}`;
  private readonly defaultSteelconf = `!muf 4

[workspace]
  .set name "demo"
  .set root "."
  .set target_dir "target"
  .set profile "debug"
..

[profile debug]
  .set mode "debug"
..

[tool cc]
  .exec "cc"
..

[bake build]
  .make c_src cglob "src/**/*.c"
  [run cc]
    .takes c_src as "@args"
    .set "-O2" 1
    .set "-g" 1
    .emits exe as "-o"
  ..
  .output exe "target/out/app.exe"
..

[export]
  .ref build
..
`;

  cSource = this.defaultCSource;

  steelconf = this.defaultSteelconf;

  diagnostics: Diagnostic[] = [];
  buildStatus = '';
  cSuggestions = [
    '#include <stdio.h>',
    '#include <stdlib.h>',
    '#include <string.h>',
    '#include <stdbool.h>',
    '#include <stdint.h>',
    'int main(void) {',
    'printf("Hello\\n");',
    'puts("Hello");',
    'return 0;',
    'int',
    'void',
    'char',
    'float',
    'double',
    'long',
    'short',
    'unsigned',
    'signed',
    'size_t',
    'bool',
    'true',
    'false',
    'struct',
    'typedef',
    'if',
    'else',
    'for',
    'while',
    'switch',
    'case',
    'break',
    'continue',
    'do',
    'goto',
    'return',
    'malloc',
    'free',
    'strlen',
    'strcpy',
    'memcpy',
    'memcmp',
    'strncpy',
    'snprintf',
    'fgets',
    'scanf',
    'fopen',
    'fclose',
    'fread',
    'fwrite',
    'FILE',
    'stdout',
    'stderr'
  ];
  steelSuggestions = [
    '[workspace]',
    '[profile debug]',
    '[tool cc]',
    '[bake build]',
    '[run cc]',
    '[export]',
    '.set',
    '.exec',
    '.make',
    '.needs',
    '.output',
    '.takes',
    '.emits',
    '.ref',
    'steel',
    'bake',
    'store',
    'capsule',
    'var',
    'profile',
    'tool',
    'plan',
    'switch',
    'set',
    'run',
    'exports',
    'wire',
    'export',
    'in',
    'out',
    'make',
    'takes',
    'emits',
    'output',
    'at',
    'cache',
    'mode',
    'path',
    'env',
    'fs',
    'net',
    'time',
    'allow',
    'deny',
    'allow_read',
    'allow_write',
    'allow_write_exact',
    'stable'
  ];

  constructor(public state: AppState) {}

  ngOnInit(): void {
  }

  ngAfterViewInit(): void {
    this.cView = this.createEditor(
      this.cEditorRef.nativeElement,
      this.cSource,
      cpp(),
      this.cSuggestions,
      (value) => {
        this.cSource = value;
        this.analyze();
      },
      'c'
    );
    this.steelView = this.createEditor(
      this.steelEditorRef.nativeElement,
      this.steelconf,
      STEELCONF_LANGUAGE,
      this.steelSuggestions,
      (value) => {
        this.steelconf = value;
        this.analyze();
      },
      'steel'
    );
  }

  ngOnDestroy(): void {
    this.cView?.destroy();
    this.steelView?.destroy();
  }

  get t() {
    return this.state.t();
  }

  get examples() {
    return this.state.examples;
  }

  analyze(): void {
    const diags: Diagnostic[] = [];
    diags.push(...this.analyzeC(this.cSource));
    diags.push(...this.analyzeSteelconf(this.steelconf));
    this.diagnostics = diags
      .map((d) => ({
        ...d,
        level: this.classify(d.message)
      }))
      .sort((a, b) => (a.line ?? Number.MAX_SAFE_INTEGER) - (b.line ?? Number.MAX_SAFE_INTEGER));
  }

  private lineOf(src: string, match: RegExp): number | undefined {
    const m = match.exec(src);
    if (!m || m.index === undefined) {
      return undefined;
    }
    return src.slice(0, m.index).split('\n').length;
  }

  runBuild(): void {
    this.analyze();
    const stamp = new Date().toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    this.buildStatus = `Build ran at ${stamp}`;
    this.cView?.dispatch({ changes: { from: 0, to: 0, insert: '' } });
    this.steelView?.dispatch({ changes: { from: 0, to: 0, insert: '' } });
  }

  resetEditor(kind: 'c' | 'steel'): void {
    if (kind === 'c') {
      this.cSource = this.defaultCSource;
      this.cView?.dispatch({
        changes: { from: 0, to: this.cView.state.doc.length, insert: this.cSource }
      });
      return;
    }
    this.steelconf = this.defaultSteelconf;
    this.steelView?.dispatch({
      changes: { from: 0, to: this.steelView.state.doc.length, insert: this.steelconf }
    });
  }

  private createEditor(
    parent: HTMLElement,
    doc: string,
    language: Extension,
    suggestions: string[],
    onChange: (value: string) => void,
    kind: 'c' | 'steel'
  ): EditorView {
    const completionSource = this.makeCompletionSource(suggestions);
    return new EditorView({
      parent,
      state: EditorState.create({
        doc,
        extensions: [
          lineNumbers(),
          highlightActiveLineGutter(),
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap, ...completionKeymap, indentWithTab]),
          language,
          autocompletion({ override: [completionSource], activateOnTyping: true }),
          linter(() => this.buildLintDiagnostics(kind)),
          EditorView.lineWrapping,
          EditorView.updateListener.of((update: { docChanged: boolean; state: EditorState }) => {
            if (update.docChanged) {
              onChange(update.state.doc.toString());
            }
          }),
          GEANY_THEME,
          syntaxHighlighting(GEANY_HIGHLIGHT)
        ]
      })
    });
  }

  private makeCompletionSource(suggestions: string[]) {
    const options: Completion[] = suggestions.map((label) => ({
      label,
      type: this.completionType(label)
    }));
    return (context: CompletionContext) => {
      const word = context.matchBefore(/[A-Za-z0-9_./-]+/);
      if (!word || (word.from === word.to && !context.explicit)) {
        const prev = context.pos > 0 ? context.state.doc.sliceString(context.pos - 1, context.pos) : '';
        if (!context.explicit && prev !== '.' && prev !== '[') {
          return null;
        }
        return {
          from: context.pos,
          options,
          validFor: /^[A-Za-z0-9_./-]+$/
        };
      }
      return {
        from: word.from,
        options,
        validFor: /^[A-Za-z0-9_./-]+$/
      };
    };
  }

  private completionType(label: string): Completion['type'] {
    if (label.startsWith('.')) return 'keyword';
    if (label.startsWith('[')) return 'type';
    if (label.endsWith(')') || label.includes('(')) return 'function';
    if (label.startsWith('#include')) return 'keyword';
    return 'variable';
  }

  private buildLintDiagnostics(kind: 'c' | 'steel'): CmdDiagnostic[] {
    const diags = kind === 'c' ? this.analyzeC(this.cSource) : this.analyzeSteelconf(this.steelconf);
    const view = kind === 'c' ? this.cView : this.steelView;
    if (!view) return [];
    return diags
      .map((d) => {
        if (!d.line) {
          return null;
        }
        const line = Math.max(1, Math.min(d.line, view.state.doc.lines));
        const lineInfo = view.state.doc.line(line);
        return {
          from: lineInfo.from,
          to: lineInfo.to,
          severity: d.level === 'error' ? 'error' : d.level === 'warning' ? 'warning' : 'info',
          message: d.message
        } as CmdDiagnostic;
      })
      .filter((d): d is CmdDiagnostic => Boolean(d));
  }

  private analyzeC(src: string): Diagnostic[] {
    const diags: Diagnostic[] = [];
    const lines = src.split('\n');
    const hasMain = /(\bint|\bvoid)\s+main\s*\(/.test(src);
    if (!hasMain) {
      diags.push({ level: 'error', message: 'C: fonction main manquante.' });
    }
    if (/\bmain\s*\(/.test(src) && !/\bint\s+main\s*\(/.test(src)) {
      diags.push({ level: 'error', message: 'C: main devrait retourner int.' });
    }
    if (/^\s*$/.test(src)) {
      diags.push({ level: 'error', message: 'C: fichier vide.' });
    }

    const openBraces = (src.match(/{/g) || []).length;
    const closeBraces = (src.match(/}/g) || []).length;
    if (openBraces !== closeBraces) {
      diags.push({ level: 'error', message: 'C: accolades non equilibrees.' });
    }

    const usesPrintf = /\bprintf\s*\(/.test(src);
    const usesScanf = /\bscanf\s*\(/.test(src);
    const usesMalloc = /\bmalloc\s*\(/.test(src);
    const usesFree = /\bfree\s*\(/.test(src);
    const usesStrlen = /\bstrlen\s*\(/.test(src);
    const usesStrcpy = /\bstrcpy\s*\(/.test(src);
    const usesPuts = /\bputs\s*\(/.test(src);
    const hasStdio = /#include\s*<stdio\.h>/.test(src);
    const hasStdlib = /#include\s*<stdlib\.h>/.test(src);
    const hasString = /#include\s*<string\.h>/.test(src);
    if ((usesPrintf || usesScanf) && !hasStdio) {
      diags.push({ level: 'error', message: 'C: ajoutez #include <stdio.h>.' });
    }
    if (usesPuts && !hasStdio) {
      diags.push({ level: 'error', message: 'C: ajoutez #include <stdio.h> pour puts().' });
    }
    if ((usesMalloc || usesFree) && !hasStdlib) {
      diags.push({ level: 'error', message: 'C: ajoutez #include <stdlib.h>.' });
    }
    if ((usesStrlen || usesStrcpy) && !hasString) {
      diags.push({ level: 'error', message: 'C: ajoutez #include <string.h>.' });
    }

    const varDecls = src.match(/\b(int|float|double|char|long|short|unsigned)\s+[a-zA-Z_]\w*/g) || [];
    varDecls.forEach((decl) => {
      const name = decl.split(/\s+/).pop() || '';
      if (name === 'main') {
        return;
      }
      if (name && (src.match(new RegExp(`\\b${name}\\b`, 'g')) || []).length <= 1) {
        diags.push({ level: 'error', message: `C: variable inutilisee "${name}".` });
      }
    });

    lines.forEach((line, idx) => {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('//') || trimmed.startsWith('#')) {
        return;
      }
      if (trimmed.includes('\t')) {
        diags.push({ level: 'error', message: `C: tabulation detectee ligne ${idx + 1}.` });
      }
      if (trimmed.includes('\t')) {
        diags.push({ level: 'error', message: `C: tabulation detectee ligne ${idx + 1}.` });
      }
      if (
        trimmed.endsWith(';') ||
        trimmed.endsWith('{') ||
        trimmed.endsWith('}') ||
        trimmed.endsWith(':')
      ) {
        return;
      }
      diags.push({ level: 'error', message: `C: point-virgule manquant ligne ${idx + 1}.` });
    });

    const printfHasNewline = /printf\s*\(\s*"[^"]*\\n[^"]*"\s*\)/.test(src);
    if (usesPrintf && !printfHasNewline) {
      diags.push({
        level: 'error',
        message: 'C: pensez a terminer printf par \\n.',
        line: this.lineOf(src, /\bprintf\s*\(/)
      });
    }

    if (usesMalloc && !usesFree) {
      diags.push({ level: 'error', message: 'C: malloc detecte sans free (fuite memoire).' });
    }

    if (hasMain && !/\breturn\s+0\s*;/.test(src)) {
      diags.push({ level: 'error', message: 'C: ajoutez "return 0;" a la fin de main.' });
    }

    if (/\bgets\s*\(/.test(src)) {
      diags.push({ level: 'error', message: 'C: gets() est dangereux, utiliser fgets().' });
    }
    if (usesScanf && !/if\s*\(\s*scanf\s*\(/.test(src)) {
      diags.push({ level: 'error', message: 'C: verifier le retour de scanf().' });
    }
    if (/\bstrcpy\s*\(/.test(src)) {
      diags.push({ level: 'error', message: 'C: strcpy() -> preferer strncpy() ou memcpy().' });
    }
    if (
      /\bint\s+main\s*\(\s*void\s*\)/.test(src) &&
      /printf\s*\(/.test(src) &&
      !/fflush\s*\(\s*stdout\s*\)/.test(src)
    ) {
      diags.push({
        level: 'error',
        message: 'C: ajoutez fflush(stdout) si besoin d output immediat.',
        line: this.lineOf(src, /\bprintf\s*\(/)
      });
    }
    if (/printf\s*\(\s*"[^"]*%s[^"]*"\s*,\s*[^)]*\)/.test(src) && !/char\s+\w+\s*\[/.test(src)) {
      diags.push({ level: 'error', message: 'C: verifiez buffer pour %s (taille + terminaison).' });
    }

    return diags;
  }

  private analyzeSteelconf(src: string): Diagnostic[] {
    const diags: Diagnostic[] = [];
    const meaningful = src
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith(';;'));
    if (meaningful.length > 0 && meaningful[0] !== '!muf 4') {
      diags.push({ level: 'error', message: 'steelconf: !muf 4 doit etre la premiere ligne utile.' });
    }
    const lines = src.split('\n').map((l) => l.trim());
    const firstMeaningful = lines.find((l) => l && !l.startsWith(';;'));
    if (firstMeaningful !== '!muf 4') {
      diags.push({ level: 'error', message: 'steelconf: en-tete !muf 4 manquant.' });
    }

    const workspaceBlocks = [...src.matchAll(/\[workspace\]([\s\S]*?)\.\./g)];
    const hasWorkspace = workspaceBlocks.length > 0;
    const toolNames = [...src.matchAll(/\[tool\s+([A-Za-z0-9_-]+)\]/g)].map((m) => m[1]);
    const bakeMatches = [...src.matchAll(/\[bake\s+([A-Za-z0-9_-]+)\]/g)];
    const bakeNames = bakeMatches.map((m) => m[1]);
    const runTools = [...src.matchAll(/\[run\s+([A-Za-z0-9_-]+)\]/g)].map((m) => m[1]);
    const hasRun = runTools.length > 0;
    const hasExec = /\.exec\s+/.test(src);
    const hasMake = /\.make\s+/.test(src);
    const hasOutput = /\.output\s+/.test(src);
    const hasTakes = /\.takes\s+/.test(src);
    const hasEmits = /\.emits\s+/.test(src);

    if (!hasWorkspace) diags.push({ level: 'error', message: 'steelconf: bloc [workspace] manquant.' });
    if (workspaceBlocks.length > 1) {
      diags.push({ level: 'error', message: 'steelconf: plusieurs blocs [workspace].' });
    }
    if (toolNames.length === 0) diags.push({ level: 'error', message: 'steelconf: bloc [tool <name>] manquant.' });
    if (bakeNames.length === 0) diags.push({ level: 'error', message: 'steelconf: bloc [bake <name>] manquant.' });
    if (!hasRun) diags.push({ level: 'error', message: 'steelconf: bloc [run <tool>] manquant.' });
    if (!hasExec) diags.push({ level: 'error', message: 'steelconf: .exec manquant dans tool.' });
    if (!hasMake) diags.push({ level: 'error', message: 'steelconf: .make manquant dans bake.' });
    if (!hasOutput) {
      const line = bakeMatches.length ? this.lineOf(src, /\[bake\s+[A-Za-z0-9_-]+\]/) : undefined;
      diags.push({ level: 'error', message: 'steelconf: .output manquant dans bake.', line });
    }
    if (!hasTakes) diags.push({ level: 'error', message: 'steelconf: .takes manquant dans run.' });
    if (!hasEmits) diags.push({ level: 'error', message: 'steelconf: .emits manquant dans run.' });

    const workspaceKeys = ['name', 'root', 'target_dir', 'profile'];
    workspaceKeys.forEach((key) => {
      if (!new RegExp(`\\.set\\s+${key}\\s+`).test(src)) {
        diags.push({ level: 'error', message: `steelconf: .set ${key} manquant dans workspace.` });
      }
    });
    workspaceBlocks.forEach((blk) => {
      if (!/\.set\s+name\s+/.test(blk[0])) {
        diags.push({ level: 'error', message: 'steelconf: workspace sans .set name.' });
      }
      if (!/\.set\s+root\s+/.test(blk[0])) {
        diags.push({ level: 'error', message: 'steelconf: workspace sans .set root.' });
      }
      if (!/\.set\s+target_dir\s+/.test(blk[0])) {
        diags.push({ level: 'error', message: 'steelconf: workspace sans .set target_dir.' });
      }
      const rootVal = blk[0].match(/\.set\s+root\s+"([^"]+)"/);
      if (rootVal && rootVal[1] !== '.') {
        diags.push({ level: 'error', message: 'steelconf: .set root "." recommande.' });
      }
      const targetVal = blk[0].match(/\.set\s+target_dir\s+"([^"]+)"/);
      if (targetVal && targetVal[1] !== 'target') {
        diags.push({ level: 'error', message: 'steelconf: .set target_dir "target" recommande.' });
      }
      if (targetVal && targetVal[1].startsWith('/')) {
        diags.push({ level: 'error', message: 'steelconf: target_dir doit rester relatif.' });
      }
    });

    const dupTools = toolNames.filter((t, i, a) => a.indexOf(t) !== i);
    dupTools.forEach((t) => {
      diags.push({ level: 'error', message: `steelconf: tool duplique "${t}".` });
    });
    const dupBakes = bakeNames.filter((b, i, a) => a.indexOf(b) !== i);
    dupBakes.forEach((b) => {
      diags.push({ level: 'error', message: `steelconf: bake duplique "${b}".` });
    });

    const toolUsage = new Map<string, number>();
    runTools.forEach((tool) => toolUsage.set(tool, (toolUsage.get(tool) || 0) + 1));
    toolNames.forEach((tool) => {
      if (!toolUsage.has(tool)) {
        diags.push({ level: 'error', message: `steelconf: tool "${tool}" jamais utilise.` });
      }
    });

    const bakeRefs = new Map<string, number>();
    const exportRefs = [...src.matchAll(/\.ref\s+([A-Za-z0-9_-]+)/g)].map((m) => m[1]);
    const needsRefs = [...src.matchAll(/\.needs\s+([A-Za-z0-9_-]+)/g)].map((m) => m[1]);
    exportRefs.concat(needsRefs).forEach((b) => bakeRefs.set(b, (bakeRefs.get(b) || 0) + 1));
    bakeNames.forEach((b) => {
      if (!bakeRefs.has(b)) {
        const line = this.lineOf(src, new RegExp(`\\[bake\\s+${b}\\]`));
        diags.push({
          level: 'error',
          message: `steelconf: bake "${b}" non reference (export/needs).`,
          line
        });
      }
    });

    runTools.forEach((tool) => {
      if (!toolNames.includes(tool)) {
        diags.push({ level: 'error', message: `steelconf: run utilise tool "${tool}" non declare.` });
      }
    });

    const outputMatches = [...src.matchAll(/\.output\s+([A-Za-z0-9_-]+)\s+"([^"]+)"/g)];
    const outputPorts = outputMatches.map((m) => m[1]);
    const outputPaths = outputMatches.map((m) => m[2]);
    outputPaths.forEach((path) => {
      if (!path.startsWith('target/out/')) {
        diags.push({ level: 'error', message: `steelconf: sortie "${path}" -> preferez target/out/.` });
      }
    });

    const makeIds = [...src.matchAll(/\.make\s+([A-Za-z0-9_-]+)\s+/g)].map((m) => m[1]);
    const takesIds = [...src.matchAll(/\.takes\s+([A-Za-z0-9_-]+)\s+as\s+/g)].map((m) => m[1]);
    takesIds.forEach((id) => {
      if (!makeIds.includes(id)) {
        diags.push({ level: 'error', message: `steelconf: .takes "${id}" ne correspond a aucun .make.` });
      }
    });

    const emitsPorts = [...src.matchAll(/\.emits\s+([A-Za-z0-9_-]+)\s+as\s+/g)].map((m) => m[1]);
    emitsPorts.forEach((port) => {
      if (!outputPorts.includes(port)) {
        diags.push({ level: 'error', message: `steelconf: .emits "${port}" ne correspond a aucun .output.` });
      }
    });

    if (outputPorts.length > 0 && emitsPorts.length === 0) {
      diags.push({ level: 'error', message: 'steelconf: outputs declares sans .emits.' });
    }

    if (exportRefs.length === 0) {
      diags.push({
        level: 'error',
        message: 'steelconf: aucune recette exportee (bloc [export]).',
        line: this.lineOf(src, /!muf\s+4/)
      });
    }
    exportRefs.forEach((b) => {
      if (!bakeNames.includes(b)) {
        diags.push({ level: 'error', message: `steelconf: .ref "${b}" ne correspond a aucun bake.` });
      }
    });

    const execLines = [...src.matchAll(/^\s*\.exec\s+"?([^"\n]+)"?/gm)].map((m) => m[1].trim());
    execLines.forEach((cmd) => {
      if (!cmd) {
        diags.push({ level: 'error', message: 'steelconf: .exec vide.' });
      }
      if (cmd.includes(' ')) {
        diags.push({ level: 'error', message: `steelconf: .exec "${cmd}" contient des espaces (preferer un binaire seul).` });
      }
      if (cmd.startsWith('./') || cmd.startsWith('../')) {
        diags.push({ level: 'error', message: `steelconf: .exec "${cmd}" est relatif (preferer un binaire du PATH).` });
      }
    });

    const makeKinds = [...src.matchAll(/\.make\s+\w+\s+([A-Za-z0-9_-]+)/g)].map((m) => m[1]);
    makeKinds.forEach((kind) => {
      if (!['cglob', 'glob', 'file', 'list'].includes(kind)) {
        diags.push({ level: 'error', message: `steelconf: kind .make "${kind}" inconnu.` });
      }
    });

    const makePatterns = [...src.matchAll(/\.make\s+\w+\s+[A-Za-z0-9_-]+\s+"([^"]*)"/g)].map((m) => m[1]);
    makePatterns.forEach((pat) => {
      if (!pat) {
        diags.push({ level: 'error', message: 'steelconf: .make avec pattern vide.' });
      }
    });
    const makeNoQuote = [...src.matchAll(/\.make\s+\w+\s+[A-Za-z0-9_-]+\s+([^\s"].*)/g)].map((m) => m[1]);
    if (makeNoQuote.length > 0) {
      diags.push({ level: 'error', message: 'steelconf: .make patterns devraient etre entre guillemets.' });
    }

    const takesFlags = [...src.matchAll(/\.takes\s+[A-Za-z0-9_-]+\s+as\s+("([^"]+)"|([^\s]+))/g)].map(
      (m) => m[2] || m[3]
    );
    takesFlags.forEach((flag) => {
      if (!flag) return;
      if (!flag.startsWith('-') && flag !== '@args') {
        diags.push({ level: 'error', message: `steelconf: .takes flag "${flag}" devrait commencer par "-" ou @args.` });
      }
    });

    const emitsFlags = [...src.matchAll(/\.emits\s+[A-Za-z0-9_-]+\s+as\s+("([^"]+)"|([^\s]+))/g)].map(
      (m) => m[2] || m[3]
    );
    emitsFlags.forEach((flag) => {
      if (!flag) return;
      if (!flag.startsWith('-')) {
        diags.push({ level: 'error', message: `steelconf: .emits flag "${flag}" devrait commencer par "-".` });
      }
    });

    if (!/^\s*!muf\s+4\s*$/m.test(src)) {
      diags.push({ level: 'error', message: 'steelconf: ligne header invalide (attendu !muf 4).'});
    }

    const blockOpens = (src.match(/^\s*\[/gm) || []).length;
    const blockCloses = (src.match(/^\s*\.\.\s*$/gm) || []).length;
    if (blockOpens !== blockCloses) {
      diags.push({ level: 'error', message: 'steelconf: blocs non equilibres (.. manquant).' });
    }

    if (!/\.set\s+profile\s+/.test(src)) {
      diags.push({ level: 'error', message: 'steelconf: ajoutez .set profile "debug".' });
    }
    if (!/^\s*!muf\s+4\s*$/m.test(src)) {
      diags.push({ level: 'error', message: 'steelconf: ligne header invalide (attendu !muf 4).' });
    }

    const profileNames = [...src.matchAll(/\[profile\s+([A-Za-z0-9_-]+)\]/g)].map((m) => m[1]);
    const workspaceProfile = src.match(/\.set\s+profile\s+"?([A-Za-z0-9_-]+)"?/);
    if (workspaceProfile && !profileNames.includes(workspaceProfile[1])) {
      diags.push({
        level: 'error',
        message: `steelconf: profile "${workspaceProfile[1]}" non declare dans [profile].`,
        line: this.lineOf(src, /\.set\s+profile\s+/)
      });
    }

    if (/\t/.test(src)) {
      diags.push({ level: 'error', message: 'steelconf: tabulations interdites (utiliser des espaces).' });
    }

    const needs = needsRefs;
    needs.forEach((dep) => {
      if (!bakeNames.includes(dep)) {
        diags.push({ level: 'error', message: `steelconf: .needs "${dep}" ne correspond a aucun bake.` });
      }
    });

    const runBlocksPerBake = [...src.matchAll(/\[bake\s+([A-Za-z0-9_-]+)\]([\s\S]*?)^\.\.\s*$/gm)];
    runBlocksPerBake.forEach((match) => {
      const blk = match[0];
      const name = match[1];
      if (!/\[run\s+/.test(blk)) {
        diags.push({ level: 'error', message: 'steelconf: bake sans bloc [run <tool>].' });
      }
      if (!/\.output\s+/.test(blk)) {
        const line = this.lineOf(src, new RegExp(`\\[bake\\s+${name}\\]`));
        diags.push({ level: 'error', message: 'steelconf: bake sans .output.', line });
      }
      if (!/\.make\s+/.test(blk)) {
        diags.push({ level: 'error', message: 'steelconf: bake sans .make.' });
      }
      const outputCount = (blk.match(/\.output\s+/g) || []).length;
      if (outputCount > 1) {
        diags.push({ level: 'error', message: 'steelconf: un seul .output par bake (recommande).' });
      }
    });

    const outputLines = [...src.matchAll(/\.output\s+[A-Za-z0-9_-]+\s+"([^"]+)"/g)].map((m) => m[1]);
    outputLines.forEach((p) => {
      if (/\s/.test(p)) {
        diags.push({ level: 'error', message: `steelconf: evitez les espaces dans "${p}".` });
      }
      if (!p.startsWith('target/out/')) {
        diags.push({ level: 'error', message: `steelconf: sortie "${p}" doit etre sous target/out/.` });
      }
      if (!/\.[a-zA-Z0-9]+$/.test(p)) {
        diags.push({
          level: 'error',
          message: `steelconf: sortie "${p}" sans extension.`,
          line: this.lineOf(
            src,
            new RegExp(`\\.output\\s+\\w+\\s+"${p.replace(/[-/\\\\.^$*+?()[\\]{}|]/g, '\\\\$&')}"`)
          )
        });
      }
    });

    const outputPathsSeen = new Set<string>();
    outputLines.forEach((p) => {
      if (outputPathsSeen.has(p)) {
        diags.push({ level: 'error', message: `steelconf: sortie dupliquee "${p}".` });
      }
      outputPathsSeen.add(p);
    });

    const toolBlocks = [...src.matchAll(/\[tool\s+([A-Za-z0-9_-]+)\]([\s\S]*?)\.\./g)];
    toolBlocks.forEach((match) => {
      const name = match[1];
      const body = match[2];
      if (!/\.exec\s+/.test(body)) {
        diags.push({ level: 'error', message: `steelconf: tool "${name}" sans .exec.` });
      }
      const execVal = body.match(/\.exec\s+"([^"]+)"/);
      if (execVal && execVal[1]) {
        const execBase = execVal[1].split('/').pop() || execVal[1];
        if (execBase && execBase !== name) {
          diags.push({
            level: 'error',
            message: `steelconf: tool "${name}" -> nommer comme le binaire "${execBase}".`
          });
        }
      }
      const allowedToolKeys = ['exec'];
      const toolKeys = [...body.matchAll(/\.([a-zA-Z_][a-zA-Z0-9_-]*)\s+/g)].map((m) => m[1]);
      toolKeys.forEach((k) => {
        if (!allowedToolKeys.includes(k)) {
          diags.push({ level: 'error', message: `steelconf: tool "${name}" contient .${k} inconnu.` });
        }
      });
    });

    const runBlocks = [...src.matchAll(/\[run\s+([A-Za-z0-9_-]+)\]([\s\S]*?)\.\./g)];
    runBlocks.forEach((match) => {
      const tool = match[1];
      const body = match[2];
      if (!toolNames.includes(tool)) {
        diags.push({ level: 'error', message: `steelconf: run "${tool}" pointe vers un tool inconnu.` });
      }
      if (!/\.takes\s+/.test(body) && !/\.emits\s+/.test(body)) {
        diags.push({ level: 'error', message: `steelconf: run "${tool}" sans .takes/.emits.` });
      }
      if (/\.takes\s+/.test(body) && !/\.emits\s+/.test(body)) {
        diags.push({ level: 'error', message: `steelconf: run "${tool}" a .takes sans .emits.` });
      }
      if (!/\.set\s+/.test(body)) {
        diags.push({ level: 'error', message: `steelconf: run "${tool}" sans .set (flags).` });
      }
      const allowedRunKeys = ['takes', 'emits', 'set', 'include', 'define', 'libdir', 'lib'];
      const runKeys = [...body.matchAll(/\.([a-zA-Z_][a-zA-Z0-9_-]*)\s+/g)].map((m) => m[1]);
      runKeys.forEach((k) => {
        if (!allowedRunKeys.includes(k)) {
          diags.push({ level: 'error', message: `steelconf: run "${tool}" contient .${k} inconnu.` });
        }
      });
    });

    const bakeBlocks = [...src.matchAll(/\[bake\s+([A-Za-z0-9_-]+)\]([\s\S]*?)^\.\.\s*$/gm)];
    bakeBlocks.forEach((match) => {
      const name = match[1];
      const body = match[2];
      if (!/^[a-z0-9_]+$/.test(name)) {
        diags.push({ level: 'error', message: `steelconf: bake "${name}" -> preferer snake_case.` });
      }
      const allowedBakeKeys = ['make', 'needs', 'output'];
      const bakeKeys = [...body.matchAll(/\.([a-zA-Z_][a-zA-Z0-9_-]*)\s+/g)].map((m) => m[1]);
      bakeKeys.forEach((k) => {
        if (!allowedBakeKeys.includes(k)) {
          if (k === 'takes' || k === 'set' || k === 'emits') {
            return;
          }
          diags.push({ level: 'error', message: `steelconf: bake "${name}" contient .${k} inconnu.` });
        }
      });
      const makeIds = [...body.matchAll(/\.make\s+([A-Za-z0-9_-]+)\s+/g)].map((m) => m[1]);
      const dupMakeIds = makeIds.filter((id, i, a) => a.indexOf(id) !== i);
      dupMakeIds.forEach((id) => {
        diags.push({ level: 'error', message: `steelconf: bake "${name}" .make duplique "${id}".` });
      });
    });

    const unknownBlocks = [...src.matchAll(/^\s*\[([A-Za-z0-9_-]+)(?:\s+[^\]]+)?\]/gm)]
      .map((m) => m[1])
      .filter((tag) => !['workspace', 'profile', 'tool', 'bake', 'run', 'export'].includes(tag));
    unknownBlocks.forEach((tag) => {
      diags.push({ level: 'error', message: `steelconf: bloc [${tag}] inconnu.` });
    });

    return diags;
  }

  private classify(message: string): DiagnosticLevel {
    if (message.startsWith('C:')) {
      if (
        message.includes('main manquante') ||
        message.includes('fichier vide') ||
        message.includes('accolades non equilibrees')
      ) {
        return 'error';
      }
      return 'warning';
    }

    if (message.startsWith('steelconf:')) {
      const errorSignals = [
        'bloc [workspace] manquant',
        'bloc [tool',
        'bloc [bake',
        'bloc [run',
        '.exec manquant',
        'ligne header invalide',
        '!muf 4 doit etre la premiere',
        'tabulations interdites',
        'tool duplique',
        'bake duplique',
        'run utilise tool',
        'bake sans bloc [run',
        'tool "',
        'run "',
        'profile "'
      ];
      if (message.includes('doit etre sous target/out/')) {
        return 'error';
      }
      if (message.includes('sans extension')) {
        return 'error';
      }
      if (errorSignals.some((s) => message.includes(s))) {
        return 'error';
      }
      if (
        message.includes('recommande') ||
        message.includes('preferez') ||
        message.includes('preferer') ||
        message.includes('sans extension') ||
        message.includes('snake_case')
      ) {
        return 'suggestion';
      }
      return 'warning';
    }

    return 'warning';
  }
}
