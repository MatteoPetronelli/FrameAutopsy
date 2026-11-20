#!/usr/bin/env node
// Project hook (PostToolUse Edit|Write|MultiEdit): format the edited file with the
// formatter the project already uses. Silent on success; never blocks; never installs anything.
// Installed by the claude-repo-setup skill. Disable: CLAUDE_FORMAT_HOOK=off
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";

function* ancestors(dir) {
  let cur = resolve(dir);
  while (true) {
    yield cur;
    const up = dirname(cur);
    if (up === cur) return;
    cur = up;
  }
}
const nearest = (file, name) => {
  for (const d of ancestors(dirname(file))) if (existsSync(join(d, name))) return join(d, name);
  return null;
};
// no shell: file paths with spaces stay one argument
const run = (cmd, args, cwd) => spawnSync(cmd, args, { cwd, encoding: "utf8", timeout: 20000 });

function formatterFor(file) {
  const ext = extname(file).toLowerCase();
  if (ext === ".rs") {
    const toml = nearest(file, "Cargo.toml");
    let edition = "2021";
    for (const d of toml ? ancestors(dirname(toml)) : []) {
      const m = existsSync(join(d, "Cargo.toml")) && readFileSync(join(d, "Cargo.toml"), "utf8").match(/^\s*edition\s*=\s*"(\d{4})"/m);
      if (m) { edition = m[1]; break; }
    }
    return ["rustfmt", ["--edition", edition, file]];
  }
  if ([".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".svelte", ".vue", ".css", ".scss", ".json", ".md", ".html"].includes(ext)) {
    // only if the project has prettier; run its JS entry via node (.cmd shims need a shell on Windows)
    const bin = nearest(file, join("node_modules", "prettier", "bin", "prettier.cjs"));
    return bin ? [process.execPath, [bin, "--write", "--log-level", "warn", file]] : null;
  }
  if (ext === ".py") return ["ruff", ["format", "--quiet", file]];
  if (ext === ".go") return ["gofmt", ["-w", file]];
  return null;
}

async function main() {
  if ((process.env.CLAUDE_FORMAT_HOOK || "").toLowerCase() === "off") return;
  let raw = "";
  for await (const c of process.stdin) raw += c;
  const file = JSON.parse(raw || "{}")?.tool_input?.file_path;
  if (!file || !existsSync(file)) return;
  const f = formatterFor(file);
  if (!f) return;
  const r = run(f[0], f[1], dirname(file));
  if (r.error || r.status === 0) return; // formatter missing, or success
  const msg = `${r.stderr || r.stdout}`.split(/\r?\n/).filter(Boolean).slice(0, 20).join("\n");
  process.stdout.write(JSON.stringify({
    hookSpecificOutput: { hookEventName: "PostToolUse", additionalContext: `formatter could not format ${file}:\n${msg}` },
  }));
}

main().catch(() => process.exit(0));
