# MPL extension for Visual Studio Code

Language support for Axiom Metrics Processing Language in Visual Studio Code. The extension includes a TextMate grammar for syntax highlighting and connects to `mpl-lsp` for diagnostics and completions.

## Language server

The extension starts the language server in this order:

1. The executable configured through `mpl.server.path`.
2. The platform-specific executable bundled under `server/`.
3. `mpl-lsp` (`mpl-lsp.exe` on Windows) resolved from `PATH`.

Official platform-specific VSIX packages include the language server. For extension-development sessions without a staged server, install `mpl-lsp` on `PATH` or configure its absolute path in VS Code settings:

```json
{
  "mpl.server.path": "/absolute/path/to/mpl-lsp"
}
```

On Windows, use the path to `mpl-lsp.exe`. Restart VS Code after changing `PATH` so the extension receives the updated environment.

## Build and install from source

Run these commands from the repository root:

```sh
npm install
npm run build -w mpl-vscode
cargo build --release -p mpl-language-server --features lsp-bin --bin mpl-lsp
npm run stage -w mpl-vscode
npm exec -w mpl-vscode -- vsce package --no-dependencies --out mpl-vscode.vsix
code --install-extension packages/mpl-vscode/mpl-vscode.vsix
```

To stage another prebuilt language server, pass its path explicitly:

```sh
npm run stage -w mpl-vscode -- /path/to/mpl-lsp
```

During extension development, staging is optional when `mpl.server.path` is configured or `mpl-lsp` is available on `PATH`.
