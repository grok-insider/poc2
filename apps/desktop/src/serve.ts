// app:// protocol — serves the apps/web static export.
//
// The web export uses root-absolute asset URLs (/_next/*, /wasm/*,
// /poc2.bundle.json.gz, /base-icons/*, ...), which file:// cannot satisfy.
// A privileged custom scheme keeps those URLs working and gives the
// renderer a real secure origin (fetch, Web Workers, WASM all behave).
import { app, net, protocol } from "electron";
import { existsSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { resolveAppUrl, wantsSpaFallback } from "./staticResolve";

export const APP_SCHEME = "app";
export const APP_ORIGIN = `${APP_SCHEME}://poc2`;

/** Must run before app.whenReady(). */
export function registerAppScheme(): void {
  protocol.registerSchemesAsPrivileged([
    {
      scheme: APP_SCHEME,
      privileges: {
        standard: true,
        secure: true,
        supportFetchAPI: true,
        stream: true,
        codeCache: true,
      },
    },
  ]);
}

/** Where the web export lives: packaged → resources/web, dev → ../web/out. */
export function webRoot(): string {
  if (app.isPackaged) {
    return path.join(process.resourcesPath, "web");
  }
  return path.join(__dirname, "..", "..", "web", "out");
}

/** Must run after app.whenReady(). */
export function handleAppScheme(): void {
  const root = webRoot();
  protocol.handle(APP_SCHEME, (request) => {
    const file = resolveAppUrl(root, request.url);
    if (file && existsSync(file)) {
      return net.fetch(pathToFileURL(file).toString());
    }
    // SPA fallback: unknown non-asset paths get index.html (the export is
    // a single page; this keeps deep links and reloads working).
    const index = path.join(root, "index.html");
    if (file && wantsSpaFallback(file) && existsSync(index)) {
      return net.fetch(pathToFileURL(index).toString());
    }
    return new Response("not found", { status: 404 });
  });
}
