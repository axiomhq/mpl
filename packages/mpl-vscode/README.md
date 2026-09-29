# MPL extension for Visual Studio Code

Language support for Axiom Metrics Processing Language in Visual Studio Code. The extension includes a TextMate grammar for syntax highlighting and connects to `mpl-lsp` for diagnostics and completions.

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

Otherwise, staging is optional, as the extension selects the language server in this priority order:

1. The executable configured through `mpl.server.path`, e.g. in your VS Code settings:
   ```json
   {
     "mpl.server.path": "/absolute/path/to/mpl-lsp"
   }
   ```
2. The platform-specific executable bundled during staging under `server/`.
3. `mpl-lsp` (`mpl-lsp.exe` on Windows) resolved from `PATH`.

