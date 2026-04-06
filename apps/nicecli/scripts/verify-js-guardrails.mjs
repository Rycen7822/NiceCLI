import { spawnSync } from "node:child_process";
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const appRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(appRoot, "..", "..");
const jsRoot = path.join(appRoot, "js");
const contractFile = path.join(
  repoRoot,
  "crates",
  "nicecli-backend",
  "src",
  "contract.rs",
);
const tauriMainFile = path.join(appRoot, "src-tauri", "src", "main.rs");

const routePattern =
  /(?:^|[^A-Za-z0-9_])((?:\/)?v0\/management\/[A-Za-z0-9._~:/?#[\]@!$&()*+,;=%-]+|(?:\/)?v1beta\/[A-Za-z0-9._~:/?#[\]@!$&()*+,;=%-]+|(?:\/)?v1\/[A-Za-z0-9._~:/?#[\]@!$&()*+,;=%-]+|(?:\/)?v1internal:[A-Za-z0-9._~:/?#[\]@!$&()*+,;=%-]+)/g;
const invokePattern = /\.invoke\(\s*["'`]([^"'`]+)["'`]/g;
const contractPattern = /"(?:GET|POST|PUT|PATCH|DELETE)\s+([^"]+)"/g;

function walkJsFiles(root) {
  const files = [];
  for (const entry of readdirSync(root)) {
    const fullPath = path.join(root, entry);
    const stats = statSync(fullPath);
    if (stats.isDirectory()) {
      files.push(...walkJsFiles(fullPath));
      continue;
    }
    if (fullPath.endsWith(".js")) {
      files.push(fullPath);
    }
  }
  return files.sort();
}

function normalizeRoute(rawRoute) {
  let route = rawRoute.trim();
  const interpolationIndex = route.indexOf("${");
  if (interpolationIndex >= 0) {
    route = route.slice(0, interpolationIndex);
  }
  const queryIndex = route.indexOf("?");
  if (queryIndex >= 0) {
    route = route.slice(0, queryIndex);
  }
  route = route.trim();
  if (!route) {
    return null;
  }
  if (!route.startsWith("/")) {
    route = `/${route}`;
  }
  return route;
}

function collectSyntaxFailures(files) {
  const failures = [];
  for (const file of files) {
    const result = spawnSync(process.execPath, ["--check", file], {
      encoding: "utf8",
    });
    if (result.status === 0) {
      continue;
    }
    failures.push({
      file,
      output: [result.stdout, result.stderr].filter(Boolean).join("\n").trim(),
    });
  }
  return failures;
}

function collectFrontendRoutes(files) {
  const routes = new Map();
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    for (const match of source.matchAll(routePattern)) {
      const normalized = normalizeRoute(match[1]);
      if (!normalized) {
        continue;
      }
      const filesForRoute = routes.get(normalized) ?? new Set();
      filesForRoute.add(path.relative(appRoot, file));
      routes.set(normalized, filesForRoute);
    }
  }
  return routes;
}

function collectContractRoutes() {
  const source = readFileSync(contractFile, "utf8");
  const routes = new Set();
  for (const match of source.matchAll(contractPattern)) {
    const normalized = normalizeRoute(match[1]);
    if (normalized) {
      routes.add(normalized);
    }
  }
  return routes;
}

function collectFrontendInvokes(files) {
  const invokes = new Map();
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    for (const match of source.matchAll(invokePattern)) {
      const command = match[1].trim();
      if (!command) {
        continue;
      }
      const filesForInvoke = invokes.get(command) ?? new Set();
      filesForInvoke.add(path.relative(appRoot, file));
      invokes.set(command, filesForInvoke);
    }
  }
  return invokes;
}

function collectTauriCommands() {
  const source = readFileSync(tauriMainFile, "utf8");
  const handlerBlock = source.match(/generate_handler!\[([\s\S]*?)\]/m)?.[1] ?? "";
  return new Set(handlerBlock.match(/[A-Za-z_][A-Za-z0-9_]*/g) ?? []);
}

function formatOwnership(map) {
  return [...map.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(
      ([name, owners]) =>
        `  - ${name}\n    ${[...owners].sort().join(", ")}`,
    )
    .join("\n");
}

const jsFiles = walkJsFiles(jsRoot);
const syntaxFailures = collectSyntaxFailures(jsFiles);
if (syntaxFailures.length > 0) {
  console.error("JS syntax verification failed:");
  for (const failure of syntaxFailures) {
    console.error(`- ${path.relative(appRoot, failure.file)}`);
    if (failure.output) {
      console.error(failure.output);
    }
  }
  process.exit(1);
}

const frontendRoutes = collectFrontendRoutes(jsFiles);
const contractRoutes = collectContractRoutes();
const unknownRoutes = new Map(
  [...frontendRoutes.entries()].filter(([route]) => !contractRoutes.has(route)),
);
if (unknownRoutes.size > 0) {
  console.error(
    "Frontend route references are not covered by crates/nicecli-backend/src/contract.rs:",
  );
  console.error(formatOwnership(unknownRoutes));
  process.exit(1);
}

const frontendInvokes = collectFrontendInvokes(jsFiles);
const tauriCommands = collectTauriCommands();
const unknownInvokes = new Map(
  [...frontendInvokes.entries()].filter(([command]) => !tauriCommands.has(command)),
);
if (unknownInvokes.size > 0) {
  console.error(
    "Frontend Tauri invoke commands are not registered in apps/nicecli/src-tauri/src/main.rs:",
  );
  console.error(formatOwnership(unknownInvokes));
  process.exit(1);
}

console.log(`JS syntax verified: ${jsFiles.length} files`);
console.log(`Frontend routes verified against Rust contract: ${frontendRoutes.size}`);
console.log(`Frontend Tauri invokes verified: ${frontendInvokes.size}`);
