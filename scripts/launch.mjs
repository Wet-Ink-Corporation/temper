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
//   (b) the npm package @wet-ink/temper-<platform>-<arch>, bin/temper[.exe] (D1), in the plugin
//       directory's own node_modules only, so a stray copy in an ancestor directory is never run;
//   (c) ${CLAUDE_PLUGIN_DATA}/engine/<key>/temper[.exe], where the fallback build installs it (§5.4,
//       Phase 0c). <key> is the plugin version for a cached copy and `dev` for a git checkout.
//
// No shell anywhere: the engine is spawned directly, with argv passed as an array.
import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
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
  // Absent when npm skipped it (unsupported platform, no npm, a failed install): fall through.
  const bin = path.join(pluginRoot, 'node_modules', '@wet-ink', `temper-${platform}-${arch}`, 'bin', exe);
  if (fs.existsSync(bin)) return bin;
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
    // A terminal's Ctrl-C reaches the engine twice (process group, then this forwarder); a default
    // SIGINT handler treats both the same.
    const forward = (signal) => () => { child.kill(signal); };
    const handlers = ['SIGINT', 'SIGTERM', 'SIGHUP'].map((signal) => [signal, forward(signal)]);
    for (const [signal, handler] of handlers) process.on(signal, handler);
    const done = () => { for (const [signal, handler] of handlers) process.off(signal, handler); };
    if (onStdout) child.stdout.on('data', onStdout);
    if (stdin !== 'inherit') {
      // A child that exits without reading its stdin must not raise an unhandled EPIPE.
      child.stdin.on('error', () => {});
      child.stdin.end(stdin);
    }
    child.once('error', (error) => { done(); reject(error); });
    // 'close', not 'exit': piped stdout may still hold data at 'exit'.
    child.once('close', (code) => { done(); resolve(code ?? 1); });
  });
  try {
    return await attempt();
  } catch (error) {
    if (error.code !== 'EACCES' || process.platform === 'win32' || !isFile(bin)) throw error;
    try {
      fs.chmodSync(bin, 0o755);
    } catch {
      throw error;
    }
    return attempt();
  }
}

function isFile(file) {
  try {
    return fs.statSync(file).isFile();
  } catch {
    return false;
  }
}

/**
 * The engine route once a binary resolves: 1.x's early warning below the Node minimum (#89),
 * unchanged, since the launcher itself still runs on Node, then the engine.
 */
export async function runEngine(bin, argv, { env, pluginRoot, version, nodeVersion, stdin = 'inherit', onStdout = null, writeStderr }) {
  const { earlyNodeCheck } = await import('./lib/requirements.mjs');
  // When `hook session-start` graduates, the stdin read here must be handed to the engine.
  const early = await earlyNodeCheck(nodeVersion, argv, env);
  if (early && early.stderr) writeStderr(early.stderr);
  if (early && early.stdout !== undefined) {
    await new Promise((done) => { process.stdout.write(early.stdout, done); });
    return early.exit;
  }
  return runBinary(bin, argv, { env, pluginRoot, version, stdin, onStdout });
}

async function readStdin() {
  if (process.stdin.isTTY) return '';
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString('utf8');
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
    const resolved = resolveBinary({ env: process.env, pluginRoot, version, platform: process.platform, arch: process.arch });
    const key = engineKey(pluginRoot, version);
    const fallback = resolved || argv[0] === 'hook' ? await import('./fallback.mjs') : null;
    // A dev engine is rebuilt at each session start (§5.3); cargo makes that a no-op if unchanged.
    const bin = resolved && fallback.sessionStartEngine({
      argv, env: process.env, pluginRoot, dataDir: process.env.CLAUDE_PLUGIN_DATA, key, platform: process.platform, bin: resolved,
    });
    if (bin) {
      try {
        return await runEngine(bin, argv, {
          env: process.env, pluginRoot, version, nodeVersion: process.versions.node,
          writeStderr: (text) => process.stderr.write(text),
        });
      } catch (error) {
        if (!error.syscall || !error.syscall.startsWith('spawn')) throw error;
        // A stale TEMPER_BIN or a pruned cache: say so in one line and fail open (exit 1).
        process.stderr.write(`temper: cannot run the engine at ${bin} (${error.code})\n`);
        return 1;
      }
    }
    if (argv[0] === 'hook') {
      // No engine for a graduated hook: the fallback build or the D6 policy (§5.4, Phase 0c).
      // The hook input is read once here, so an engine that becomes available still gets it.
      const stdin = await readStdin();
      // 1.x's Node check first, with the input already read, so an old Node never waits on a build.
      const { earlyNodeCheck } = await import('./lib/requirements.mjs');
      const early = await earlyNodeCheck(process.versions.node, argv, process.env, { readStdin: () => Promise.resolve(stdin) });
      const write = (text) => new Promise((done) => { process.stdout.write(text, done); });
      if (early && early.stdout !== undefined) {
        await write(early.stdout);
        return early.exit;
      }
      const r = fallback.withoutEngine({
        argv, env: process.env, stdin, pluginRoot, dataDir: process.env.CLAUDE_PLUGIN_DATA, key, platform: process.platform,
      });
      if (r.bin) {
        try {
          return await runBinary(r.bin, argv, { env: process.env, pluginRoot, version, stdin });
        } catch (error) {
          // Invariant 4: a hook that cannot run its engine is a notice with exit 0.
          await write(JSON.stringify({ systemMessage: `temper hook ${argv[1]} failed: cannot run the engine at ${r.bin} (${error.code ?? error.message})` }));
          return 0;
        }
      }
      await write(r.stdout);
      return r.exit;
    }
    // A command with no engine runs in 1.x while the port is under way.
  }
  // The 1.x CLI, in this process: it reads process.argv and stdin itself and sets the exit code.
  await import(pathToFileURL(path.join(pluginRoot, 'scripts', 'temper.mjs')).href);
  return null;
}

// Node realpaths the main module's URL but not argv[1], so compare real paths: a symlinked plugin
// directory (or macOS's /var -> /private/var) must still run main.
function isMain() {
  if (!process.argv[1]) return false;
  try {
    return fs.realpathSync(process.argv[1]) === fs.realpathSync(fileURLToPath(import.meta.url));
  } catch {
    return false;
  }
}

if (isMain()) {
  const code = await main();
  if (code !== null) process.exitCode = code;
}
