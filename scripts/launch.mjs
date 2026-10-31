#!/usr/bin/env node
// temper's 2.0 launcher (docs/plans/temper-2-rust-port.md §5.1), during the port.
//
// While the port is under way, hooks/hooks.json still runs scripts/temper.mjs, so users never reach
// this file. Maintainers dogfood the Rust engine with `node scripts/launch.mjs ...`, setting
// TEMPER_ENGINE=rust (and TEMPER_BIN to a local build). A subcommand graduates into RUST_READY when
// its parity cases pass on Linux, Windows and macOS (§3). Until then, and whenever no engine
// binary resolves, the launcher runs the 1.x CLI in-process, so argv and stdin are unchanged.
//
// The engine is found in this order (§5.1 step 3):
//   (a) TEMPER_BIN;
//   (b) the npm package @wet-ink/temper-<platform>-<arch>, bin/temper[.exe] (D1);
//   (c) ${CLAUDE_PLUGIN_DATA}/engine/<key>/temper[.exe], where the fallback build installs it (§5.4,
//       Phase 0c). <key> is the plugin version for a cached copy and `dev` for a git checkout.
//
// No shell anywhere: the engine is spawned directly, with argv passed as an array.
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

/** The subcommands the Rust engine handles, keyed as `hook <event>` or the first argument. */
export const RUST_READY = new Set(['--version']);

/** Subcommands that exist only in 2.0, so they go to the engine without TEMPER_ENGINE (D16(g)). */
const NEW_IN_2 = new Set(['--version']);

const keyOf = (argv) => (argv[0] === 'hook' ? `hook ${argv[1] ?? ''}` : (argv[0] ?? ''));

/**
 * Where a command line goes: `disabled` (TEMPER_DISABLE=1 on a hook: exit 0 at once), `engine`
 * (the Rust binary, when one resolves) or `node` (the 1.x CLI).
 */
export function route(argv, env) {
  if (argv[0] === 'hook' && env.TEMPER_DISABLE === '1') return 'disabled';
  const key = keyOf(argv);
  if (!RUST_READY.has(key)) return 'node';
  return NEW_IN_2.has(key) || env.TEMPER_ENGINE === 'rust' ? 'engine' : 'node';
}

/** The fallback engine's directory key: the version for an immutable cached copy, else `dev`. */
export function engineKey(pluginRoot, version) {
  return fs.existsSync(path.join(pluginRoot, '.git')) ? 'dev' : version;
}

/** The engine binary to run, or null when none is installed or built. */
export function resolveBinary({ env, pluginRoot, version, platform, arch }) {
  if (env.TEMPER_BIN) return env.TEMPER_BIN;
  const exe = platform === 'win32' ? 'temper.exe' : 'temper';
  try {
    const require = createRequire(path.join(pluginRoot, 'scripts', 'launch.mjs'));
    const manifest = require.resolve(`@wet-ink/temper-${platform}-${arch}/package.json`);
    const bin = path.join(path.dirname(manifest), 'bin', exe);
    if (fs.existsSync(bin)) return bin;
  } catch {
    // Not installed: npm skipped it (unsupported platform, no npm, a failed install). Fall through.
  }
  if (env.CLAUDE_PLUGIN_DATA) {
    const built = path.join(env.CLAUDE_PLUGIN_DATA, 'engine', engineKey(pluginRoot, version), exe);
    if (fs.existsSync(built)) return built;
  }
  return null;
}

/**
 * Run the engine and resolve to its exit code. A signal-killed engine gives 1, a non-blocking hook
 * error, so a hook fails open as in 1.x. On POSIX, a binary that lost its exec bit in transit
 * (EACCES) is made executable once and run again. `stdin` is a string to write, or `inherit`.
 */
export async function runBinary(bin, args, { env, pluginRoot, version, stdin = 'inherit', onStdout = null }) {
  const attempt = () => new Promise((resolve, reject) => {
    const child = spawn(bin, args, {
      stdio: [stdin === 'inherit' ? 'inherit' : 'pipe', onStdout ? 'pipe' : 'inherit', 'inherit'],
      windowsHide: true,
      env: { ...env, TEMPER_PLUGIN_ROOT: pluginRoot, TEMPER_PLUGIN_VERSION: version },
    });
    const forward = (signal) => () => { child.kill(signal); };
    const handlers = ['SIGINT', 'SIGTERM', 'SIGHUP'].map((signal) => [signal, forward(signal)]);
    for (const [signal, handler] of handlers) process.on(signal, handler);
    const done = () => { for (const [signal, handler] of handlers) process.off(signal, handler); };
    if (onStdout) child.stdout.on('data', onStdout);
    if (stdin !== 'inherit') child.stdin.end(stdin);
    child.once('error', (error) => { done(); reject(error); });
    child.once('exit', (code) => { done(); resolve(code ?? 1); });
  });
  try {
    return await attempt();
  } catch (error) {
    if (error.code !== 'EACCES' || process.platform === 'win32') throw error;
    fs.chmodSync(bin, 0o755);
    return attempt();
  }
}

async function drainStdin() {
  if (process.stdin.isTTY) return;
  for await (const chunk of process.stdin) void chunk;
}

async function main() {
  const argv = process.argv.slice(2);
  const pluginRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const where = route(argv, process.env);
  if (where === 'disabled') {
    await drainStdin();
    return 0;
  }
  if (where === 'engine') {
    const version = JSON.parse(fs.readFileSync(path.join(pluginRoot, '.claude-plugin', 'plugin.json'), 'utf8')).version;
    const bin = resolveBinary({ env: process.env, pluginRoot, version, platform: process.platform, arch: process.arch });
    if (bin) {
      const { earlyNodeCheck } = await import('./lib/requirements.mjs');
      // 1.x's warning below the Node minimum (#89), unchanged: the launcher itself still runs on Node.
      const early = await earlyNodeCheck(process.versions.node, argv, process.env);
      if (early && early.stderr) process.stderr.write(early.stderr);
      if (early && early.stdout !== undefined) {
        await new Promise((done) => { process.stdout.write(early.stdout, done); });
        return early.exit;
      }
      return runBinary(bin, argv, { env: process.env, pluginRoot, version });
    }
  }
  // The 1.x CLI, in this process: it reads process.argv and stdin itself and sets the exit code.
  await import(pathToFileURL(path.join(pluginRoot, 'scripts', 'temper.mjs')).href);
  return null;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const code = await main();
  if (code !== null) process.exitCode = code;
}
