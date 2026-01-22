import { CommonModule } from '@angular/common';
import {
  AfterViewInit,
  Component,
  ElementRef,
  effect,
  OnDestroy,
  OnInit,
  ViewChild
} from '@angular/core';

import { AppState } from './app.state';
import { SeoService } from './seo.service';

type BuildLogLevel = 'info' | 'warn' | 'error' | 'debug' | 'success';

type BuildIssue = {
  level: 'error' | 'warn';
  file: string;
  line: number;
  column: number;
  message: string;
};

type BuildArtifact = {
  path: string;
  status: 'built' | 'skipped';
};

type CFile = {
  name: string;
  path: string;
  initial: string;
  model?: any;
};

declare global {
  interface Window {
    require?: any;
    MonacoEnvironment?: any;
    __monacoLoaded?: boolean;
  }
}

@Component({
  selector: 'app-docs',
  imports: [CommonModule],
  templateUrl: './docs.component.html'
})
export class DocsComponent implements OnInit, AfterViewInit, OnDestroy {
  @ViewChild('cEditorHost', { static: false }) cEditorHost?: ElementRef<HTMLDivElement>;
  @ViewChild('steelEditorHost', { static: false }) steelEditorHost?: ElementRef<HTMLDivElement>;

  cFiles: CFile[] = [
    {
      name: 'main.c',
      path: 'src/main.c',
      initial: `#include <stdio.h>

int main(void) {
  printf("Steel says hello!\\n");
  return 0;
}`
    },
    {
      name: 'utils.c',
      path: 'src/utils.c',
      initial: `#include <stddef.h>

int add(int a, int b) {
  return a + b;
}`
    }
  ];
  activeCFile = 0;
  steelConfCode = `[tool cc]
\t.exec "cc"
..

[bake c_build]
\t.make c_src cglob "src/**/*.c"
\t[run cc]
\t\t.set "-O2" 1
\t\t.set "-g" 1
\t\t.takes c_src as "@args"
\t\t.emits exe as "-o"
\t..
\t.output exe "target/out/app_c"
..`;

  buildLogs: { level: BuildLogLevel; text: string }[] = [];
  buildIssues: BuildIssue[] = [];
  buildArtifacts: BuildArtifact[] = [];
  buildResult = '';
  issueFilter: 'all' | 'error' | 'warn' = 'all';

  private buildToggle = false;
  private monaco: any;
  private cEditor: any;
  private steelEditor: any;

  constructor(
    public state: AppState,
    private seo: SeoService
  ) {
    effect(() => {
      const t = this.state.t();
      const title = this.seo.titleWithSite(t.docs.title);
      const description = t.docs.lead;
      this.seo.update({ title, description, ogType: 'website' });
      this.seo.setJsonLd('app-jsonld', null);
      this.seo.setJsonLd('blog-jsonld', null);
    });
  }

  get t() {
    return this.state.t();
  }

  get examples() {
    return this.state.examples;
  }

  ngOnInit(): void {
    this.buildArtifacts = [
      { path: 'src/main.c', status: 'built' },
      { path: 'src/utils.c', status: 'built' },
      { path: 'src/steelconf.mff', status: 'skipped' }
    ];
  }

  async ngAfterViewInit(): Promise<void> {
    await this.loadMonaco();
    if (!this.monaco || !this.cEditorHost || !this.steelEditorHost) {
      return;
    }

    this.registerSteelconfLanguage();
    this.registerCompletions();

    this.cFiles.forEach((file) => {
      file.model = this.monaco.editor.createModel(
        file.initial,
        'c',
        this.monaco.Uri.parse(`file:///${file.path}`)
      );
    });

    this.cEditor = this.monaco.editor.create(this.cEditorHost.nativeElement, {
      model: this.cFiles[this.activeCFile].model,
      language: 'c',
      theme: 'steel-light',
      minimap: { enabled: false },
      fontFamily: '"IBM Plex Mono", monospace',
      fontSize: 14,
      lineHeight: 20,
      automaticLayout: true
    });

    this.steelEditor = this.monaco.editor.create(this.steelEditorHost.nativeElement, {
      value: this.steelConfCode,
      language: 'steelconf',
      theme: 'steel-light',
      minimap: { enabled: false },
      fontFamily: '"IBM Plex Mono", monospace',
      fontSize: 14,
      lineHeight: 20,
      automaticLayout: true
    });

    this.cEditor.onDidChangeModelContent(() => {
      const model = this.cEditor.getModel();
      const file = this.cFiles.find((item) => item.model === model);
      if (file) {
        file.initial = model.getValue();
      }
    });
    this.steelEditor.onDidChangeModelContent(() => {
      this.steelConfCode = this.steelEditor.getValue();
    });
  }

  ngOnDestroy(): void {
    this.cFiles.forEach((file) => {
      if (file.model) {
        file.model.dispose();
      }
    });
    if (this.cEditor) {
      this.cEditor.dispose();
    }
    if (this.steelEditor) {
      this.steelEditor.dispose();
    }
  }

  runBuild(): void {
    this.buildToggle = !this.buildToggle;
    this.issueFilter = 'all';
    const diagnostics = this.getBuildDiagnostics();
    if (this.buildToggle) {
      const errorCount = diagnostics.issues.filter((issue) => issue.level === 'error').length;
      this.buildLogs = [
        { level: 'info', text: 'steel build: parsing steelconf...' },
        { level: 'debug', text: 'cc -O2 -g src/main.c src/utils.c -o target/out/app_c' },
        ...diagnostics.logs,
        { level: 'info', text: `build failed with ${errorCount} error(s)` }
      ];
      this.buildIssues = diagnostics.issues;
      this.buildArtifacts = [
        { path: 'target/out/app_c', status: 'skipped' },
        { path: 'target/out/app_c.dSYM', status: 'skipped' }
      ];
      this.buildResult = 'Build failed. Fix errors and try again.';
      this.applyMarkers(diagnostics.issues);
    } else {
      this.buildLogs = [
        { level: 'info', text: 'steel build: parsing steelconf...' },
        { level: 'debug', text: 'cc -O2 -g src/main.c src/utils.c -o target/out/app_c' },
        { level: 'info', text: 'linking target/out/app_c' },
        { level: 'success', text: 'build succeeded in 0.9s' }
      ];
      this.buildIssues = [];
      this.buildArtifacts = [
        { path: 'target/out/app_c', status: 'built' },
        { path: 'target/out/app_c.dSYM', status: 'built' }
      ];
      this.buildResult = 'Result: Steel says hello!';
      this.applyMarkers([]);
    }
  }

  setActiveCFile(index: number): void {
    if (!this.cEditor || !this.cFiles[index]) {
      return;
    }
    this.activeCFile = index;
    this.cEditor.setModel(this.cFiles[index].model);
  }

  jumpToIssue(issue: BuildIssue): void {
    if (!this.monaco) {
      return;
    }
    if (issue.file.endsWith('steelconf.mff')) {
      if (this.steelEditor) {
        this.steelEditor.revealPositionInCenter({ lineNumber: issue.line, column: issue.column });
        this.steelEditor.setPosition({ lineNumber: issue.line, column: issue.column });
        this.steelEditor.focus();
      }
      return;
    }
    const index = this.cFiles.findIndex((file) => file.path === issue.file);
    if (index !== -1) {
      this.setActiveCFile(index);
      if (this.cEditor) {
        this.cEditor.revealPositionInCenter({ lineNumber: issue.line, column: issue.column });
        this.cEditor.setPosition({ lineNumber: issue.line, column: issue.column });
        this.cEditor.focus();
      }
    }
  }

  get filteredIssues(): BuildIssue[] {
    if (this.issueFilter === 'all') {
      return this.buildIssues;
    }
    return this.buildIssues.filter((issue) => issue.level === this.issueFilter);
  }

  setIssueFilter(filter: 'all' | 'error' | 'warn'): void {
    this.issueFilter = filter;
  }

  private async loadMonaco(): Promise<void> {
    if (window.__monacoLoaded) {
      this.monaco = (window as any).monaco;
      return;
    }

    await new Promise<void>((resolve) => {
      const existing = document.querySelector('script[data-monaco-loader]');
      if (existing) {
        existing.addEventListener('load', () => resolve());
        resolve();
        return;
      }
      const script = document.createElement('script');
      script.setAttribute('data-monaco-loader', 'true');
      script.src = 'https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.52.0/min/vs/loader.min.js';
      script.onload = () => resolve();
      document.body.appendChild(script);
    });

    await new Promise<void>((resolve) => {
      if (!window.require) {
        resolve();
        return;
      }
      window.require.config({
        paths: {
          vs: 'https://cdnjs.cloudflare.com/ajax/libs/monaco-editor/0.52.0/min/vs'
        }
      });
      window.require(['vs/editor/editor.main'], () => {
        window.__monacoLoaded = true;
        this.monaco = (window as any).monaco;
        this.defineTheme();
        resolve();
      });
    });
  }

  private defineTheme(): void {
    if (!this.monaco) {
      return;
    }
    this.monaco.editor.defineTheme('steel-light', {
      base: 'vs',
      inherit: true,
      rules: [
        { token: 'comment', foreground: '7a6d7f' },
        { token: 'keyword', foreground: 'b1202d', fontStyle: 'bold' },
        { token: 'string', foreground: '1f7a44' },
        { token: 'number', foreground: '1f6fb2' },
        { token: 'type', foreground: '7a2bd9' },
        { token: 'preproc', foreground: 'b36b00', fontStyle: 'bold' }
      ],
      colors: {
        'editor.background': '#fbf7f3',
        'editorLineNumber.foreground': '#a89aa8',
        'editorLineNumber.activeForeground': '#3a0a18'
      }
    });
  }

  private registerSteelconfLanguage(): void {
    if (!this.monaco) {
      return;
    }
    this.monaco.languages.register({ id: 'steelconf' });
    this.monaco.languages.setMonarchTokensProvider('steelconf', {
      tokenizer: {
        root: [
          [/\[[^\]]+\]/, 'keyword'],
          [/"(?:[^"\\]|\\.)*"/, 'string'],
          [/\b\d+\b/, 'number'],
          [/\b(tool|bake|run|make|set|emits|takes|output|exec|workspace|profile|target_dir|root|cglob)\b/, 'keyword'],
          [/\.{2}/, 'delimiter'],
          [/#.*$/, 'comment']
        ]
      }
    });
  }

  private registerCompletions(): void {
    if (!this.monaco) {
      return;
    }
    this.monaco.languages.registerCompletionItemProvider('c', {
      provideCompletionItems: () => ({
        suggestions: [
          {
            label: 'for',
            kind: this.monaco.languages.CompletionItemKind.Snippet,
            insertText: 'for (int i = 0; i < ${1:count}; i++) {\n\t$0\n}',
            insertTextRules: this.monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
          },
          {
            label: 'if',
            kind: this.monaco.languages.CompletionItemKind.Snippet,
            insertText: 'if (${1:condition}) {\n\t$0\n}',
            insertTextRules: this.monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
          },
          {
            label: 'printf',
            kind: this.monaco.languages.CompletionItemKind.Function,
            insertText: 'printf("${1:message}\\n"${2:});'
          }
        ]
      })
    });

    this.monaco.languages.registerCompletionItemProvider('steelconf', {
      provideCompletionItems: () => ({
        suggestions: [
          {
            label: 'tool',
            kind: this.monaco.languages.CompletionItemKind.Snippet,
            insertText: '[tool cc]\n\t.exec "cc"\n..',
            insertTextRules: this.monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
          },
          {
            label: 'bake',
            kind: this.monaco.languages.CompletionItemKind.Snippet,
            insertText:
              '[bake build]\n\t.make src cglob "src/**/*.c"\n\t[run cc]\n\t\t.takes src as "@args"\n\t\t.emits exe as "-o"\n\t..\n\t.output exe "target/out/app"\n..',
            insertTextRules: this.monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
          },
          {
            label: 'workspace',
            kind: this.monaco.languages.CompletionItemKind.Snippet,
            insertText:
              '[workspace]\n\t.set name "demo"\n\t.set root "."\n\t.set target_dir "target"\n\t.set profile "debug"\n..',
            insertTextRules: this.monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet
          }
        ]
      })
    });
  }

  private getBuildDiagnostics(): {
    issues: BuildIssue[];
    logs: { level: BuildLogLevel; text: string }[];
  } {
    const issues: BuildIssue[] = [];
    const logs: { level: BuildLogLevel; text: string }[] = [];
    const mainCode = this.getCFileValue('src/main.c');
    const utilsCode = this.getCFileValue('src/utils.c');
    const steelCode = this.steelConfCode;

    const printfLine = this.findLine(mainCode, /printf\(/);
    if (printfLine && !/printf\([^)]*\);/.test(this.getLine(mainCode, printfLine))) {
      issues.push({
        level: 'error',
        file: 'src/main.c',
        line: printfLine,
        column: 3,
        message: 'expected ";" after printf call'
      });
    }

    if (!/return\s+0\s*;/.test(mainCode)) {
      issues.push({
        level: 'error',
        file: 'src/main.c',
        line: 3,
        column: 3,
        message: 'missing return 0; in main'
      });
    }

    if (/buffer/.test(utilsCode) && !/\buse\b/.test(utilsCode)) {
      issues.push({
        level: 'warn',
        file: 'src/utils.c',
        line: this.findLine(utilsCode, /buffer/) || 1,
        column: 5,
        message: 'unused variable `buffer`'
      });
    }

    if (!/\[tool\s+cc\]/.test(steelCode)) {
      issues.push({
        level: 'error',
        file: 'src/steelconf.mff',
        line: 1,
        column: 1,
        message: 'missing [tool cc] block'
      });
    }

    if (!/\[bake\s+.+\]/.test(steelCode)) {
      issues.push({
        level: 'warn',
        file: 'src/steelconf.mff',
        line: 1,
        column: 1,
        message: 'no [bake] block found'
      });
    }

    issues.forEach((issue) => {
      logs.push({
        level: issue.level === 'error' ? 'error' : 'warn',
        text: `${issue.level}: ${issue.message} in ${issue.file}:${issue.line}:${issue.column}`
      });
    });

    return { issues, logs };
  }

  private applyMarkers(issues: BuildIssue[]): void {
    if (!this.monaco) {
      return;
    }
    this.cFiles.forEach((file) => {
      const markers = issues
        .filter((issue) => issue.file === file.path)
        .map((issue) => ({
          severity:
            issue.level === 'error'
              ? this.monaco.MarkerSeverity.Error
              : this.monaco.MarkerSeverity.Warning,
          message: issue.message,
          startLineNumber: issue.line,
          startColumn: issue.column,
          endLineNumber: issue.line,
          endColumn: issue.column + 1
        }));
      this.monaco.editor.setModelMarkers(file.model, 'steel-build', markers);
    });

    const steelMarkers = issues
      .filter((issue) => issue.file.endsWith('steelconf.mff'))
      .map((issue) => ({
        severity:
          issue.level === 'error'
            ? this.monaco.MarkerSeverity.Error
            : this.monaco.MarkerSeverity.Warning,
        message: issue.message,
        startLineNumber: issue.line,
        startColumn: issue.column,
        endLineNumber: issue.line,
        endColumn: issue.column + 1
      }));
    if (this.steelEditor) {
      this.monaco.editor.setModelMarkers(this.steelEditor.getModel(), 'steel-build', steelMarkers);
    }
  }

  private getCFileValue(path: string): string {
    const file = this.cFiles.find((item) => item.path === path);
    if (!file || !file.model) {
      return '';
    }
    return file.model.getValue();
  }

  private findLine(content: string, pattern: RegExp): number | null {
    const lines = content.split('\n');
    for (let i = 0; i < lines.length; i += 1) {
      if (pattern.test(lines[i])) {
        return i + 1;
      }
    }
    return null;
  }

  private getLine(content: string, line: number): string {
    const lines = content.split('\n');
    return lines[line - 1] || '';
  }
}
