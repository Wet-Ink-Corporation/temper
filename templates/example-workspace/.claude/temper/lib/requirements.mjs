// The documented minimum versions (README.md, Install) and the warnings for an older runtime.
// Message only: an old Node or Claude Code never changes a hook decision or an exit code.
//
// temper.mjs calls earlyNodeCheck before any other code runs, so this file must still load on a
// Node older than the minimum: plain syntax, no Node 17+ APIs, only node: built-ins.
import { spawnSync } from 'node:child_process';
import path from 'node:path';

/** The minimums the README's Install section documents. Keep the two in step (tests/messages.test.mjs checks). */
export const MINIMUM_VERSIONS = Object.freeze({ node: '18.0.0', claudeCode: '2.1.286' });

function parts(version) {
  const m = /^v?(\d+)\.(\d+)\.(\d+)/.exec(typeof version === 'string' ? version.trim() : '');
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/** True when `version` is below `minimum`. An unparsable version is unknown, never older. */
export function isOlder(version, minimum) {
  const v = parts(version);
  const m = parts(minimum);
  if (!v || !m) return false;
  for (let i = 0; i < 3; i++) if (v[i] !== m[i]) return v[i] < m[i];
  return false;
}

/** The version in `claude --version` output ("2.1.292 (Claude Code)"), else null. */
export function parseClaudeCodeVersion(stdout) {
  const m = /^\s*v?(\d+\.\d+\.\d+)\S*\s+\(Claude Code\)/m.exec(typeof stdout === 'string' ? stdout : '');
  return m ? m[1] : null;
}

/**
 * The running Claude Code's version, or null when it cannot be learnt. Hook input carries no
 * version, and CLAUDE_CODE_VERSION is not set by Claude Code (other tools set it, to other
 * values), so this asks the binary Claude Code names in CLAUDE_CODE_EXECPATH. That is one
 * bounded process spawn (about 15 ms); anything that is not a `claude` binary is not run.
 */
export function claudeCodeVersion(env = process.env, spawn = spawnSync) {
  const exe = env.CLAUDE_CODE_EXECPATH;
  if (!exe || !/^claude(\.exe)?$/i.test(path.basename(exe))) return null;
  let r;
  try {
    r = spawn(exe, ['--version'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], timeout: 2000, windowsHide: true });
  } catch {
    return null;
  }
  if (!r || r.status !== 0) return null;
  return parseClaudeCodeVersion(r.stdout);
}

const nodeWarning = (v) => `temper: Node ${v} is older than the documented minimum, Node ${MINIMUM_VERSIONS.node} (README, Install). temper's hooks and CLI may fail on it and check nothing. Put Node ${MINIMUM_VERSIONS.node.split('.')[0]} or later on the PATH that Claude Code sees.`;
const claudeWarning = (v) => `temper: Claude Code ${v} is older than the documented minimum, Claude Code ${MINIMUM_VERSIONS.claudeCode} (README, Install). temper is untested on it, and a hook it does not run checks nothing. Update Claude Code.`;

/** One warning line per runtime below its documented minimum. Unknown versions say nothing. */
export function runtimeWarnings({ node, claudeCode } = {}) {
  const out = [];
  if (isOlder(node, MINIMUM_VERSIONS.node)) out.push(nodeWarning(node));
  if (isOlder(claudeCode, MINIMUM_VERSIONS.claudeCode)) out.push(claudeWarning(claudeCode));
  return out;
}

/**
 * What temper.mjs does before anything else on a Node below the minimum, or null to carry on.
 * `hook session-start` prints the warning for the human (systemMessage) and the model
 * (additionalContext) and exits 0, since the rest of temper may not load. Other hooks carry on
 * unchanged, so the warning comes once per session. Commands warn on stderr and carry on, keeping
 * their own output and exit code. TEMPER_DISABLE=1 stays silent.
 */
export function earlyNodeCheck(node, argv, env) {
  if (!isOlder(node, MINIMUM_VERSIONS.node) || (env && env.TEMPER_DISABLE === '1')) return null;
  const msg = nodeWarning(node);
  if (argv[0] === 'hook') {
    if (argv[1] !== 'session-start') return null;
    return { stdout: JSON.stringify({ systemMessage: msg, hookSpecificOutput: { hookEventName: 'SessionStart', additionalContext: msg } }), exit: 0 };
  }
  return { stderr: `${msg}\n` };
}
