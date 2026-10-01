import { createReadStream } from "node:fs";
import { mkdir, stat } from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  createCivilizationService,
  routeCivilizationRequest,
} from "./civilization-service.mjs";

const MODULE_DIR = path.dirname(fileURLToPath(import.meta.url));
const PROJECT_DIR = path.resolve(MODULE_DIR, "..");
// Use the same default state root as the full gateway so switching between the
// chain-independent server and port 4173 does not create a second civilization.
// Only one writer should serve a given workDir at a time.
const DEFAULT_WORK_DIR = path.resolve(PROJECT_DIR, "../../work/devnet");
const DEFAULT_STATIC_DIR = path.resolve(PROJECT_DIR, "../permutation-state-prototype");

const CONTENT_TYPES = Object.freeze({
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
});

function sendJson(response, statusCode, value) {
  const body = `${JSON.stringify(value)}\n`;
  response.writeHead(statusCode, {
    "Content-Type": "application/json; charset=utf-8",
    "Content-Length": Buffer.byteLength(body),
    "Cache-Control": "no-store",
  });
  response.end(body);
}

function redirectToCivilization(response) {
  response.writeHead(302, {
    Location: "/civilization/",
    "Cache-Control": "no-store",
  });
  response.end();
}

async function serveStatic(staticDir, requestUrl, response, method) {
  let pathname;
  try {
    pathname = decodeURIComponent(requestUrl.pathname);
  } catch {
    sendJson(response, 400, { error: "Invalid URL path" });
    return;
  }
  if (pathname.endsWith("/")) pathname += "index.html";
  const filePath = path.resolve(staticDir, `.${pathname}`);
  if (filePath !== staticDir && !filePath.startsWith(`${staticDir}${path.sep}`)) {
    sendJson(response, 403, { error: "Forbidden" });
    return;
  }
  try {
    const info = await stat(filePath);
    if (!info.isFile()) throw new Error("Not a file");
    response.writeHead(200, {
      "Content-Type": CONTENT_TYPES[path.extname(filePath)] || "application/octet-stream",
      "Content-Length": info.size,
      "Cache-Control": "no-cache",
    });
    if (method === "HEAD") return response.end();
    createReadStream(filePath).pipe(response);
  } catch {
    sendJson(response, 404, { error: "Not found" });
  }
}

async function route(runtime, request, response) {
  const requestUrl = new URL(request.url, "http://127.0.0.1");
  if (await routeCivilizationRequest(runtime.civilizationService, request, response, requestUrl)) return;
  if (
    (request.method === "GET" || request.method === "HEAD")
    && (requestUrl.pathname === "/" || requestUrl.pathname === "/civilization")
  ) {
    redirectToCivilization(response);
    return;
  }
  if (request.method !== "GET" && request.method !== "HEAD") {
    sendJson(response, 405, { error: "Method not allowed" });
    return;
  }
  await serveStatic(runtime.staticDir, requestUrl, response, request.method);
}

export async function startCivilizationServer({
  port = Number(process.env.CIVILIZATION_PORT || 4174),
  workDir = path.resolve(
    process.env.PERMSTATE_CIVILIZATION_WORK_DIR
      || process.env.PERMSTATE_WORK_DIR
      || DEFAULT_WORK_DIR,
  ),
  staticDir = path.resolve(process.env.PERMSTATE_STATIC_DIR || DEFAULT_STATIC_DIR),
  log = console.log,
} = {}) {
  const civilizationService = createCivilizationService({ workDir });
  const runtime = { civilizationService, workDir, staticDir };
  await mkdir(workDir, { recursive: true });
  const server = http.createServer((request, response) => {
    route(runtime, request, response).catch((error) => {
      console.error(`[civilization] ${request.method} ${request.url}: ${error.message}`);
      if (!response.headersSent) {
        sendJson(response, error.statusCode || 500, {
          error: error.message,
          code: error.code || "INTERNAL_ERROR",
        });
      } else {
        response.destroy(error);
      }
    });
  });
  server.once("close", () => {
    civilizationService.close().catch((error) => {
      console.error(`[civilization] close: ${error.message}`);
    });
  });
  try {
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(port, "127.0.0.1", resolve);
    });
  } catch (error) {
    await civilizationService.close();
    throw error;
  }
  const address = server.address();
  const actualPort = typeof address === "object" && address ? address.port : port;
  log(`Wylls civilization ready at http://127.0.0.1:${actualPort}/civilization/`);
  return { server, runtime, url: `http://127.0.0.1:${actualPort}/civilization/` };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  startCivilizationServer().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
