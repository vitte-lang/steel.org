import {
  CompletionItem,
  CompletionItemKind,
  CodeAction,
  CodeActionKind,
  Diagnostic,
  DiagnosticSeverity,
  createConnection,
  InitializeParams,
  InitializeResult,
  ProposedFeatures,
  TextDocuments,
  TextDocumentSyncKind,
  CompletionParams,
  CodeActionParams,
  HoverParams,
} from "vscode-languageserver/node";
import { TextDocument } from "vscode-languageserver-textdocument";
import * as fs from "fs";
import * as path from "path";

const connection = createConnection(ProposedFeatures.all);
const documents: TextDocuments<TextDocument> = new TextDocuments(TextDocument);

const TOP_LEVEL_BLOCKS = ["workspace", "profile", "tool", "bake", "run", "export"];
const DIRECTIVES = [".set", ".exec", ".make", ".output", ".ref", ".needs", ".takes", ".emits"];
const OUTPUT_PORTS = ["exe", "lib", "obj", "out"];
const MAKE_KINDS = ["glob", "cglob"];
const WORKSPACE_KEYS = ["name", "root", "target_dir", "profile"];
const PROFILE_KEYS = ["opt", "debug"];
const PROFILE_VALUES: Record<string, string[]> = {
  opt: ["0", "1", "2", "3"],
  debug: ["0", "1"],
};
const WORKSPACE_VALUES: Record<string, string[]> = {
  profile: ["debug", "release"],
};
const TOOL_FLAGS = [
  "-O0",
  "-O1",
  "-O2",
  "-O3",
  "-g",
  "-Wall",
  "-Wextra",
  "-std=c17",
  "-std=c++17",
  "-I",
  "-D",
  "-L",
  "-l",
  "-u",
  "-m",
];
const VARIABLE_HINTS = ["${root}", "${target_dir}", "${profile}", "${name}"];
const OUTPUT_PREFIXES = ["target/out", "dist", "build", "bin"];
const MAX_WORKSPACE_FILES = 200;
const MAX_DEPTH = 6;
const UNKNOWN_BLOCK_CODE = "unknown-block";
const UNKNOWN_DIRECTIVE_CODE = "unknown-directive";
const MISSING_BLOCK_CLOSE_CODE = "missing-block-close-bracket";
const UNEXPECTED_BLOCK_END_CODE = "unexpected-block-end";

let workspaceFolders: string[] = [];

const BLOCK_DIRECTIVES: Record<string, string[]> = {
  workspace: [".set"],
  profile: [".set"],
  tool: [".exec", ".set"],
  bake: [".make", ".output", ".ref", ".needs"],
  run: [".set", ".takes", ".emits"],
  export: [],
};

connection.onInitialize((params: InitializeParams): InitializeResult => {
  workspaceFolders =
    params.workspaceFolders?.map((f) => f.uri.replace("file://", "")) ?? [];
  if (workspaceFolders.length === 0 && params.rootUri) {
    workspaceFolders = [params.rootUri.replace("file://", "")];
  }
  return {
    capabilities: {
      textDocumentSync: TextDocumentSyncKind.Incremental,
      completionProvider: {
        triggerCharacters: ["[", ".", " ", "\n"],
      },
      codeActionProvider: true,
      hoverProvider: true,
    },
  };
});

connection.onCompletion((params: CompletionParams): CompletionItem[] => {
  const doc = documents.get(params.textDocument.uri);
  if (!doc) {
    return [];
  }

  const offset = doc.offsetAt(params.position);
  const text = doc.getText();
  const before = text.slice(0, offset);
  const line = before.split("\n").slice(-1)[0] || "";
  const blockKind = currentBlockKind(before);

  const { bakes, tools, profiles, vars, ports } = collectSymbols(
    text,
    readWorkspaceSources()
  );

  const isHeader = /\[[^\]]*$/.test(before);
  const isDirective = /\.[A-Za-z_]*$/.test(before);

  const items: CompletionItem[] = [];

  if (isHeader) {
    for (const block of TOP_LEVEL_BLOCKS) {
      items.push({
        label: block,
        kind: CompletionItemKind.Keyword,
        insertText: block,
      });
    }
    return items;
  }

  if (/^\s*\[run\s+[^\]]*$/.test(line)) {
    if (blockKind !== "bake") {
      return items;
    }
    for (const tool of tools) {
      items.push({
        label: tool,
        kind: CompletionItemKind.Function,
        insertText: tool,
      });
    }
    return items;
  }

  if (isDirective) {
    const allowed = blockKind ? BLOCK_DIRECTIVES[blockKind] || [] : [];
    for (const dir of allowed) {
      items.push({
        label: dir,
        kind: CompletionItemKind.Keyword,
        insertText: dir,
      });
    }
    return items;
  }

  if (/\.(needs|ref)\b/.test(line)) {
    if (blockKind !== "bake") {
      return items;
    }
    for (const bake of bakes) {
      items.push({
        label: bake,
        kind: CompletionItemKind.Reference,
        insertText: bake,
      });
    }
    return items;
  }

  if (/\.make\b/.test(line)) {
    if (blockKind !== "bake") {
      return items;
    }
    for (const kind of MAKE_KINDS) {
      items.push({
        label: kind,
        kind: CompletionItemKind.Keyword,
        insertText: kind,
      });
    }
    const allow = line.replace(/\"[^\"]*$/, "");
    if (/\.make\s+\S+\s+\S+/.test(allow)) {
      for (const hint of ["\"src/*.c\"", "\"src/**/*.c\"", "\"src/*.cpp\"", "\"tests/*.py\""]) {
        items.push({
          label: hint,
          kind: CompletionItemKind.Snippet,
          insertText: hint,
        });
      }
    }
    return items;
  }

  if (/\.output\b/.test(line)) {
    if (blockKind !== "bake") {
      return items;
    }
    for (const port of OUTPUT_PORTS) {
      items.push({
        label: port,
        kind: CompletionItemKind.EnumMember,
        insertText: port,
      });
    }
    for (const port of ports) {
      items.push({
        label: port,
        kind: CompletionItemKind.EnumMember,
        insertText: port,
      });
    }
    for (const prefix of OUTPUT_PREFIXES) {
      items.push({
        label: `"${prefix}/"`,
        kind: CompletionItemKind.Folder,
        insertText: `"${prefix}/"`,
      });
    }
    return items;
  }

  if (/\.emits\b/.test(line)) {
    if (blockKind !== "run") {
      return items;
    }
    for (const port of OUTPUT_PORTS) {
      items.push({
        label: port,
        kind: CompletionItemKind.EnumMember,
        insertText: port,
      });
    }
    for (const port of ports) {
      items.push({
        label: port,
        kind: CompletionItemKind.EnumMember,
        insertText: port,
      });
    }
    if (/\bas\s+\"[^\"]*$/.test(line)) {
      items.push({
        label: "\"-o\"",
        kind: CompletionItemKind.Constant,
        insertText: "\"-o\"",
      });
      return items;
    }
    items.push({
      label: "\"-o\"",
      kind: CompletionItemKind.Constant,
      insertText: "\"-o\"",
    });
    return items;
  }

  if (/\.set\b/.test(line)) {
    const keys =
      blockKind === "profile"
        ? PROFILE_KEYS
        : blockKind === "workspace"
          ? WORKSPACE_KEYS
          : [];
    for (const key of keys) {
      items.push({
        label: key,
        kind: CompletionItemKind.Property,
        insertText: key,
      });
    }
    if (items.length > 0) {
      return items;
    }
  }

  if (/\.set\b/.test(line)) {
    const keyMatch = line.match(/\.set\s+([A-Za-z0-9_]+)\s+/);
    const key = keyMatch?.[1];
    if (key && PROFILE_VALUES[key]) {
      for (const value of PROFILE_VALUES[key]) {
        items.push({
          label: value,
          kind: CompletionItemKind.Value,
          insertText: value,
        });
      }
      return items;
    }
    if (key && WORKSPACE_VALUES[key]) {
      for (const value of WORKSPACE_VALUES[key]) {
        items.push({
          label: value,
          kind: CompletionItemKind.Value,
          insertText: value,
        });
      }
      return items;
    }
    if (blockKind === "tool" || blockKind === "run") {
      for (const flag of TOOL_FLAGS) {
        items.push({
          label: flag,
          kind: CompletionItemKind.Constant,
          insertText: `"${flag}"`,
        });
      }
    }
  }

  if (/\$\{[^}]*$/.test(line)) {
    for (const hint of VARIABLE_HINTS) {
      items.push({
        label: hint,
        kind: CompletionItemKind.Variable,
        insertText: hint,
      });
    }
    for (const v of vars) {
      items.push({
        label: `\${${v}}`,
        kind: CompletionItemKind.Variable,
        insertText: `\${${v}}`,
      });
    }
    return items;
  }

  if (/^\s*$/.test(before.split("\n").slice(-1)[0] || "") && !blockKind) {
    for (const block of TOP_LEVEL_BLOCKS) {
      items.push({
        label: `[${block}]`,
        kind: CompletionItemKind.Snippet,
        insertText: `[${block}]`,
      });
    }
  }

  return items;
});

connection.onCodeAction((params: CodeActionParams): CodeAction[] => {
  const actions: CodeAction[] = [];
  const doc = documents.get(params.textDocument.uri);
  if (!doc) {
    return actions;
  }

  const lines = doc.getText().split("\n");
  const { tools } = collectSymbols(doc.getText(), []);

  for (const diag of params.context.diagnostics) {
    if (diag.code === "unknown-tool" && typeof diag.data === "string") {
      const toolName = diag.data;
      actions.push({
        title: `Create [tool ${toolName}]`,
        kind: CodeActionKind.QuickFix,
        diagnostics: [diag],
        edit: {
          changes: {
            [doc.uri]: [
              {
                range: {
                  start: { line: 0, character: 0 },
                  end: { line: 0, character: 0 },
                },
                newText: `[tool ${toolName}]\n  .exec \"${toolName}\"\n..\n\n`,
              },
            ],
          },
        },
      });
    }
    if (diag.code === "unknown-bake" && typeof diag.data === "string") {
      const bakeName = diag.data;
      actions.push({
        title: `Create [bake ${bakeName}]`,
        kind: CodeActionKind.QuickFix,
        diagnostics: [diag],
        edit: {
          changes: {
            [doc.uri]: [
              {
                range: {
                  start: { line: doc.lineCount, character: 0 },
                  end: { line: doc.lineCount, character: 0 },
                },
                newText:
                  `\n[bake ${bakeName}]\n  .make src glob \"src/*\"\n  [run tool]\n    .set \"-O2\" 1\n  ..\n  .output exe \"target/out/${bakeName}\"\n..\n`,
              },
            ],
          },
        },
      });
    }
    if (diag.code === "missing-block-end") {
      actions.push({
        title: "Insert '..' to close block",
        kind: CodeActionKind.QuickFix,
        diagnostics: [diag],
        edit: {
          changes: {
            [doc.uri]: [
              {
                range: {
                  start: { line: doc.lineCount, character: 0 },
                  end: { line: doc.lineCount, character: 0 },
                },
                newText: "\n..\n",
              },
            ],
          },
        },
      });
    }
    if (diag.code === UNKNOWN_BLOCK_CODE && typeof diag.data === "string") {
      const suggestion = diag.data;
      actions.push({
        title: `Replace with [${suggestion}]`,
        kind: CodeActionKind.QuickFix,
        diagnostics: [diag],
        edit: {
          changes: {
            [doc.uri]: [
              {
                range: diag.range,
                newText: suggestion,
              },
            ],
          },
        },
      });
    }
    if (diag.code === UNKNOWN_DIRECTIVE_CODE && typeof diag.data === "string") {
      const suggestion = diag.data;
      actions.push({
        title: `Replace with ${suggestion}`,
        kind: CodeActionKind.QuickFix,
        diagnostics: [diag],
        edit: {
          changes: {
            [doc.uri]: [
              {
                range: diag.range,
                newText: suggestion,
              },
            ],
          },
        },
      });
    }
    if (diag.code === MISSING_BLOCK_CLOSE_CODE) {
      actions.push({
        title: "Add closing ']'",
        kind: CodeActionKind.QuickFix,
        diagnostics: [diag],
        edit: {
          changes: {
            [doc.uri]: [
              {
                range: {
                  start: { line: diag.range.end.line, character: diag.range.end.character },
                  end: { line: diag.range.end.line, character: diag.range.end.character },
                },
                newText: "]",
              },
            ],
          },
        },
      });
    }
    if (diag.code === UNEXPECTED_BLOCK_END_CODE) {
      actions.push({
        title: "Remove '..'",
        kind: CodeActionKind.QuickFix,
        diagnostics: [diag],
        edit: {
          changes: {
            [doc.uri]: [
              {
                range: {
                  start: { line: diag.range.start.line, character: 0 },
                  end: {
                    line: diag.range.end.line,
                    character: Math.max(diag.range.end.character, 2),
                  },
                },
                newText: "",
              },
            ],
          },
        },
      });
    }
    if (diag.code === "missing-bake-output" && typeof diag.data === "string") {
      const bakeName = diag.data || "app";
      const insertLine = findBakeBlockCloseLine(lines, bakeName) ?? doc.lineCount;
      actions.push({
        title: "Add .output to bake",
        kind: CodeActionKind.QuickFix,
        diagnostics: [diag],
        edit: {
          changes: {
            [doc.uri]: [
              {
                range: {
                  start: { line: insertLine, character: 0 },
                  end: { line: insertLine, character: 0 },
                },
                newText: `  .output exe \"target/out/${bakeName}\"\n`,
              },
            ],
          },
        },
      });
    }
    if (diag.code === "missing-bake-run" && typeof diag.data === "string") {
      const bakeName = diag.data;
      const toolName = tools[0] || "tool";
      const insertLine = findBakeBlockCloseLine(lines, bakeName) ?? doc.lineCount;
      actions.push({
        title: "Add [run] to bake",
        kind: CodeActionKind.QuickFix,
        diagnostics: [diag],
        edit: {
          changes: {
            [doc.uri]: [
              {
                range: {
                  start: { line: insertLine, character: 0 },
                  end: { line: insertLine, character: 0 },
                },
                newText: `  [run ${toolName}]\n    .set \"-O2\" 1\n  ..\n`,
              },
            ],
          },
        },
      });
    }
  }

  return actions;
});

connection.onHover((params: HoverParams) => {
  const doc = documents.get(params.textDocument.uri);
  if (!doc) {
    return null;
  }

  const diagnostics = collectDiagnostics(doc);
  const hit = diagnostics.find((diag) => {
    const { line, character } = params.position;
    if (line < diag.range.start.line || line > diag.range.end.line) {
      return false;
    }
    if (line === diag.range.start.line && character < diag.range.start.character) {
      return false;
    }
    if (line === diag.range.end.line && character > diag.range.end.character) {
      return false;
    }
    return true;
  });

  if (!hit) {
    return null;
  }

  const suggestions = hoverSuggestions(hit);
  const value =
    suggestions.length > 0
      ? `${hit.message}\n\nQuick fix: ${suggestions.join(" | ")}`
      : hit.message;

  return {
    contents: {
      kind: "markdown",
      value,
    },
    range: hit.range,
  };
});

documents.onDidChangeContent((change) => {
  validateDocument(change.document);
});

documents.onDidOpen((event) => {
  validateDocument(event.document);
});

documents.onDidSave((event) => {
  validateDocument(event.document);
});

function collectSymbols(
  text: string,
  extraSources: string[]
): {
  bakes: string[];
  tools: string[];
  profiles: string[];
  vars: string[];
  ports: string[];
} {
  const bakes = new Set<string>();
  const tools = new Set<string>();
  const profiles = new Set<string>();
  const vars = new Set<string>();
  const ports = new Set<string>();

  const allLines = text
    .split("\n")
    .concat(...extraSources.map((src) => src.split("\n")));

  let blockKind: string | null = null;

  for (const line of allLines) {
    const bake = line.match(/\[bake\s+([^\]\s]+)\]/);
    if (bake?.[1]) {
      bakes.add(bake[1]);
      blockKind = "bake";
    }
    const tool = line.match(/\[tool\s+([^\]\s]+)\]/);
    if (tool?.[1]) {
      tools.add(tool[1]);
      blockKind = "tool";
    }
    const profile = line.match(/\[profile\s+([^\]\s]+)\]/);
    if (profile?.[1]) {
      profiles.add(profile[1]);
      blockKind = "profile";
    }
    if (line.match(/^\s*\[workspace\]/)) {
      blockKind = "workspace";
    }
    if (line.match(/^\s*\.\.\s*$/)) {
      blockKind = null;
    }

    const setMatch = line.match(/^\s*\.set\s+([A-Za-z0-9_]+)/);
    if (setMatch?.[1] && (blockKind === "workspace" || blockKind === "profile")) {
      vars.add(setMatch[1]);
    }

    const outputMatch = line.match(/^\s*\.output\s+([A-Za-z0-9_]+)/);
    if (outputMatch?.[1]) {
      ports.add(outputMatch[1]);
    }
  }

  return {
    bakes: Array.from(bakes).sort(),
    tools: Array.from(tools).sort(),
    profiles: Array.from(profiles).sort(),
    vars: Array.from(vars).sort(),
    ports: Array.from(ports).sort(),
  };
}

function readWorkspaceSources(): string[] {
  const files = new Set<string>();
  for (const folder of workspaceFolders) {
    for (const f of walkWorkspace(folder)) {
      files.add(f);
      if (files.size >= MAX_WORKSPACE_FILES) {
        break;
      }
    }
  }

  const out: string[] = [];
  for (const file of files) {
    try {
      out.push(fs.readFileSync(file, "utf8"));
    } catch {
      // ignore unreadable files
    }
  }
  return out;
}

function walkWorkspace(root: string): string[] {
  const out: string[] = [];
  const stack: Array<{ dir: string; depth: number }> = [{ dir: root, depth: 0 }];

  while (stack.length > 0 && out.length < MAX_WORKSPACE_FILES) {
    const cur = stack.pop();
    if (!cur) {
      continue;
    }
    const { dir, depth } = cur;
    if (depth > MAX_DEPTH) {
      continue;
    }
    let entries: fs.Dirent[] = [];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (entry.name === ".git" || entry.name === "node_modules") {
        continue;
      }
      if (entry.isDirectory()) {
        if (entry.name === "target" || entry.name === "dist") {
          continue;
        }
        stack.push({ dir: path.join(dir, entry.name), depth: depth + 1 });
      } else if (entry.isFile()) {
        if (
          entry.name === "steelconf" ||
          entry.name.endsWith(".steelconf") ||
          entry.name === "steelconfig.mff"
        ) {
          out.push(path.join(dir, entry.name));
          if (out.length >= MAX_WORKSPACE_FILES) {
            break;
          }
        }
      }
    }
  }
  return out;
}

function validateDocument(doc: TextDocument) {
  const diagnostics = collectDiagnostics(doc);
  connection.sendDiagnostics({ uri: doc.uri, diagnostics });
}

function currentBlockKind(before: string): string | null {
  const stack: string[] = [];
  const lines = before.split("\n");
  const blockStartRe = /^\s*\[([a-zA-Z_]+)(?:\s+[^\]\s]+)?\]/;

  for (const line of lines) {
    const match = line.match(blockStartRe);
    if (match?.[1]) {
      stack.push(match[1]);
      continue;
    }
    if (/^\s*\.\.\s*$/.test(line)) {
      stack.pop();
    }
  }

  return stack.length > 0 ? stack[stack.length - 1] : null;
}

function collectDiagnostics(doc: TextDocument): Diagnostic[] {
  const text = doc.getText();
  const lines = text.split("\n");
  const diagnostics: Diagnostic[] = [];

  const { bakes, tools } = collectSymbols(text, []);

  let currentBlock: { kind: string; name: string; startLine: number } | null =
    null;
  let openBlocks = 0;
  let bakeHasOutput = false;
  let bakeHasRun = false;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];

    const blockMatch = line.match(/^\s*\[([a-zA-Z_]+)(?:\s+([^\]\s]+))?\]/);
    if (blockMatch) {
      const blockName = blockMatch[1];
      if (!TOP_LEVEL_BLOCKS.includes(blockName)) {
        const suggestion = closestMatch(blockName, TOP_LEVEL_BLOCKS);
        diagnostics.push(
          makeDiag(
            i,
            line.indexOf(blockName),
            line.indexOf(blockName) + blockName.length,
            `Unknown block: ${blockName}`,
            DiagnosticSeverity.Error,
            UNKNOWN_BLOCK_CODE,
            suggestion ? suggestion : undefined
          )
        );
      }
      openBlocks += 1;
      if (currentBlock?.kind === "bake") {
        if (!bakeHasOutput) {
          diagnostics.push(
            makeDiag(
              i,
              0,
              line.length,
              "Bake missing .output",
              DiagnosticSeverity.Warning,
              "missing-bake-output",
              currentBlock.name
            )
          );
        }
        if (!bakeHasRun) {
          diagnostics.push(
            makeDiag(
              i,
              0,
              line.length,
              "Bake missing [run]",
              DiagnosticSeverity.Warning,
              "missing-bake-run",
              currentBlock.name
            )
          );
        }
      }
      currentBlock = {
        kind: blockMatch[1],
        name: blockMatch[2] || "",
        startLine: i,
      };
      bakeHasOutput = false;
      bakeHasRun = false;
      continue;
    }

    if (/^\s*\[[^\]]*$/.test(line)) {
      diagnostics.push(
        makeDiag(
          i,
          Math.max(0, line.indexOf("[")),
          line.length,
          "Block header missing ']'",
          DiagnosticSeverity.Error,
          MISSING_BLOCK_CLOSE_CODE
        )
      );
      continue;
    }

    if (/^\s*\.\.\s*$/.test(line)) {
      if (openBlocks === 0) {
        diagnostics.push(
          makeDiag(
            i,
            line.indexOf(".."),
            line.indexOf("..") + 2,
            "Unexpected '..' without open block",
            DiagnosticSeverity.Error,
            UNEXPECTED_BLOCK_END_CODE
          )
        );
        continue;
      }
      if (openBlocks > 0) {
        openBlocks -= 1;
      }
      if (currentBlock?.kind === "bake") {
        if (!bakeHasOutput) {
          diagnostics.push(
            makeDiag(
              i,
              0,
              line.length,
              "Bake missing .output",
              DiagnosticSeverity.Warning,
              "missing-bake-output",
              currentBlock.name
            )
          );
        }
        if (!bakeHasRun) {
          diagnostics.push(
            makeDiag(
              i,
              0,
              line.length,
              "Bake missing [run]",
              DiagnosticSeverity.Warning,
              "missing-bake-run",
              currentBlock.name
            )
          );
        }
      }
      currentBlock = null;
      continue;
    }

    const directiveMatch = line.match(/^\s*\.(\w+)/);
    if (directiveMatch) {
      const directive = `.${directiveMatch[1]}`;
      if (!DIRECTIVES.includes(directive)) {
        const suggestion = closestMatch(directive, DIRECTIVES);
        diagnostics.push(
          makeDiag(
            i,
            line.indexOf(directive),
            line.indexOf(directive) + directive.length,
            `Unknown directive: ${directive}`,
            DiagnosticSeverity.Error,
            UNKNOWN_DIRECTIVE_CODE,
            suggestion ? suggestion : undefined
          )
        );
      }
    }

    if (currentBlock?.kind === "bake") {
      if (/^\s*\[run\s+([^\]\s]+)\]/.test(line)) {
        bakeHasRun = true;
        const toolName = line.match(/^\s*\[run\s+([^\]\s]+)\]/)?.[1];
        if (toolName && !tools.includes(toolName)) {
          diagnostics.push(
            makeDiag(
              i,
              line.indexOf(toolName),
              line.indexOf(toolName) + toolName.length,
              `Unknown tool: ${toolName}`,
              DiagnosticSeverity.Warning,
              "unknown-tool",
              toolName
            )
          );
        }
      }
      if (/^\s*\.output\b/.test(line)) {
        bakeHasOutput = true;
      }
    }

    if (/^\s*\.(needs|ref)\s+([^\s]+)/.test(line)) {
      const name = line.match(/^\s*\.(needs|ref)\s+([^\s]+)/)?.[2];
      if (name && !bakes.includes(name)) {
        diagnostics.push(
          makeDiag(
            i,
            line.indexOf(name),
            line.indexOf(name) + name.length,
            `Unknown bake: ${name}`,
            DiagnosticSeverity.Warning,
            "unknown-bake",
            name
          )
        );
      }
    }
  }

  if (currentBlock?.kind === "bake") {
    if (!bakeHasOutput) {
      diagnostics.push(
        makeDiag(
          currentBlock.startLine,
          0,
          1,
          "Bake missing .output",
          DiagnosticSeverity.Warning,
          "missing-bake-output",
          currentBlock.name
        )
      );
    }
    if (!bakeHasRun) {
      diagnostics.push(
        makeDiag(
          currentBlock.startLine,
          0,
          1,
          "Bake missing [run]",
          DiagnosticSeverity.Warning,
          "missing-bake-run",
          currentBlock.name
        )
      );
    }
  }
  if (openBlocks > 0) {
    diagnostics.push(
      makeDiag(
        Math.max(0, lines.length - 1),
        0,
        1,
        "Block not closed with '..'",
        DiagnosticSeverity.Error,
        "missing-block-end"
      )
    );
  }

  return diagnostics;
}

function makeDiag(
  line: number,
  start: number,
  end: number,
  message: string,
  severity: DiagnosticSeverity,
  code?: string,
  data?: string
): Diagnostic {
  return {
    severity,
    range: {
      start: { line, character: Math.max(0, start) },
      end: { line, character: Math.max(0, end) },
    },
    message,
    source: "steelconf",
    code,
    data,
  };
}

function findBakeBlockCloseLine(
  lines: string[],
  bakeName: string
): number | null {
  const startRe = new RegExp(`^\\s*\\[bake\\s+${escapeRegExp(bakeName)}\\]`);
  const blockStartRe = /^\s*\[([a-zA-Z_]+)(?:\s+([^\]\s]+))?\]/;
  for (let i = 0; i < lines.length; i += 1) {
    if (!startRe.test(lines[i])) {
      continue;
    }
    let depth = 1;
    for (let j = i + 1; j < lines.length; j += 1) {
      const line = lines[j];
      if (blockStartRe.test(line)) {
        depth += 1;
        continue;
      }
      if (/^\s*\.\.\s*$/.test(line)) {
        depth -= 1;
        if (depth === 0) {
          return j;
        }
      }
    }
    return null;
  }
  return null;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function hoverSuggestions(diag: Diagnostic): string[] {
  if (diag.code === "unknown-tool" && typeof diag.data === "string") {
    return [`Create [tool ${diag.data}]`];
  }
  if (diag.code === "unknown-bake" && typeof diag.data === "string") {
    return [`Create [bake ${diag.data}]`];
  }
  if (diag.code === UNKNOWN_BLOCK_CODE && typeof diag.data === "string") {
    return [`Replace with [${diag.data}]`];
  }
  if (diag.code === UNKNOWN_DIRECTIVE_CODE && typeof diag.data === "string") {
    return [`Replace with ${diag.data}`];
  }
  if (diag.code === MISSING_BLOCK_CLOSE_CODE) {
    return ["Add closing ']'"];
  }
  if (diag.code === UNEXPECTED_BLOCK_END_CODE) {
    return ["Remove '..'"];
  }
  if (diag.code === "missing-block-end") {
    return ["Insert '..' to close block"];
  }
  if (diag.code === "missing-bake-output") {
    return ["Add .output to bake"];
  }
  if (diag.code === "missing-bake-run") {
    return ["Add [run] to bake"];
  }
  return [];
}

function closestMatch(value: string, choices: string[]): string | null {
  let best: string | null = null;
  let bestScore = Number.POSITIVE_INFINITY;
  for (const choice of choices) {
    const score = levenshtein(value, choice);
    if (score < bestScore) {
      bestScore = score;
      best = choice;
    }
  }
  return bestScore <= 3 ? best : null;
}

function levenshtein(a: string, b: string): number {
  const dp: number[] = new Array(b.length + 1);
  for (let j = 0; j <= b.length; j += 1) {
    dp[j] = j;
  }
  for (let i = 1; i <= a.length; i += 1) {
    let prev = dp[0];
    dp[0] = i;
    for (let j = 1; j <= b.length; j += 1) {
      const temp = dp[j];
      if (a[i - 1] === b[j - 1]) {
        dp[j] = prev;
      } else {
        dp[j] = Math.min(prev + 1, dp[j] + 1, dp[j - 1] + 1);
      }
      prev = temp;
    }
  }
  return dp[b.length];
}

documents.listen(connection);
connection.listen();
