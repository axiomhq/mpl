import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const executable = process.platform === "win32" ? "mpl-lsp.exe" : "mpl-lsp";
  const configuredServer = vscode.workspace
    .getConfiguration("mpl")
    .get<string>("server.path")
    ?.trim();
  const bundledServer = context.asAbsolutePath(path.join("server", executable));
  const command = configuredServer || (fs.existsSync(bundledServer) ? bundledServer : executable);

  const serverOptions: ServerOptions = {
    run: {
      command,
      transport: TransportKind.stdio,
    },
    debug: {
      command,
      transport: TransportKind.stdio,
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "mpl" }],
  };

  client = new LanguageClient("mpl", "MPL Language Server", serverOptions, clientOptions);

  try {
    await client.start();
  } catch (error) {
    client = undefined;
    const detail = error instanceof Error ? error.message : String(error);
    void vscode.window.showErrorMessage(
      `Failed to start the MPL language server using "${command}". ` +
        `Set mpl.server.path or install ${executable} on PATH. ${detail}`,
    );
    throw error;
  }
}

export async function deactivate(): Promise<void> {
  await client?.dispose();
  client = undefined;
}
