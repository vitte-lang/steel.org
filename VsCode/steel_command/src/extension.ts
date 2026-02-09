import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
  const serverModule = context.asAbsolutePath(
    path.join("server", "dist", "server.js")
  );
  const serverOptions: ServerOptions = {
    run: { module: serverModule, transport: TransportKind.ipc },
    debug: {
      module: serverModule,
      transport: TransportKind.ipc,
      options: { execArgv: ["--nolazy", "--inspect=6009"] },
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: "steelconf" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/steelconf"),
    },
  };

  client = new LanguageClient(
    "steelconfLanguageServer",
    "Steelconf Language Server",
    serverOptions,
    clientOptions
  );

  client.start();
  context.subscriptions.push(client);

  context.subscriptions.push(
    vscode.commands.registerCommand("steelconf.syntaxHelp", async () => {
      await openMarkdownPreview(context, "syntax-help.md");
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("steelconf.openManifest", async () => {
      await openMarkdownPreview(
        context,
        path.join("..", "..", "doc", "manifest.md"),
        "Impossible d'ouvrir doc/manifest.md."
      );
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("steelconf.openRootSteelconf", async () => {
      await openWorkspaceFile(
        "steelconf",
        "Aucun fichier steelconf trouve a la racine du workspace."
      );
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("steelconf.openSwiftExample", async () => {
      await openWorkspaceFile(
        path.join("examples", "swift", "steelconf"),
        "Exemple Swift introuvable (examples/swift/steelconf)."
      );
    })
  );
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
}

async function openMarkdownPreview(
  context: vscode.ExtensionContext,
  relativePath: string,
  fallbackMessage = "Aide indisponible."
): Promise<void> {
  const helpUri = vscode.Uri.file(context.asAbsolutePath(relativePath));
  try {
    await vscode.workspace.fs.stat(helpUri);
    await vscode.commands.executeCommand("markdown.showPreview", helpUri);
  } catch {
    try {
      const doc = await vscode.workspace.openTextDocument(helpUri);
      await vscode.window.showTextDocument(doc, { preview: true });
    } catch {
      void vscode.window.showWarningMessage(fallbackMessage);
    }
  }
}

async function openWorkspaceFile(
  relativePath: string,
  notFoundMessage: string
): Promise<void> {
  const root = vscode.workspace.workspaceFolders?.[0]?.uri;
  if (!root) {
    void vscode.window.showWarningMessage("Aucun workspace ouvert.");
    return;
  }
  const fileUri = vscode.Uri.joinPath(root, relativePath);
  try {
    await vscode.workspace.fs.stat(fileUri);
    const doc = await vscode.workspace.openTextDocument(fileUri);
    await vscode.window.showTextDocument(doc, { preview: true });
  } catch {
    void vscode.window.showWarningMessage(notFoundMessage);
  }
}
