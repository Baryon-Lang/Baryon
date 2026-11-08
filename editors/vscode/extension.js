// The Baryon extension's code: it starts `baryon lsp` for `.byn` files and stops it on exit.
//
// Highlighting and editing behavior need no code; `package.json` declares them. This file only
// wires the language server in, through Microsoft's `vscode-languageclient`, which speaks the
// protocol, restarts a crashed server and turns its diagnostics into squiggles. The server reads
// the first workspace folder as one package, as `docs/reference/commands.md` describes.
// Decision 0025 explains why the client is plain JavaScript on that package.

"use strict";

const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

/** The running client, kept so `deactivate` can stop the server. Undefined until activation. */
let client;

/**
 * Starts the language server. VS Code calls this the first time a Baryon file opens.
 *
 * The server is the `baryon` program named by the `baryon.path` setting, run as `baryon lsp`
 * over standard input and output. If it cannot be started, the user sees one error message
 * naming the setting, and highlighting keeps working without diagnostics.
 *
 * @param {vscode.ExtensionContext} context Owns what the extension must clean up.
 * @returns {Promise<void>} Settles once the server has answered `initialize` or failed to.
 */
async function activate(context) {
  const command = vscode.workspace.getConfiguration("baryon").get("path", "baryon");
  const server = { command, args: ["lsp"] };
  client = new LanguageClient(
    "baryon",
    "Baryon",
    { run: server, debug: server },
    { documentSelector: [{ scheme: "file", language: "baryon" }] },
  );
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("baryon.path")) {
        vscode.window.showInformationMessage(
          "Reload the window to start the Baryon language server from the new `baryon.path`.",
        );
      }
    }),
  );
  try {
    await client.start();
  } catch (error) {
    vscode.window.showErrorMessage(
      `Could not start \`${command} lsp\`: ${error.message}. ` +
        "Install `baryon`, or set `baryon.path` to the program, then reload the window.",
    );
  }
}

/**
 * Stops the language server. VS Code calls this when the window closes or the extension is
 * disabled.
 *
 * @returns {Promise<void> | undefined} Settles once the server has shut down.
 */
function deactivate() {
  return client?.stop();
}

module.exports = { activate, deactivate };
