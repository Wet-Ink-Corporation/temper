// One reader of shell command lines for the preBash rules that look at what a command
// writes or deletes: the state-dir deny, the protected-config ask and the scan of copies and patches
// onto Rust sources. It reads quotes, escapes, expansions, heredocs, operators, subshells,
// compound-command keywords, wrappers (sudo, env, xargs, ...) and `cd`, and reports the
// files each simple command writes. It never runs anything and reads no files: a rule that
// needs the disk (a patch's text, whether a path is a directory) reads it itself.
import os from 'node:os';
import path from 'node:path';

// ---------- lexer ----------

// Outside quotes a backslash escapes the next character. Before a letter, digit, `_` or `-`
// the lexer keeps it as its own part (`bs`): bash drops it (`c\p` runs cp, `tem\per` is
// `temper`), and a PowerShell command reads it as a path separator (`.git\temper\verdicts`),
// so makeWord decides once the command is known.
const KEEP_BACKSLASH = /[\w-]/;
// Characters inside double quotes with no special meaning there.
const DQ_PLAIN = /[^"\\$`]*/y;
const REDIRECT = /^(?:&>>|&>|>>|>\||>&|>|<<<|<<-|<<|<&|<>|<)/;
const OPERATOR = /^(?:;;&|;;|;&|&&|\|\||\|&|[;&|()])/;
/** Redirection operators that write their target. */
const WRITE_REDIRECTS = new Set(['>', '>>', '>|', '&>', '&>>', '>&', '<>']);

/** Index of the `)` (or `close`) that balances the opener at `open`, skipping quoted text; s.length when unbalanced. */
function balanced(s, open, openCh = '(', closeCh = ')') {
  let depth = 0;
  for (let i = open; i < s.length; i++) {
    const c = s[i];
    if (c === '\\') { i++; continue; }
    if (c === "'") { const e = s.indexOf("'", i + 1); if (e === -1) return s.length; i = e; continue; }
    if (c === '"') {
      let j = i + 1;
      while (j < s.length && s[j] !== '"') j += s[j] === '\\' ? 2 : 1;
      i = j;
      continue;
    }
    if (c === openCh) depth++;
    else if (c === closeCh && --depth === 0) return i;
  }
  return s.length;
}

/** The expansion that starts at s[i] (`$`, a backquote): [part, index after it], or null for a lone `$`. */
function expansion(s, i, quoted) {
  if (s[i] === '`') {
    let j = i + 1;
    let src = '';
    while (j < s.length && s[j] !== '`') {
      if (s[j] === '\\' && '`$\\'.includes(s[j + 1] ?? '')) { src += s[j + 1]; j += 2; continue; }
      src += s[j++];
    }
    return [{ t: 'sub', src, s: s.slice(i, j + 1), q: quoted }, Math.min(j + 1, s.length)];
  }
  const nx = s[i + 1];
  if (nx === '(') {
    if (s[i + 2] === '(') {
      const e = balanced(s, i + 1);
      return [{ t: 'arith', s: s.slice(i, e + 1), q: quoted }, e + 1];
    }
    const e = balanced(s, i + 1);
    return [{ t: 'sub', src: s.slice(i + 2, e), s: s.slice(i, e + 1), q: quoted }, e + 1];
  }
  if (nx === '{') {
    const e = balanced(s, i + 1, '{', '}');
    const inner = s.slice(i + 2, e);
    const p = /^[A-Za-z_]\w*$/.test(inner) ? { t: 'var', name: inner } : { t: 'param' };
    return [{ ...p, s: s.slice(i, e + 1), q: quoted }, e + 1];
  }
  // PowerShell's `$env:NAME` is one variable.
  const name = /^(?:env:[A-Za-z_]\w*|[A-Za-z_]\w*|[0-9@*#?$!-])/i.exec(s.slice(i + 1, i + 256));
  if (name) return [{ t: 'var', name: name[0], s: `$${name[0]}`, q: quoted }, i + 1 + name[0].length];
  return null;
}

// The escapes of ANSI-C quoting (`$'...'`) that stand for one character.
const ANSI_C = { a: '\x07', b: '\b', e: '\x1b', E: '\x1b', f: '\f', n: '\n', r: '\r', t: '\t', v: '\v', '\\': '\\', "'": "'", '"': '"', '?': '?' };

/**
 * The text of a `$'...'` whose body starts at s[j], decoded as bash decodes it, and the index of
 * its closing quote. The text is null when an escape is one temper does not decode exactly: `\c`,
 * a NUL (bash ends the word there), or a character outside ASCII (it depends on the locale).
 */
function ansiC(s, j) {
  let text = '';
  let exact = true;
  while (j < s.length && s[j] !== "'") {
    if (s[j] !== '\\' || j + 1 >= s.length) { text += s[j++]; continue; }
    const e = s[j + 1];
    if (ANSI_C[e] !== undefined) { text += ANSI_C[e]; j += 2; continue; }
    const m = /^(?:[0-7]{1,3}|x[0-9a-fA-F]{1,2}|u[0-9a-fA-F]{1,4}|U[0-9a-fA-F]{1,8})/.exec(s.slice(j + 1, j + 10));
    if (m) {
      const code = /^[0-7]/.test(m[0]) ? parseInt(m[0], 8) : parseInt(m[0].slice(1), 16);
      if (code === 0 || code > 0x7f) exact = false;
      else text += String.fromCharCode(code);
      j += 1 + m[0].length;
      continue;
    }
    if ('cxuU'.includes(e)) exact = false;
    else text += `\\${e}`; // bash keeps an escape it does not know as written
    j += 2;
  }
  return [exact ? text : null, j];
}

/**
 * The simple commands of a command line as written: `{ words, redirects, sep, after, raw }`
 * entries, with `{ open: true }` / `{ close: true }` for a subshell's parentheses. A word is
 * `{ parts }`: literal text (`lit`, with `q` when quoted), `var`, `param` (`${x:-y}`, or a
 * `$'...'` with an escape temper does not decode), `sub`
 * (`$(...)` or backquotes, with its `src`), `arith` and `proc` (`<(...)`). A redirect is
 * `{ op, fd, word }`; a heredoc's also carries `body` and `quoted` (its delimiter was quoted).
 * `sep` and `after` are the operators before and after the command (`|`, `&&`, `;`, newline...).
 */
function lex(src) {
  const s = String(src);
  const n = s.length;
  const out = [];
  let i = 0;
  let cmd = null;
  let word = null;
  let redir = null;
  let sep = '';
  let heredocs = [];
  const bodies = []; // [start, end) of each heredoc body
  const ensure = (at) => (cmd ??= { words: [], redirects: [], sep, after: '', start: at });
  const startWord = () => (word ??= { parts: [], start: i });
  const lit = (text, q) => {
    const w = startWord();
    const last = w.parts.at(-1);
    if (last?.t === 'lit' && last.q === q && !last.bs) last.s += text;
    else w.parts.push({ t: 'lit', s: text, q });
  };
  const endWord = () => {
    if (!word) return;
    const w = word;
    word = null;
    ensure(w.start);
    if (redir) {
      const r = { ...redir, word: w };
      redir = null;
      if (r.op === '<<' || r.op === '<<-') {
        r.delim = w.parts.filter((p) => !p.bs).map((p) => p.s).join(''); // `<<\EOF` is a quoted EOF
        r.quoted = w.parts.some((p) => p.q || p.bs);
        r.body = null;
        heredocs.push(r);
      }
      cmd.redirects.push(r);
    } else cmd.words.push(w);
  };
  const endCmd = (op, at) => {
    endWord();
    redir = null;
    if (cmd) {
      cmd.after = op;
      cmd.raw = s.slice(cmd.start, at).trim();
      out.push(cmd);
    }
    cmd = null;
    sep = op;
  };
  while (i < n) {
    const c = s[i];
    if (c === '\\') {
      if (s[i + 1] === '\n') { i += 2; continue; }
      if (s[i + 1] === '\r' && s[i + 2] === '\n') { i += 3; continue; }
      if (i + 1 < n && !KEEP_BACKSLASH.test(s[i + 1])) { lit(s[i + 1], true); i += 2; continue; }
      if (i + 1 < n) { startWord().parts.push({ t: 'lit', s: '\\', q: false, bs: true }); i++; continue; }
      lit('\\', false);
      i++;
      continue;
    }
    if (c === "'") {
      const e = s.indexOf("'", i + 1);
      const end = e === -1 ? n : e;
      lit(s.slice(i + 1, end), true);
      i = end + 1;
      continue;
    }
    if (c === '"') {
      startWord();
      lit('', true);
      let j = i + 1;
      while (j < n && s[j] !== '"') {
        if (s[j] === '\\' && j + 1 < n && '"\\$`\n'.includes(s[j + 1])) { if (s[j + 1] !== '\n') lit(s[j + 1], true); j += 2; continue; }
        if (s[j] === '$' || s[j] === '`') {
          const x = expansion(s, j, true);
          if (x) { word.parts.push(x[0]); j = x[1]; continue; }
        }
        // A run of plain characters at once, so a long string stays fast.
        DQ_PLAIN.lastIndex = j + 1;
        const end = DQ_PLAIN.test(s) ? DQ_PLAIN.lastIndex : j + 1;
        lit(s.slice(j, end), true);
        j = end;
      }
      i = j + 1;
      continue;
    }
    if (c === '$' && s[i + 1] === "'") { // ANSI-C quoting
      const [text, j] = ansiC(s, i + 2);
      if (text === null) startWord().parts.push({ t: 'param', s: s.slice(i, j + 1), q: true });
      else lit(text, true);
      i = j + 1;
      continue;
    }
    if (c === '$' || c === '`') {
      const x = expansion(s, i, false);
      if (x) { startWord().parts.push(x[0]); i = x[1]; continue; }
      lit('$', false);
      i++;
      continue;
    }
    if (c === '#' && !word) {
      while (i < n && s[i] !== '\n') i++;
      continue;
    }
    if (c === '\n') {
      endCmd('\n', i);
      i++;
      for (const h of heredocs) {
        const lines = [];
        const from = i;
        for (;;) {
          if (i >= n) break;
          const e = s.indexOf('\n', i);
          const end = e === -1 ? n : e;
          const line = s.slice(i, end);
          i = end + 1;
          if ((h.op === '<<-' ? line.replace(/^\t+/, '') : line).replace(/\r$/, '') === h.delim) { h.closed = true; break; }
          lines.push(h.op === '<<-' ? line.replace(/^\t+/, '') : line);
        }
        h.body = lines.length ? `${lines.join('\n')}\n` : '';
        bodies.push([from, Math.min(i, n)]);
      }
      heredocs = [];
      continue;
    }
    if (c === ' ' || c === '\t' || c === '\r') { endWord(); i++; continue; }
    if ((c === '<' || c === '>') && s[i + 1] === '(') {
      const e = balanced(s, i + 1);
      startWord().parts.push({ t: 'proc', src: s.slice(i + 2, e), s: s.slice(i, e + 1) });
      i = e + 1;
      continue;
    }
    const r = REDIRECT.exec(s.slice(i, i + 3));
    if (r) {
      let fd = null;
      if (word && word.parts.length === 1 && word.parts[0].t === 'lit' && !word.parts[0].q && /^\d+$/.test(word.parts[0].s)) {
        fd = Number(word.parts[0].s);
        ensure(word.start);
        word = null;
      } else endWord();
      ensure(i);
      i += r[0].length;
      // `2>&1`, `>&-`, `<&3`: a descriptor, not a file.
      const dup = /^[ \t]*(?:\d+|-)(?=$|[\s;&|<>()])/.exec(s.slice(i));
      if ((r[0] === '>&' || r[0] === '<&') && dup) { i += dup[0].length; continue; }
      redir = { op: r[0], fd };
      continue;
    }
    const op = OPERATOR.exec(s.slice(i, i + 3));
    if (op) {
      if (op[0] === '(' && !cmd && !word && s[i + 1] === '(') { // `(( arithmetic ))`
        const e = balanced(s, i);
        i = e + 1;
        continue;
      }
      endCmd(op[0], i);
      i += op[0].length;
      if (op[0] === '(') out.push({ open: true });
      else if (op[0] === ')') out.push({ close: true });
      continue;
    }
    lit(c, false);
    i++;
  }
  endCmd('', n);
  out.bodies = bodies;
  return out;
}

// ---------- words ----------

/**
 * A word as the rules see it: `raw` (quotes removed, expansions as written), `text` (known
 * expansions replaced, unknown ones as written), `value` (the text, or null when any part is
 * unknown), `literal` (no expansion, brace list or glob at all: the text is exactly what the
 * shell passes), `glob` (an unquoted `*`, `?`, `[...]` or brace list), `quoted` and its `parts`.
 */
function makeWord(parts, env = {}) {
  let raw = '';
  let text = '';
  let known = true;
  let glob = false;
  let bracket = -1; // where an unquoted `[` is in raw: a glob when a `]` follows it (`[` alone is the test command)
  parts.forEach((p, k) => {
    if (p.bs && !env.ps) return; // bash removes the escaping backslash
    raw += p.s;
    if (p.t === 'lit') {
      let s = p.s;
      if (!p.q && k === 0 && /^~(?=\/|\\|$)/.test(s) && env.home) s = env.home + s.slice(1);
      else if (!p.q && k === 0 && s.startsWith('~')) known = false;
      if (!p.q && (/[*?]/.test(s) || braceList(s))) glob = true;
      if (!p.q && bracket === -1 && p.s.includes('[')) bracket = raw.length - p.s.length + p.s.indexOf('[');
      text += s;
      return;
    }
    const v = partValue(p, env);
    if (v === null) { known = false; text += p.s; } else text += v;
  });
  if (bracket !== -1 && raw.includes(']', bracket + 1)) glob = true;
  const assign = parts[0]?.t === 'lit' && !parts[0].q ? /^[A-Za-z_]\w*(?=\+?=)/.exec(parts[0].s)?.[0] : undefined;
  // An unquoted `{` may open a brace list, one that spans a quoted part too (`{'git apply x',y}`).
  const literal = parts.every((p) => p.t === 'lit') && !glob && !parts.some((p) => !p.q && p.s.includes('{'));
  return { raw, text, value: known ? text : null, literal, glob, quoted: parts.some((p) => p.q), assign, parts };
}

/** An unquoted brace list (`{a,b}`, `{1..3}`), found in one pass. */
function braceList(s) {
  for (const m of s.matchAll(/\{([^{}]*)\}/g)) if (m[1].includes(',') || m[1].includes('..')) return true;
  return false;
}

/** A word whose value is unknown (xargs input, find's `{}`). */
const unknownWord = (raw) => ({ raw, text: raw, value: null, literal: false, glob: false, quoted: false });

/** `git rev-parse --git-dir` and friends inside `$(...)`, which name directories temper knows. */
function gitRevParse(src, env) {
  const m = /^\s*git\b(?:\s+-C\s+\S+)?\s+rev-parse\b(.*)$/s.exec(src);
  if (!m) return null;
  const args = m[1].replace(/(?:^|\s)--path-format=(?:absolute|relative)(?=\s|$)/g, ' ').trim();
  const gitPath = /--git-path\s+["']?([^\s"'`)]+)["']?\s*$/.exec(args);
  if (gitPath) return `${env.gitDir ?? '.git'}/${gitPath[1]}`;
  if (/^(?:--(?:git-common-dir|git-dir|absolute-git-dir))$/.test(args)) return env.gitDir ?? '.git';
  if (/^--show-toplevel$/.test(args)) return env.top ?? null;
  return null;
}

// Variables that name the temp directory, and a `mktemp` that makes its file or directory there.
const TEMP_VARS = /^(?:TMPDIR|TMP|TEMP|RUNNER_TEMP|env:(?:TMP|TEMP))$/i;
const MKTEMP = /^\s*mktemp((?:\s+(?:-[dqut]+|--(?:directory|quiet|dry-run|tmpdir)))*)(?:\s+([^\s/\\]+))?\s*$/;
function mktemp(src, env) {
  const m = MKTEMP.exec(src);
  // A template without -t or --tmpdir is made in the current directory.
  if (!m || (m[2] && !/(?:^|\s)(?:-[dqu]*t[dqu]*|--tmpdir)(?=\s|$)/.test(m[1]))) return null;
  return env.tmp ? path.join(env.tmp, 'tmp.XXXXXXXXXX') : null;
}

function partValue(p, env) {
  if (p.t === 'var') {
    if (env.vars?.has(p.name)) return env.vars.get(p.name);
    if (p.name === 'HOME') return env.home ?? null;
    if (p.name === 'PWD') return env.cwd ?? null;
    if (p.name === 'OLDPWD') return env.prev ?? null;
    if (TEMP_VARS.test(p.name)) return env.tmp ?? null;
    return null;
  }
  if (p.t === 'sub') return gitRevParse(p.src, env) ?? mktemp(p.src, env);
  return null;
}

/** A git-bash path (`/c/Users/...`, `/tmp/...`) as the Windows path it names; other paths unchanged. */
export function msysPath(t, platform = process.platform) {
  if (platform !== 'win32') return t;
  const drive = /^\/([A-Za-z])(\/.*)?$/.exec(t);
  if (drive) return `${drive[1].toUpperCase()}:${drive[2] ?? '/'}`;
  const tmp = /^\/tmp(\/.*)?$/.exec(t);
  if (tmp) return `${os.tmpdir().replace(/\\/g, '/')}${tmp[1] ?? ''}`;
  return t;
}

/** The absolute path `value` names from `base`, or null when it is unknown, empty, or relative to an unknown directory. */
export function resolvePath(value, base, platform = process.platform) {
  if (value === null || value === undefined || value === '') return null;
  const t = msysPath(value, platform);
  if (path.isAbsolute(t)) return path.resolve(t);
  return base ? path.resolve(base, t) : null;
}

/** The absolute path a word names when the command runs in `cwd`; null for a glob or an unknown word. */
export const wordPath = (w, cwd) => (w && !w.glob ? resolvePath(w.value, cwd) : null);

// ---------- commands ----------

const KEYWORDS = new Set(['if', 'then', 'else', 'elif', 'fi', 'do', 'done', 'while', 'until', '!', '{', '}', 'esac']);
// Wrappers that run the command after them, with their options that take a separate value.
const WRAPPERS = new Map([
  ['sudo', ['-u', '-g', '-C', '-D', '-h', '-p', '-r', '-t', '-U', '-T', '--user', '--group', '--chdir', '--prompt']],
  ['doas', ['-u', '-C']], ['env', ['-u', '-C', '-S', '--unset', '--chdir', '--split-string']], ['command', []], ['builtin', []],
  ['exec', ['-a']], ['nohup', []], ['time', ['-f', '-o', '--format', '--output']], ['nice', ['-n', '--adjustment']],
  ['ionice', ['-c', '-n', '-p']], ['stdbuf', ['-i', '-o', '-e']], ['timeout', ['-s', '-k', '--signal', '--kill-after']],
  ['xargs', ['-a', '-d', '-E', '-I', '-L', '-n', '-P', '-s', '--arg-file', '--delimiter', '--max-args', '--max-procs', '--replace']],
  ['chronic', []], ['unbuffer', []], ['setsid', []], ['busybox', []],
  ['watch', ['-n', '--interval', '-q', '--equexit']], // without -x, watch runs its words with sh -c
]);
const SHELLS = new Set(['bash', 'sh', 'zsh', 'dash', 'ksh']);
const CD = new Set(['cd', 'pushd', 'popd', 'chdir', 'set-location', 'sl']);
const DECLARE = new Set(['export', 'declare', 'local', 'readonly', 'typeset']);
const MAX_DEPTH = 4;

/** A command name as the rules compare it: basename, lower case, no `.exe`; GNU patch's `gpatch` is `patch`. */
const commandName = (v) => String(v ?? '').replace(/\\/g, '/').split('/').pop().toLowerCase().replace(/\.exe$/, '').replace(/^gpatch$/, 'patch');

/**
 * The simple commands `cmd` runs, in order, each as `{ name, argv0, args, redirects, stdin,
 * piped, cwd, raw, line, via }`. `args` are words (see makeWord) after leading reserved
 * words (`if`, `then`, `do`, `{`, `!`, ...), `VAR=value` prefixes and wrappers (`sudo`,
 * `env`, `timeout 5`, `xargs`, ...). `cwd` is the absolute directory the command runs in,
 * following `cd`, `pushd` and `popd` (a subshell's or pipeline's `cd` ends with it), or null
 * after a `cd` temper cannot follow. Shell variables assigned earlier are expanded; `~`,
 * `$HOME`, `$PWD` and `$(git rev-parse --git-dir|--git-common-dir|--git-path p|--show-toplevel)`
 * are known. Commands inside `bash -c`, `sh -c`, `eval`, `pwsh -Command`, `find -exec`,
 * `$(...)` and a script fed to a shell on stdin are included, with `via` saying how.
 * `stdin` is `{ kind: 'file'|'heredoc'|'string'|'pipe', word?, body?, literal?, exact? }`: a
 * heredoc's or here-string's `literal` is its body when it has no expansion (a quoted heredoc
 * delimiter, or none written), else null; a pipe's `body` is a heredoc or here-string body an
 * earlier stage read, `exact` when that body was literal and that stage was a plain `cat`. A
 * command run under xargs has `xargs: true`. Past the nesting temper follows (MAX_DEPTH), one
 * command with `opaque: 'depth'` and no name stands for what is not read.
 *
 * `$TMPDIR`, `$TMP`, `$TEMP`, `$env:TEMP` and `$(mktemp [-d])` name the temp directory.
 *
 * Each command's `named` says whether temper reads its name exactly (see readName).
 *
 * Options: `cwd` (the starting directory), `home`, `top` (`--show-toplevel`), `gitDir`, `tmp`, and
 * `untracked`, under which a variable the command assigns is unknown rather than its model's value
 * (#151 review 7: `read`, `printf -v`, arrays and assignments that never run are not modelled), and
 * a command whose name or wrapper words temper does not read exactly is reported as one with
 * `opaque: 'name'` and no name, whatever the model makes of it (`cd`, `export`, `timeout $T`).
 */
export function parseCommands(cmd, opts = {}) {
  const env = {
    cwd: opts.cwd ? path.resolve(opts.cwd) : null, prev: null, home: opts.home ?? os.homedir(),
    top: opts.top ?? null, gitDir: opts.gitDir ?? '.git', tmp: opts.tmp ?? os.tmpdir(), vars: new Map(), dirs: [],
    untracked: Boolean(opts.untracked), whole: String(cmd),
  };
  const out = [];
  run(String(cmd), env, out, 0, undefined);
  return out;
}

function run(src, env, out, depth, via) {
  if (depth > MAX_DEPTH) { out.push(depthMarker(src, env, via)); return; }
  const items = lex(src);
  // The line as code, without its heredoc bodies (a PowerShell expression is read from it).
  let line = src;
  for (const [a, b] of [...items.bodies].reverse()) line = line.slice(0, a) + line.slice(b);
  const scopes = [];
  let prevBody = null;
  let prevExact = false;
  for (const it of items) {
    if (it.open) { scopes.push({ cwd: env.cwd, prev: env.prev, dirs: env.dirs.length, vars: new Map(env.vars) }); continue; }
    if (it.close) {
      const o = scopes.pop();
      if (o) { env.cwd = o.cwd; env.prev = o.prev; env.dirs.length = Math.min(env.dirs.length, o.dirs); env.vars = o.vars; }
      continue;
    }
    // Command substitutions run first, in a subshell.
    for (const w of it.words.concat(it.redirects.map((r) => r.word).filter(Boolean))) {
      for (const p of w.parts) if ((p.t === 'sub' || p.t === 'proc') && p.src.trim() && !(p.t === 'sub' && gitRevParse(p.src, env) !== null)) sub(p.src, env, out, depth + 1, 'subst');
    }
    // A PowerShell command reads a kept backslash as a path separator; bash drops it (see lex).
    const ps = Boolean(env.ps) || psCommand(it.words, env);
    const wenv = ps === Boolean(env.ps) ? env : { ...env, ps };
    const piped = it.sep === '|' || it.sep === '|&';
    const isolated = piped || it.after === '|' || it.after === '|&' || it.after === '&';
    const redirects = it.redirects.map((r) => ({ op: r.op, fd: r.fd, word: r.word ? makeWord(r.word.parts, wenv) : null, body: r.body, quoted: r.quoted }));
    let stdin = piped ? { kind: 'pipe', body: prevBody, exact: prevExact } : null;
    for (const r of redirects) {
      if (r.op === '<' && r.word) stdin = { kind: 'file', word: r.word };
      else if (r.op === '<<' || r.op === '<<-') stdin = { kind: 'heredoc', body: r.body ?? null, literal: r.body !== null && r.body !== undefined && (r.quoted || !/[$`]|\\[\\$`\n]/.test(r.body)) ? r.body : null };
      else if (r.op === '<<<' && r.word) stdin = { kind: 'string', word: r.word, body: r.word.text, literal: r.word.literal ? r.word.text : null };
    }
    prevBody = stdin && stdin.kind !== 'pipe' && stdin.kind !== 'file' ? stdin.body : null;
    const words = it.words.map((w) => makeWord(w.parts, wenv));
    // A body piped on is the next stage's script exactly only when it is literal and `cat` passes it through unchanged.
    prevExact = prevBody !== null && stdin.literal !== null && stdin.literal !== undefined && catsStdin(words);
    simple(words, { redirects, stdin, piped, isolated, raw: it.raw, line, via }, env, out, depth);
  }
}

// PowerShell cmdlets (Verb-Noun with a common verb) and the aliases the write and delete rules read.
const PS_NAME = /^(?:(?:get|set|new|remove|copy|move|add|clear|out|write|read|invoke|start|stop|test|import|export|select|where|foreach|rename|join|split|resolve|convert|convertto|convertfrom|format|measure|tee|push|pop|expand|compress|update|enter|sort|group|compare)-[a-z]+|cpi|mi|sc|ac|ni|sl|copy|move|ri|del|erase|rd)$/;

/** Whether a simple command is PowerShell: a cmdlet name, `$x = ...` or a `[Type]::Method(...)` expression. */
function psCommand(words, env) {
  const ws = words.map((w) => makeWord(w.parts, { ...env, ps: false }));
  const k = ws.findIndex((w) => !w.assign);
  if (k === -1) return false;
  const first = ws[k];
  if (/^\[[\w.]+\]::/.test(first.raw) || (/^\$\w+$/.test(first.raw) && ws[k + 1]?.raw === '=')) return true;
  return first.value !== null && PS_NAME.test(commandName(first.value));
}

/** Whether a stage is `cat` with no operand but `-`: it passes its stdin on unchanged. */
function catsStdin(words) {
  let i = 0;
  while (i < words.length && isAssign(words[i])) i++;
  return words[i]?.value === 'cat' && words.slice(i + 1).every((w) => w.value === '-');
}

/** The command that stands for what is nested deeper than temper reads, or (`opaque: 'name'`) a command name it does not read. */
const depthMarker = (src, env, via, opaque = 'depth', argv0 = unknownWord('')) => ({
  name: null, argv0, args: [], redirects: [], stdin: null, piped: false, cwd: null,
  raw: String(src).trim(), line: String(src), via, envNames: [], opaque, named: false, whole: env.whole,
});

function sub(src, env, out, depth, via) {
  const saved = { cwd: env.cwd, prev: env.prev, dirs: env.dirs.length, vars: env.vars };
  env.vars = new Map(env.vars);
  run(src, env, out, depth, via);
  env.cwd = saved.cwd;
  env.prev = saved.prev;
  env.dirs.length = Math.min(env.dirs.length, saved.dirs);
  env.vars = saved.vars;
}

// Shell options that take the next word as their value.
const SHELL_VALUED = new Set(['-o', '+o', '-O', '+O', '--rcfile', '--init-file']);

/**
 * Where a shell (`bash`, `sh`, ...) reads its script, from its arguments: `{ from: 'arg', word }`
 * for `-c` (the first operand after the options, which may follow `-c`; no word when there is
 * none), `{ from: 'file', word }` for a script operand, `{ from: 'stdin' }` for `-s` or no
 * operand, or `{ from: 'none' }` for `--version` and `--help`. `--` or `-` ends the options, and
 * `-o` or `-O`, alone or at the end of a cluster (`-euo pipefail`), takes the next word.
 */
function shellScript(args) {
  let stdin = false;
  let cmd = false;
  const operand = (w) => (cmd ? { from: 'arg', word: w } : stdin || !w ? { from: 'stdin' } : { from: 'file', word: w });
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a.value === '--version' || a.value === '--help') return { from: 'none' };
    if (a.value === '--' || a.value === '-') return operand(args[i + 1]);
    if (a.value === null || !/^[-+]/.test(a.raw)) return operand(a);
    if (SHELL_VALUED.has(a.value)) { i++; continue; }
    if (a.value.startsWith('--')) continue;
    if (/^-[A-Za-z]*c/.test(a.value)) cmd = true;
    if (/^-[A-Za-z]*s/.test(a.value)) stdin = true;
    i += (a.value.slice(1).match(/[oO]/g) ?? []).length;
  }
  return cmd ? { from: 'arg', word: undefined } : { from: 'stdin' };
}

const isAssign = (w) => Boolean(w.assign);
// A variable's new value; unknown (null) when nothing is known, or when the parse is `untracked`.
const setVar = (env, name, value) => {
  if (env.untracked) env.vars.set(name, null);
  else if (value === null) env.vars.delete(name);
  else env.vars.set(name, value);
};
const assignTo = (env, w) => {
  if (w.value?.[w.assign.length] === '+') { // `X+=y` appends, to a value temper may not know
    const old = env.vars.get(w.assign);
    setVar(env, w.assign, typeof old === 'string' ? old + w.value.slice(w.assign.length + 2) : null);
  } else setVar(env, w.assign, w.value === null ? null : w.value.slice(w.assign.length + 1));
};

// The index of the first word at or after `i` that is not a reserved word.
const skipKeywords = (w, i) => {
  while (i < w.length && KEYWORDS.has(w[i].raw) && !w[i].quoted) i++;
  return i;
};

function simple(words, meta, env, out, depth) {
  let w = words;
  // The variables the command's environment sets: `VAR=value` prefixes, `env VAR=value`, and
  // GIT_CONFIG_* set earlier (git reads them as configuration, aliases included).
  const envNames = [];
  let xargs = null;
  const emit = (name, rest, extra = {}) => out.push({
    name, argv0: rest[0], args: rest.slice(1), redirects: meta.redirects, stdin: meta.stdin, piped: meta.piped,
    cwd: extra.cwd !== undefined ? extra.cwd : env.cwd, raw: meta.raw, line: meta.line, via: meta.via,
    envNames: [...envNames, ...[...env.vars.keys()].filter((n) => n.startsWith('GIT_CONFIG'))], xargs: Boolean(xargs),
    named: readName(rest[0], env.whole), whole: env.whole,
  });
  // Under `untracked`, a wrapper word (w[from..to)) or command name (w[name]) temper does not read
  // exactly is reported before the model acts on it: `timeout $T apply x` runs `git apply x` when $T is `5 git`.
  const unread = (from, to, name = -1) => {
    if (!env.untracked) return;
    const x = w.slice(from, to).find((y) => !readName(y, env.whole) && !fixedWord(y)) ?? (name < w.length && name !== -1 && !readName(w[name], env.whole) ? w[name] : undefined);
    if (x) out.push(depthMarker(meta.raw, env, meta.via, 'name', x));
  };
  w = w.slice(skipKeywords(w, 0));
  if (!w.length) { if (meta.redirects.length) emit('', [literalWord('')]); return; }
  if (w[0].raw === 'function' && !w[0].quoted) { // function f { ...
    w = w.slice(skipKeywords(w, 2));
    if (!w.length) return;
  }
  if (['for', 'select', 'case'].includes(w[0].raw) && !w[0].quoted) {
    if (w[0].raw !== 'case' && w[1]) setVar(env, w[1].raw, null);
    return;
  }
  // Assignments: alone they set shell variables; before a command they only set its environment.
  let k = 0;
  while (k < w.length && isAssign(w[k])) k++;
  if (k === w.length) {
    if (!meta.isolated) for (const a of w) assignTo(env, a);
    if (meta.redirects.length) emit('', [literalWord('')]);
    return;
  }
  // PowerShell: `$x = 'value'`.
  if (w.length === 3 && /^\$\w+$/.test(w[0].raw) && w[1].raw === '=') {
    setVar(env, w[0].raw.slice(1), w[2].value);
    return;
  }
  for (const a of w.slice(0, k)) envNames.push(a.assign);
  let cwd = env.cwd;
  // Wrappers are walked with an index, so a long chain (`busybox busybox ...`) stays linear.
  let i = k;
  for (;;) {
    if (i >= w.length || w[i].value === null) break;
    const name = commandName(w[i].value);
    const valued = WRAPPERS.get(name);
    if (!valued) break;
    i++;
    if (name === 'command' && w[i] && /^-[vV]+$/.test(w[i].raw)) return; // a lookup, not a run
    if (name === 'xargs') xargs = { replace: null };
    let watchExec = false;
    while (i < w.length && ((w[i].raw.startsWith('-') && w[i].raw !== '-') || (name === 'env' && isAssign(w[i])))) {
      const a = w[i].raw;
      if (name === 'env' && isAssign(w[i])) envNames.push(w[i].assign);
      i++;
      if (name === 'xargs') {
        const rep = /^(?:-I|--replace=?|-i)(.*)$/.exec(a);
        if (rep) {
          if (rep[1]) xargs.replace = rep[1];
          else if (a === '-I' || a === '--replace') { xargs.replace = w[i]?.raw ?? '{}'; i++; } else xargs.replace = '{}';
          continue;
        }
      }
      if ((name === 'env' || name === 'sudo') && (a === '-C' || a === '--chdir')) { cwd = resolvePath(w[i]?.value ?? null, cwd); i++; continue; }
      if (name === 'env' && /^(?:-S|--split-string)(?:=|$)|^-S./.test(a)) { // env -S splits its string into the command and runs it
        const str = /^-S./.test(a) ? a.slice(2) : a.includes('=') ? a.slice(a.indexOf('=') + 1) : w[i++]?.text ?? '';
        unread(k, i);
        sub([str, ...w.slice(i).map((x) => x.text)].join(' '), { ...env, cwd, ps: false }, out, depth + 1, 'env -S');
        return;
      }
      if (name === 'env' && /^--chdir=/.test(a)) { cwd = resolvePath(a.slice(8), cwd); continue; }
      if (name === 'watch') {
        if (a === '--exec' || /^-[a-zA-Z]*x/.test(a)) watchExec = true;
        if (/^-[a-zA-Z]*[nq]$/.test(a)) { i++; continue; } // -n 2, -dn 2
      }
      if (valued.includes(a)) i++;
    }
    if (name === 'watch' && !watchExec) {
      unread(k, i);
      if (i < w.length) sub(w.slice(i).map((x) => x.text).join(' '), { ...env, cwd, ps: false }, out, depth + 1, 'watch');
      return;
    }
    if (name === 'timeout' && i < w.length) i++;
    if (name === 'nice' && w[i] && /^-\d+$/.test(w[i].raw)) i++;
  }
  unread(k, i, i);
  w = w.slice(i);
  if (!w.length) { if (meta.redirects.length) emit('', [literalWord('')]); return; } // `exec > file`
  if (xargs) {
    if (xargs.replace) w = w.map((x) => (x.raw.includes(xargs.replace) ? unknownWord(x.raw) : x));
    else w = [...w, unknownWord('(xargs input)')];
  }
  if (w[0].value === null) { emit(null, w, { cwd }); return; }
  const name = commandName(w[0].value);
  const args = w.slice(1);

  if (CD.has(name)) {
    if (meta.isolated) return;
    const before = env.cwd;
    if (name === 'popd') env.cwd = env.dirs.length ? env.dirs.pop() : null;
    else {
      const a = args.filter((x) => x.value === null || !/^(?:--|-[LPe@]+|-path|-literalpath)$/i.test(x.raw));
      if (name === 'pushd') env.dirs.push(env.cwd);
      if (!a.length) env.cwd = name === 'pushd' ? null : env.home;
      else if (a.length > 1) env.cwd = null;
      else if (a[0].value === '-') env.cwd = env.prev;
      else env.cwd = wordPath(a[0], env.cwd);
    }
    env.prev = before;
    return;
  }
  if (DECLARE.has(name)) {
    if (!meta.isolated) for (const a of args) if (isAssign(a)) assignTo(env, a);
    return;
  }
  emit(name, w, { cwd });
  if (SHELLS.has(name)) {
    const src = shellScript(args);
    const script = src.from === 'arg' ? src.word?.text : src.from === 'stdin' ? meta.stdin?.body : null;
    if (script) sub(script, { ...env, cwd, ps: false }, out, depth + 1, `${name} -c`);
  } else if (name === 'eval') {
    run(args.map((a) => a.text).join(' '), env, out, depth + 1, 'eval');
  } else if (name === 'pwsh' || name === 'powershell') {
    const flag = args.findIndex((a) => /^-(?:c|command)$/i.test(a.raw));
    if (flag !== -1 && args[flag + 1]) sub(args.slice(flag + 1).map((a) => a.text).join(' '), { ...env, cwd, ps: true }, out, depth + 1, `${name} -Command`);
  } else if (name === 'find') {
    for (let k2 = 0; k2 < args.length; k2++) {
      if (!['-exec', '-execdir', '-ok', '-okdir'].includes(args[k2].raw)) continue;
      if (depth >= MAX_DEPTH) { out.push(depthMarker(meta.raw, env, 'find -exec')); break; }
      const j = args.findIndex((a, x) => x > k2 && (a.value === ';' || a.value === '+'));
      const inner = args.slice(k2 + 1, j === -1 ? args.length : j).map((a) => (a.raw.includes('{}') ? unknownWord(a.raw) : a));
      if (inner.length) simple(inner, { ...meta, redirects: [], stdin: null, via: 'find -exec' }, { ...env, cwd: args[k2].raw.endsWith('dir') ? null : cwd, vars: new Map(env.vars), dirs: [] }, out, depth + 1);
      k2 = j === -1 ? args.length : j;
    }
  }
}

// ---------- what a command writes ----------

const literalWord = (s) => ({ raw: s, text: s, value: s, literal: true, glob: false, quoted: false });

/**
 * The subcommand of a dashed git command (`git-apply`, `/usr/lib/git-core/git-am`,
 * `"$(git --exec-path)/git-apply"`: the basename as written), or null.
 */
const dashedGit = (c) => /^git-([\w-]+)$/.exec(c.name ?? commandName(c.argv0?.raw ?? ''))?.[1] ?? null;

// git's own commands (`git --list-cmds=main`, plus the newer `backfill`, `refs` and `replay`).
// An alias never overrides one; any other subcommand may be an alias.
export const GIT_BUILTINS = new Set(`add am annotate apply archive backfill bisect blame branch bugreport bundle cat-file check-attr
check-ignore check-mailmap check-ref-format checkout checkout-index cherry cherry-pick citool clean clone column commit
commit-graph commit-tree config count-objects credential credential-cache credential-store daemon describe diagnose diff
diff-files diff-index diff-tree difftool fast-export fast-import fetch fetch-pack filter-branch fmt-merge-msg for-each-ref
for-each-repo format-patch fsck fsck-objects gc get-tar-commit-id grep gui hash-object help hook http-backend http-fetch
http-push imap-send index-pack init init-db instaweb interpret-trailers log ls-files ls-remote ls-tree mailinfo mailsplit
maintenance merge merge-base merge-file merge-index merge-octopus merge-one-file merge-ours merge-recursive
merge-recursive-ours merge-recursive-theirs merge-resolve merge-subtree merge-tree mergetool mktag mktree multi-pack-index mv
name-rev notes pack-objects pack-redundant pack-refs patch-id pickaxe prune prune-packed pull push quiltimport range-diff
read-tree rebase receive-pack reflog refs remote remote-ext remote-fd remote-ftp remote-ftps remote-http remote-https repack
replace replay request-pull rerere reset restore rev-list rev-parse revert rm send-pack shell shortlog show show-branch
show-index show-ref sparse-checkout stage stash status stripspace submodule switch symbolic-ref tag unpack-file
unpack-objects update-index update-ref update-server-info upload-archive upload-pack var verify-commit verify-pack
verify-tag version whatchanged worktree write-tree`.split(/\s+/));

// Configuration git reads from somewhere temper does not: the environment, or an included file.
const HIDDEN_CONFIG_ENV = /^GIT_CONFIG/;
const INCLUDE_KEY = /^include(?:if\.[\s\S]*)?\.path=/i;

/**
 * `git [-C dir] [-c k=v] [--config-env k=ENV] <sub> args`: `{ sub, args, cwd, dynamic, hiddenAlias }`,
 * or null when the command is not git. A subcommand that is not one of git's own and has an alias
 * set with `-c alias.x=...` resolves to the command it names, its words before `args`.
 * `dynamic` is true when temper cannot tell the subcommand: an unknown word, a shell alias
 * (`!...`), a `-c` value that could define an alias, or a subcommand git does not have while
 * configuration temper cannot read is in play (`--config-env alias.x=...`, a `GIT_CONFIG_*`
 * variable, `-c include.path=...`); `hiddenAlias` marks that last case.
 */
function gitCommand(c) {
  const dashed = dashedGit(c);
  if (dashed === 'apply' || dashed === 'am') return { sub: dashed, args: c.args, cwd: c.cwd, dynamic: false, hiddenAlias: false };
  if (c.name !== 'git') return null;
  let dir = c.cwd;
  let a = c.args;
  const aliases = new Map();
  let opaque = false;
  let hidden = (c.envNames ?? []).some((n) => HIDDEN_CONFIG_ENV.test(n));
  let expansions = 0;
  for (let i = 0; i < a.length; i++) {
    const t = a[i].raw;
    if (t === '-C') { dir = wordPath(a[++i], dir); continue; }
    if (t === '-c') {
      const w = a[++i];
      // `alias.x=$V` is an alias temper cannot read; `color.ui=$V` sets no alias.
      const m = /^alias\.([^=]+)=([\s\S]*)$/i.exec(w?.value ?? w?.raw ?? '');
      if (m) aliases.set(m[1].toLowerCase(), w.value === null ? '!' : m[2].trim());
      else if (INCLUDE_KEY.test(w?.value ?? w?.raw ?? '')) hidden = true;
      else if (w?.value == null && !/^[A-Za-z][\w-]*\.[\w.-]*=/.test(w?.raw ?? '')) opaque = true;
      continue;
    }
    if (t === '--config-env' || t.startsWith('--config-env=')) {
      // Its value comes from the environment, so an alias it sets is never readable.
      const v = t === '--config-env' ? a[++i]?.raw ?? '' : t.slice('--config-env='.length);
      if (/^alias\./i.test(v)) hidden = true;
      else if (!/^[A-Za-z][\w-]*\.[\w.-]*=/.test(v) || INCLUDE_KEY.test(v)) hidden = true;
      continue;
    }
    if (t === '--git-dir' || t === '--work-tree' || t === '--namespace') { i++; continue; }
    if (t.startsWith('-')) continue;
    if (a[i].value === null) return { sub: '', args: a.slice(i + 1), cwd: dir, dynamic: true, hiddenAlias: false };
    const sub = a[i].value;
    const args = a.slice(i + 1);
    if (!GIT_BUILTINS.has(sub) && aliases.has(sub.toLowerCase())) {
      // A chain longer than temper follows may end on git apply: it is read as a guessed one.
      if (expansions === 8) return { sub: '', args, cwd: dir, dynamic: true, hiddenAlias: true };
      const def = aliases.get(sub.toLowerCase());
      if (!def || def.startsWith('!')) return { sub: '', args, cwd: dir, dynamic: true, hiddenAlias: false };
      // git reads options at the start of an alias (`-p apply`, `-c k=v apply`) as its own: the
      // expansion goes back through this loop.
      expansions++;
      a = [...def.split(/\s+/).map(literalWord), ...args];
      i = -1;
      continue;
    }
    const hiddenAlias = hidden && !GIT_BUILTINS.has(sub);
    return { sub, args, cwd: dir, dynamic: opaque || hiddenAlias, hiddenAlias };
  }
  return { sub: '', args: [], cwd: dir, dynamic: opaque, hiddenAlias: false };
}

const INSTALL_VALUES = ['--suffix', '--mode', '--owner', '--group', '--strip-program', '--context'];
const RSYNC_VALUES = new Set(['-e', '--rsh', '-f', '--filter', '--exclude', '--include', '--exclude-from', '--include-from', '--files-from',
  '--chmod', '--chown', '--log-file', '--backup-dir', '--suffix', '--partial-dir', '-T', '--temp-dir', '--compare-dest', '--copy-dest',
  '--link-dest', '--max-size', '--min-size', '--bwlimit', '--timeout', '--port', '-M', '--remote-option', '-B', '--block-size']);
const remote = (w) => w.value !== null && /^[^/\\]+:/.test(w.value) && !/^[A-Za-z]:(?:[\\/]|$)/.test(w.value);

/**
 * cp, mv, install, ln, git mv, rsync and scp arguments: `{ sources, dest, intoDir, recursive }`,
 * or null when nothing is copied (`install -d`, a dry run, a remote destination). `intoDir`
 * is true for `-t` or several sources, false for `-T`, and undefined when it depends on
 * whether the destination is a directory.
 */
function copyArgs(name, args) {
  const rsync = name === 'rsync' || name === 'scp';
  const valueLetters = name === 'install' ? 'tSmogZ' : rsync ? '' : 'tS';
  let target = null;
  let noTarget = false;
  let recursive = name === 'mv' || name === 'git';
  const pos = [];
  let opts = true;
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (opts && a.raw === '--') { opts = false; continue; }
    if (opts && a.raw.startsWith('--')) {
      const eq = a.raw.indexOf('=');
      const k = eq === -1 ? a.raw : a.raw.slice(0, eq);
      const v = eq === -1 ? undefined : { ...a, raw: a.raw.slice(eq + 1), text: a.text.slice(a.text.indexOf('=') + 1), value: a.value === null ? null : a.value.slice(a.value.indexOf('=') + 1) };
      if (k === '--target-directory' && !rsync) { target = v ?? args[++i] ?? null; if (!target) return null; }
      else if (k === '--no-target-directory') noTarget = true;
      else if (k === '--recursive' || k === '--archive') recursive = true;
      else if (k === '--directory' || k === '--dry-run' || k === '--list-only') return null;
      else if (v === undefined && (INSTALL_VALUES.includes(k) || (rsync && RSYNC_VALUES.has(k)))) i++;
      continue;
    }
    if (opts && /^-[^-]/.test(a.raw)) {
      if (rsync) {
        if (RSYNC_VALUES.has(a.raw)) i++;
        if (/^-[a-zA-Z]*[ra]/.test(a.raw)) recursive = true;
        if (/^-[a-zA-Z]*n/.test(a.raw)) return null; // --dry-run
        continue;
      }
      const cluster = a.raw.slice(1);
      for (let j = 0; j < cluster.length; j++) {
        const ch = cluster[j];
        if (valueLetters.includes(ch)) {
          const rest = cluster.slice(j + 1);
          const v = rest ? { raw: rest, text: rest, value: rest, glob: false } : args[++i];
          if (ch === 't') { target = v ?? null; if (!target) return null; }
          break;
        }
        if (ch === 'T') noTarget = true;
        else if ('rRa'.includes(ch)) recursive = true;
        else if (ch === 'd' && name === 'install') return null;
        else if (ch === 'n' && name === 'git') return null; // git mv -n is a dry run
      }
      continue;
    }
    pos.push(a);
  }
  let sources;
  let dest;
  let intoDir;
  if (target) { sources = pos; dest = target; intoDir = true; } else {
    if (pos.length < 2) return null;
    sources = pos.slice(0, -1);
    dest = pos.at(-1);
    intoDir = noTarget ? false : pos.length > 2 ? true : undefined;
  }
  if (rsync) {
    if (remote(dest)) return null;
    sources = sources.map((s) => (remote(s) ? unknownWord(s.raw) : s));
  }
  return { sources, dest, intoDir, recursive };
}

/** PowerShell parameters: named (`-Name value`, `-Name:value`, switches) and positional. */
function psParams(args, switches = /^(?:force|append|noclobber|nonewline|recurse|passthru|whatif|confirm|container|asbytestream)$/) {
  const named = new Map();
  const pos = [];
  for (let j = 0; j < args.length; j++) {
    const m = /^-([A-Za-z]+)(?::(.*))?$/s.exec(args[j].raw);
    if (!m || args[j].quoted) { pos.push(args[j]); continue; }
    const key = m[1].toLowerCase();
    named.set(key, m[2] !== undefined ? { raw: m[2], text: m[2], value: m[2], glob: false } : switches.test(key) ? true : args[++j]);
  }
  // A parameter may be abbreviated to any unique prefix.
  const get = (...full) => [...named].find(([k]) => k.length > 1 && full.some((f) => f.startsWith(k)))?.[1];
  return { named, pos, get };
}

/**
 * Only a bare `-WhatIf` or `-WhatIf:$true` makes a PowerShell command a dry run. Any other value
 * (`$false`, `0`, `$f`, `(1 -eq 0)`, a variable) may be false, so the command counts as running.
 */
const psDryRun = (named) => {
  const w = named.get('whatif');
  return w === true || (w !== undefined && /^\$true$/i.test(w.raw));
};

/** Copy-Item / Move-Item (and copy, move, cpi, mi): `{ sources, dest, intoDir, recursive }`, or null. */
function psCopyArgs(move, args) {
  const { pos, get, named } = psParams(args);
  if (psDryRun(named)) return null;
  const paths = [get('path'), get('literalpath'), get('pspath'), named.get('lp')].filter((x) => x && x !== true);
  if (!paths.length && pos.length) paths.push(pos.shift());
  let dest = get('destination');
  if ((!dest || dest === true) && pos.length) dest = pos.shift();
  if (!paths.length || !dest || dest === true) return null;
  const filtered = ['filter', 'include', 'exclude'].some((k) => named.has(k));
  const sources = paths.flatMap((p) => (p.value === null ? [p] : p.value.split(',').map((t) => (filtered ? unknownWord(t) : { ...p, raw: t, text: t, value: t }))));
  return { sources, dest, intoDir: sources.length > 1 ? true : undefined, recursive: move || named.has('recurse') };
}

/**
 * The value of `-o`/`--output`-style options: `-o f`, `-sSLo f`, `-of`, `-sSLof`, `--output f`,
 * `--output=f`. A short-option cluster is read letter by letter and stops at a letter in
 * `valueLetters`, whose value is the rest of the word (`-dfoo`) or the next word.
 */
function optionValues(args, short, long, valueLetters = '') {
  const out = [];
  for (let k = 0; k < args.length; k++) {
    const a = args[k].raw;
    const eq = long.find((l) => a.startsWith(`${l}=`));
    const word = (t) => ({ ...args[k], raw: t, text: t, value: args[k].value === null ? null : t });
    if (eq) out.push(word(a.slice(eq.length + 1)));
    else if (long.includes(a)) { if (args[k + 1]) out.push(args[++k]); }
    else if (short && /^-[a-zA-Z]/.test(a)) {
      for (let j = 1; j < a.length; j++) {
        const rest = a.slice(j + 1);
        if (a[j] === short) {
          if (rest) out.push(word(rest)); else if (args[k + 1]) out.push(args[++k]);
          break;
        }
        if (valueLetters.includes(a[j])) { if (!rest) k++; break; }
        if (!/[a-zA-Z]/.test(a[j])) break;
      }
    }
  }
  return out;
}
// Short options that take a value, so a cluster stops at them (`curl -dfoo`, `wget -Uo`).
const CURL_VALUES = 'AbcCdDeEFHKmoPQrtTuUwxXyYz';
const WGET_VALUES = 'aeiBtOoTwQPUlARDIX';

/** sed -i / perl -i: a short-option cluster that reaches `i` before an option taking a value (`-Mstrict` is not -i). */
const inPlace = (sed, args) => args.some((a) => (sed ? /^-[nErsuz]*[iI]/ : /^-[nplawsTtUWXc]*i/).test(a.raw) || /^--in-place(?:=|$)/.test(a.raw));

/** Files sed -i / perl -i rewrites: the operands after the script, unless -e/-f gave it. */
function inPlaceTargets(args, sed) {
  const files = [];
  let script = false;
  for (let k = 0; k < args.length; k++) {
    const a = args[k].raw;
    if (sed && (a === '-i' || a === '-I') && args[k + 1]?.raw === '' && args[k + 1].quoted) { k++; continue; } // BSD `-i ''`
    if (a === '') continue;
    if (/^-[a-zA-Z]*[ef]$/.test(a) || a === '--expression' || a === '--file') { script = true; k++; continue; }
    if (/^--(?:expression|file)=/.test(a) || /^-e./.test(a)) { script = true; continue; }
    if (a.startsWith('-')) continue;
    files.push(args[k]);
  }
  return script ? files : files.slice(1);
}

/** `git checkout <tree-ish> [--] paths` / `git restore [--source=<tree-ish>] paths`: `{ source, paths }` (source null: the index). */
function gitRestore(sub, args) {
  let source = null;
  const paths = [];
  if (sub === 'checkout') {
    const dd = args.findIndex((a) => a.raw === '--');
    const before = [];
    const head = dd === -1 ? args : args.slice(0, dd);
    for (let k = 0; k < head.length; k++) {
      if (['-b', '-B', '--orphan', '--conflict'].includes(head[k].raw)) { k++; continue; }
      if (!head[k].raw.startsWith('-')) before.push(head[k]);
    }
    source = before[0]?.raw ?? null;
    paths.push(...(dd === -1 ? before.slice(1) : args.slice(dd + 1)));
    return { source, paths };
  }
  if (args.some((a) => a.raw === '--staged' || a.raw === '-S') && !args.some((a) => a.raw === '--worktree' || a.raw === '-W')) return { source: null, paths: [] };
  for (let k = 0; k < args.length; k++) {
    const a = args[k].raw;
    if (a === '-s' || a === '--source') source = args[++k]?.raw ?? null;
    else if (a.startsWith('--source=')) source = a.slice(9);
    else if (/^-s./.test(a)) source = a.slice(2);
    else if (a !== '--' && !a.startsWith('-')) paths.push(args[k]);
  }
  return { source, paths };
}

/** A git pathspec that is a pattern, not a path: a wildcard (`*`, `?`, `[`) or magic (`:(glob)...`, `:/x`, `:!x`). */
export const pathspecPattern = (w) => /[*?[]/.test(w.raw) || /^:(?:\(|[/!^])/.test(w.raw);

// Interpreter code that writes a file: the literal (or a variable assigned one) in a write position.
const LIT_PREFIX = '[rbfuRBFU]{0,2}';
const PATH_WRAP = String.raw`(?:(?:pathlib\s*\.\s*)?Path\s*\(\s*)?`;
const ARG = String.raw`${PATH_WRAP}(?:${LIT_PREFIX}__L(?<a>\d+)__|(?<![\w.])\$?(?<v>[A-Za-z_]\w*))\s*\)?`;
// A mode: a literal, or a variable the code assigned one (`m = 'w'; open(p, m)`).
const MODE = String.raw`(?:${LIT_PREFIX}__L(?<m>\d+)__|(?<mv>[A-Za-z_]\w*)\b(?!\s*[(.\[]))`;
const CODE_WRITES = [
  [String.raw`\bopen\s*\(\s*${ARG}\s*,\s*(?:mode\s*=\s*)?${MODE}`, (m) => /[wax+]/.test(m)],
  [String.raw`\bopen\s*\(?\s*(?:my\s+)?\$?\w+\s*,\s*__L(?<m>\d+)__\s*,\s*${ARG}`, (m) => /^\s*(?:\+?>>?|\+<)/.test(m)],
  [String.raw`\bos\s*\.\s*open\s*\(\s*${ARG}\s*,[^)]*O_(?:WRONLY|RDWR|CREAT|APPEND|TRUNC)`],
  [String.raw`${ARG}\s*\.\s*(?:write_text|write_bytes|touch)\b`],
  [String.raw`${ARG}\s*\.\s*open\s*\(\s*(?:mode\s*=\s*)?${MODE}`, (m) => /[wax+]/.test(m)],
  [String.raw`\b(?:(?:write|append|output)File(?:Sync)?|writeJson(?:Sync)?|createWriteStream|truncate(?:Sync)?|(?:File|IO)\s*\.\s*(?:bin)?write|(?:Write|Append)All(?:Text|Bytes|Lines))\s*\(\s*${ARG}`],
  [String.raw`\b(?:copy(?:file|2|tree|File(?:Sync)?)?|copy_file|move|mv|rename(?:Sync)?|replace|cp(?:Sync)?|(?:sym)?link(?:Sync)?)\s*\([^,()]*(?:\([^()]*\)[^,()]*)?,\s*${ARG}`],
].map(([re, mode]) => [new RegExp(re, 'g'), mode]);

/**
 * `code` with each string literal ('...', "...", `...`, backslash escapes kept) replaced by
 * `__L<n>__`, its text pushed to `lits`. One pass: a quote that never closes stays as it is,
 * and so does every later quote of its kind.
 */
const CODE_PLAIN = /[^"'`]*/y;
function stringLiterals(code, lits) {
  let out = '';
  const open = new Set(['"', "'", '`']);
  for (let i = 0; i < code.length;) {
    const q = code[i];
    if (!open.has(q)) {
      CODE_PLAIN.lastIndex = i + 1;
      const end = CODE_PLAIN.test(code) ? CODE_PLAIN.lastIndex : i + 1;
      out += code.slice(i, end);
      i = end;
      continue;
    }
    let j = i + 1;
    while (j < code.length && code[j] !== q) j += code[j] === '\\' ? 2 : 1;
    if (j >= code.length) { open.delete(q); out += q; i++; continue; }
    out += `__L${lits.push(code.slice(i + 1, j)) - 1}__`;
    i = j + 1;
  }
  return out;
}

/**
 * Files that python, node, ruby, perl or PowerShell code writes: `open(p, 'w')`, `Path(p).write_text`,
 * `writeFileSync(p)`, `File.write(p)`, `shutil.copy(src, p)`, `fs.renameSync(src, p)`, perl's
 * `open(my $fh, '>', p)`, `[IO.File]::WriteAllText(p)`... where `p` is a string literal or a
 * variable the code assigned one. Returns the literals; reads and writes elsewhere are not listed.
 */
export function codeWriteTargets(code) {
  const lits = [];
  const text = stringLiterals(String(code), lits);
  const vars = new Map();
  for (const m of text.matchAll(new RegExp(String.raw`(?:^|[^\w$.])\$?([A-Za-z_]\w*)\s*=(?!=)\s*${PATH_WRAP}${LIT_PREFIX}__L(\d+)__`, 'g'))) vars.set(m[1], lits[Number(m[2])]);
  const out = [];
  for (const [re, mode] of CODE_WRITES) {
    for (const m of text.matchAll(re)) {
      if (mode && !mode((m.groups.mv !== undefined ? vars.get(m.groups.mv) : lits[Number(m.groups.m)]) ?? '')) continue;
      const lit = m.groups.a !== undefined ? lits[Number(m.groups.a)] : vars.get(m.groups.v);
      if (lit) out.push(lit);
    }
  }
  // Perl's two-argument open: open(FH, '>file').
  for (const m of text.matchAll(/\bopen\s*\(?\s*(?:my\s+)?\$?\w+\s*,\s*__L(\d+)__\s*\)/g)) {
    const t = /^\s*\+?>>?\s*(\S.*)$/.exec(lits[Number(m[1])]);
    if (t) out.push(t[1]);
  }
  return [...new Set(out)];
}

// A line is read once, however many PowerShell expressions it holds.
let lastLine = null;
let lastTargets = [];
function lineCodeTargets(line) {
  if (line !== lastLine) { lastTargets = codeWriteTargets(line); lastLine = line; }
  return lastTargets;
}

const INTERPRETERS = /^(?:python[\d.]*|py|node|nodejs|deno|bun|ruby|perl|php|pwsh|powershell)$/;
const PS_CONTENT = /^(?:set-content|add-content|out-file|new-item|tee-object|sc|ac|ni)$/;
const PS_COPY = /^(?:copy-item|move-item|cpi|mi|copy|move)$/;

// Interpreter options before the script that take the next word as their value, by interpreter
// (perl's and ruby's -W, and perl's -C, are flags; -r, -E and php's -c are read as code options).
const CODE_VALUED = [
  [/^(?:python[\d.]*|py)$/, /^(?:-[WX]|--check-hash-based-pycs)$/], [/^(?:node|nodejs|bun)$/, /^(?:-C|--(?:require|import|loader|experimental-loader|conditions|input-type|title|env-file))$/],
  [/^ruby$/, /^-[IC]$/], [/^perl$/, /^-I$/], [/^php$/, /^-[dfz]$/],
];
/**
 * The words an interpreter runs as code from its options (-c, -e, -E, -p, -r, --eval, -Command), up
 * to its script or module (`python3 tool.py -c x`, `python3 -m pytest -p x`: the rest is theirs),
 * and `deno eval`'s operand. A word in an option's place built from an expansion (`python3 $O "$X"`)
 * is counted as code too, since temper cannot tell whether it is `-c`.
 */
function codeArgs(c) {
  const a = c.args;
  if (c.name === 'deno') {
    const k = a[0]?.value === 'eval' ? a.findIndex((x, j) => j > 0 && !x.raw.startsWith('-')) : -1;
    return k === -1 ? [] : [a[k]];
  }
  const code = [];
  // python's -c is its only code option, and ends its options; -E and -p are flags there.
  const py = /^(?:python[\d.]*|py)$/.test(c.name);
  const valued = CODE_VALUED.find(([n]) => n.test(c.name))?.[1];
  for (let j = 0; j < a.length; j++) {
    const x = a[j];
    if ((py ? /^-[a-zA-Z]*c$/.test(x.raw) : /^-[a-zA-Z]*[ceEp]$/.test(x.raw) || /^(?:-r|--eval|--print|-command)$/i.test(x.raw)) && a[j + 1]) {
      code.push(a[++j]);
      if (py) break;
      continue;
    }
    if (!pathWord(x, c.whole ?? '')) { code.push(x); break; }
    if (x.raw === '--' || x.raw === '-' || !x.raw.startsWith('-') || (x.raw === '-m' && py)) break;
    if (valued?.test(x.raw)) j++;
  }
  return code;
}

/** The code an interpreter runs from its arguments (see codeArgs) and stdin. */
function interpreterCode(c) {
  if (!INTERPRETERS.test(c.name ?? '')) return [];
  return [...(c.stdin?.body ? [c.stdin.body] : []), ...codeArgs(c).map((w) => w.text)];
}

/**
 * The files one parsed command writes, in the order the shell would, as targets
 * `{ verb, raw, text, value, glob, path, cwd }` (`path` is absolute, or null when the name
 * or the directory is unknown). Copies (cp, mv, install, ln, rsync, scp, git mv, Copy-Item,
 * Move-Item) add `sources`, `intoDir` and `recursive`, and their target is the destination
 * as written (see landing). `git checkout`/`git restore` targets add `source` (the
 * revision, or null for the index). `dir: true` marks a directory written into (`tar -C`,
 * `unzip -d`, `wget -P`, `curl --output-dir`). Interpreter code targets have verb `code`.
 */
export function commandWrites(c) {
  const out = [];
  const add = (verb, w, extra = {}) => {
    if (!w || w === true) return;
    out.push({ verb, raw: w.raw, text: w.text, value: w.value, glob: w.glob, path: wordPath(w, extra.cwd !== undefined ? extra.cwd : c.cwd), cwd: c.cwd, ...extra });
  };
  for (const r of c.redirects) if (WRITE_REDIRECTS.has(r.op)) add('redirect', r.word);
  const name = c.name;
  if (!name) return out;
  const args = c.args;
  if (name === 'tee' || name === 'sponge') {
    for (const a of args) if (!a.raw.startsWith('-')) add(name, a);
  } else if (name === 'truncate') {
    for (let k = 0; k < args.length; k++) {
      if (['-s', '--size', '-r', '--reference'].includes(args[k].raw)) k++;
      else if (!args[k].raw.startsWith('-')) add(name, args[k]);
    }
  } else if ((name === 'sed' || name === 'perl') && inPlace(name === 'sed', args)) {
    for (const a of inPlaceTargets(args, name === 'sed')) add(`${name} -i`, a);
  } else if (name === 'curl') {
    for (const a of optionValues(args, 'o', ['--output'], CURL_VALUES)) if (a.raw !== '-') add(name, a);
    for (const a of optionValues(args, '', ['--output-dir'])) add(name, a, { dir: true });
  } else if (name === 'wget') {
    for (const a of optionValues(args, 'O', ['--output-document'], WGET_VALUES)) if (a.raw !== '-') add(name, a);
    for (const a of optionValues(args, 'P', ['--directory-prefix'], WGET_VALUES)) add(name, a, { dir: true });
  } else if (name === 'dd') {
    for (const a of args) if (a.raw.startsWith('of=')) add(name, { ...a, raw: a.raw.slice(3), text: a.text.slice(3), value: a.value === null ? null : a.value.slice(3) });
  } else if (name === 'tar') {
    const extract = args.some((a, j) => /^--(?:extract|get)$/.test(a.raw) || /^-[a-zA-Z]*x[a-zA-Z]*$/.test(a.raw) || (j === 0 && /^[a-zA-Z]*x[a-zA-Z]*$/.test(a.raw)));
    if (extract) for (const a of optionValues(args, 'C', ['--directory'])) add(name, a, { dir: true });
  } else if (name === 'unzip') {
    for (const a of optionValues(args, 'd', [])) add(name, a, { dir: true });
  } else if (['cp', 'mv', 'install', 'ln', 'rsync', 'scp'].includes(name)) {
    const op = copyArgs(name, args);
    if (op) add(name, op.dest, { sources: op.sources, intoDir: op.intoDir, recursive: op.recursive });
  } else if (PS_COPY.test(name)) {
    const op = psCopyArgs(/^(?:move-item|mi|move)$/.test(name), args);
    if (op) add(name, op.dest, { sources: op.sources, intoDir: op.intoDir, recursive: op.recursive });
  } else if (PS_CONTENT.test(name)) {
    const { pos, get, named } = psParams(args);
    if (!psDryRun(named)) {
      const p = get('path', 'literalpath', 'filepath');
      add(name, p && p !== true ? p : pos[0]);
    }
  } else if (name === 'git') {
    const g = gitCommand(c);
    if (g.sub === 'mv') {
      const op = copyArgs('git', g.args);
      if (op) add('git mv', op.dest, { sources: op.sources, intoDir: op.intoDir, recursive: true, cwd: g.cwd });
    } else if (g.sub === 'checkout' || g.sub === 'restore') {
      const r = gitRestore(g.sub, g.args);
      for (const p of r.paths) add(`git ${g.sub}`, p, { source: r.source, cwd: g.cwd });
    }
  }
  if (INTERPRETERS.test(name)) {
    for (const code of interpreterCode(c)) for (const lit of codeWriteTargets(code)) add('code', { raw: lit, text: lit, value: lit, glob: false });
  }
  const a0 = c.argv0?.raw ?? '';
  if (/^[[$]/.test(a0) && a0 !== '[' && a0 !== '[[') { // a PowerShell expression: [IO.File]::WriteAllText(...); `[ -f x ]` is a test
    for (const lit of lineCodeTargets(c.line)) add('code', { raw: lit, text: lit, value: lit, glob: false });
  }
  return out;
}

/** Every file `cmd` writes (see parseCommands for the options and commandWrites for the targets). */
export const writeTargets = (cmd, opts = {}) => parseCommands(cmd, opts).flatMap((c) => commandWrites(c).map((t) => ({ ...t, command: c })));

// ---------- what a command deletes ----------

const PS_REMOVE = /^(?:remove-item|ri|del|erase|rd)$/;

/**
 * The files a simple command deletes, as `{ verb, raw, text, value, glob, path, cwd, recursive }`:
 * the operands of `rm` and `unlink`, the paths of `Remove-Item` (and its `ri`, `del`, `erase`
 * and `rd` aliases; PowerShell's `rm` reads as rm) and the pathspecs of `git rm`, which
 * counts with `--cached` too, since the file leaves the repository. `recursive` marks a delete
 * that takes a whole directory (`rm -r`, `-Recurse`, `git rm -r`). Dry runs (`git rm -n`,
 * `-WhatIf`) delete nothing. Kept apart from commandWrites: the state-dir rule lets `rm` through.
 */
export function commandDeletes(c) {
  const out = [];
  const add = (verb, w, recursive, cwd = c.cwd) => {
    if (!w || w === true) return;
    out.push({ verb, raw: w.raw, text: w.text, value: w.value, glob: w.glob, path: wordPath(w, cwd), cwd, recursive });
  };
  const name = c.name;
  if (name === 'rm' || name === 'unlink') {
    let recursive = false;
    let opts = true;
    const pos = [];
    for (const a of c.args) {
      if (opts && a.raw === '--') { opts = false; continue; }
      if (opts && /^-./.test(a.raw)) { if (/^-[a-zA-Z]*[rR]|^--recursive$/.test(a.raw)) recursive = true; continue; }
      pos.push(a);
    }
    for (const a of pos) add(name, a, recursive);
  } else if (PS_REMOVE.test(name ?? '')) {
    // `-Recurse` may be shortened to any prefix (`-r`, `-rec`), and takes no value.
    const { pos, get, named } = psParams(c.args, /^(?:force|whatif|confirm|r(?:e(?:c(?:u(?:r(?:se?)?)?)?)?)?)$/);
    if (psDryRun(named)) return out;
    const paths = [get('path'), get('literalpath'), named.get('lp')].filter((x) => x && x !== true);
    if (!paths.length && pos.length) paths.push(pos[0]);
    const filtered = ['filter', 'include', 'exclude'].some((k) => named.has(k));
    const recursive = [...named.keys()].some((k) => k.startsWith('r') && 'recurse'.startsWith(k));
    const words = paths.flatMap((p) => (p.value === null ? [p] : p.value.split(',').map((t) => (filtered ? unknownWord(t) : { ...p, raw: t, text: t, value: t }))));
    for (const w of words) add(name, w, recursive);
  } else if (name === 'git') {
    const g = gitCommand(c);
    if (g.sub !== 'rm') return out;
    let recursive = false;
    let opts = true;
    const pos = [];
    for (const a of g.args) {
      if (opts && a.raw === '--') { opts = false; continue; }
      if (opts && /^-./.test(a.raw)) {
        if (/^-[a-zA-Z]*n|^--dry-run$|^--pathspec-from-file/.test(a.raw)) return out;
        if (/^-[a-zA-Z]*r/.test(a.raw)) recursive = true;
        continue;
      }
      pos.push(a);
    }
    for (const a of pos) add('git rm', a, recursive, g.cwd);
  }
  return out;
}

/**
 * Where one source of a copy target lands: the destination, or the source's name inside it
 * when the copy goes into a directory (`-t`, several sources, a trailing `/`, or `destIsDir`,
 * which the caller reads from the disk). rsync's `src/` copies the contents. Null when the
 * destination or the source's name is unknown.
 */
export function landing(t, src, destIsDir = false) {
  if (!t.path) return null;
  if (t.verb === 'rsync' && /[\\/]$/.test(src.raw)) return t.path;
  const into = t.intoDir ?? (destIsDir || /[\\/]$/.test(t.raw));
  if (!into) return t.path;
  if (src.value === null || src.glob) return null;
  return path.join(t.path, path.basename(src.value.replace(/[\\/]+$/, '')));
}

// ---------- patches ----------

const PATCH_VALUES = new Set(['-p', '-i', '-o', '-d', '-D', '-F', '-B', '-Y', '-z', '-V', '-r', '-g', '--strip', '--input', '--output', '--directory', '--ifdef', '--fuzz', '--prefix', '--basename-prefix', '--suffix', '--version-control', '--reject-file', '--get', '--quoting-style']);
const GIT_APPLY_VALUES = new Set(['-p', '-C', '--exclude', '--include', '--directory', '--whitespace', '--build-fake-ancestor']);

/**
 * git apply, git am and patch: `{ tool, apply, inputs, outputs, target, cwd, stored }`, or
 * null for any other command. `apply` is false for the forms that change nothing
 * (`--check`, `--stat` without `--apply`, `--dry-run`, `--help`, `--version`, `git am --abort`).
 * `inputs` are the patch files named on the command line (relative to `cwd`, which follows
 * `git -C` and `patch -d`); with none, the patch comes from `c.stdin`. `outputs` (`-o`) and
 * `target` (patch's file operand) are files it writes. `stored` marks `git am --continue`,
 * which applies patches temper cannot see.
 */
export function patchCommand(c) {
  let guessed = false;
  let tool;
  let args;
  let cwd = c.cwd;
  if (c.name === 'patch') { tool = 'patch'; args = c.args; } else {
    const g = gitCommand(c);
    if (!g || (g.sub !== 'apply' && g.sub !== 'am' && !g.hiddenAlias)) return null;
    // A subcommand git does not have, under configuration temper cannot read, may be an alias
    // of git apply: it is read as one, and only a patch file temper can read counts (`guessed`).
    if (g.hiddenAlias) guessed = true;
    tool = g.hiddenAlias ? 'git apply' : `git ${g.sub}`;
    args = g.args;
    cwd = g.cwd;
  }
  let readOnly = false;
  let reportOnly = false;
  let applyToo = false;
  let stored = false;
  const inputs = [];
  const outputs = [];
  const pos = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a.value === null || !a.raw.startsWith('-') || a.raw === '-') { pos.push(a); continue; }
    const eq = a.raw.indexOf('=');
    const k = eq === -1 ? a.raw : a.raw.slice(0, eq);
    if (['-h', '--help', '--version'].includes(k) || (tool === 'patch' && k === '-v')) return { tool, apply: false };
    if (tool !== 'patch') {
      if (k === '--check') readOnly = true;
      else if (['--stat', '--numstat', '--summary'].includes(k)) reportOnly = true;
      else if (k === '--apply') applyToo = true;
      else if (['--abort', '--quit', '--show-current-patch'].includes(k)) return { tool, apply: false };
      else if (['--continue', '--skip', '--resolved', '-r'].includes(k) && tool === 'git am') stored = true;
      else if (eq === -1 && GIT_APPLY_VALUES.has(k)) i++;
      continue;
    }
    if (k === '--dry-run') { readOnly = true; continue; }
    let v = eq === -1 ? undefined : { ...a, raw: a.raw.slice(eq + 1), text: a.text.slice(a.text.indexOf('=') + 1), value: a.value.slice(eq + 1) };
    const short = /^-[pioDdFBYzVrg]/.test(k) && !k.startsWith('--');
    if (short && k.length > 2) v = { raw: k.slice(2), text: k.slice(2), value: k.slice(2), glob: false };
    const key = short ? k.slice(0, 2) : k;
    if (!PATCH_VALUES.has(key)) continue;
    if (v === undefined) v = args[++i];
    if (!v) continue;
    if (key === '-i' || key === '--input') inputs.push(v);
    else if (key === '-o' || key === '--output') outputs.push(v);
    else if (key === '-d' || key === '--directory') cwd = wordPath(v, cwd);
  }
  if ((readOnly || reportOnly) && !applyToo) return { tool, apply: false };
  let target = null;
  if (tool === 'patch') {
    target = pos[0] ?? null;
    if (pos[1]) inputs.push(pos[1]);
  } else inputs.push(...pos.filter((p) => p.raw !== '-'));
  return { tool, apply: true, inputs, outputs, target, cwd, stored, guessed };
}

// Interpreter code that runs git apply or git am (`['git', 'apply', ...]` too, across lines), or a
// patch command: `patch` followed by an option, a file or `<`, in a string or after `;`, `|`, `&`.
// git's options before the subcommand, the ones that take a value first.
const GIT_OPTS = String.raw`(?:(?:-C|-c|--git-dir|--work-tree|--namespace|--config-env)\s+\S+\s+|--?[\w-]+(?:=\S*)?\s+){0,8}`;
// In code with quotes, commas and brackets made spaces: git, its options, then apply or am as
// the subcommand (`--grep apply` is an option's value), or the dashed git-apply and git-am.
const GIT_PATCH_CODE = new RegExp(String.raw`\bgit\s+(?:args\s*[:=]\s*)?${GIT_OPTS}(?:apply|am)(?![\w./-])|\bgit-(?:apply|am)\b`);
// Wrappers that run the patch after them: `busybox`, `exec`, `command`, `nohup`, `timeout [-k 5] 10`.
// Each starts with its own word and the timeout's options and durations alternate with spaces, so the match stays linear.
const PATCH_WRAP = String.raw`(?:(?:busybox|exec|command|nohup|timeout(?:\s+-\S+)*(?:\s+\d[\w.]*){1,2})\s+)`;
// A patch command in a string or after `;`, `|`, `&`, a directory or a wrapper: `patch` followed by an
// option, a file, `<`, `|` or `>` (`patch<x.diff`); in list form, by an option or a .diff or .patch argument.
const PATCH_CMD_CODE = new RegExp(String.raw`(?:^|['"\`(\[;|&/]\s*|\b${PATCH_WRAP})g?patch(?:[<|>]|\s+[-<\w./~$"']|['"],\s*['"]-|['"](?=,[^\]\n]{0,160}?\.(?:diff|patch)['"]))`, 'm');
// `patch` reading stdin at the end of a pipeline or a command list (`'cat x.diff|patch'`,
// `'cat x.diff|exec patch'`, `'cat x.diff|timeout 5 patch'`), at any path.
const PATCH_END_CODE = new RegExp(String.raw`[;|&]\s*${PATCH_WRAP}{0,3}(?:[\w.~-]*\/)*g?patch\s*(?:['"\`;&|)]|$)`, 'm');
// A bare patch handed to a call that runs a command (`os.system('patch')`, `run(['/usr/bin/patch'])`):
// it reads the interpreter's stdin, which may be a patch. `(?:\[\s*)?` keeps one way to match a run of spaces.
const PATCH_CALL_CODE = new RegExp(String.raw`\b(?:system|popen|run|call|check_call|check_output|Popen|exec\w*|spawn\w*)\s*\(\s*(?:\[\s*)?['"\`]\s*${PATCH_WRAP}{0,3}(?:[\w.~-]*\/)*g?patch\s*['"\`]`);

/** The string escapes interpreters read (`\x20`, `\040`, `\u0020`, `\u{20}`) decoded; `\t`, `\n`, `\r`, `\f`, `\v` as spaces. */
const decodeEscapes = (code) => code.replace(/\\(?:x([0-9a-fA-F]{2})|u\{([0-9a-fA-F]{1,6})\}|u([0-9a-fA-F]{4})|([0-7]{1,3})|[tnrfv])/g, (m, x, ub, u, o) => {
  if (x === undefined && ub === undefined && u === undefined && o === undefined) return ' ';
  const n = o !== undefined ? parseInt(o, 8) : parseInt(x ?? ub ?? u, 16);
  return n <= 0x10ffff ? String.fromCodePoint(n) : m;
});

/** Whether interpreter code mentions git apply, git am or a patch command, with its string escapes decoded. */
function codePatches(code) {
  const text = decodeEscapes(code);
  return GIT_PATCH_CODE.test(text.replace(/['"`,[\]{}()+]/g, ' ')) || PATCH_CMD_CODE.test(text) || PATCH_END_CODE.test(text) || PATCH_CALL_CODE.test(text);
}
const CONFIG_VERBS = new Set(['set', 'unset', 'rename-section', 'remove-section']);
const CONFIG_WRITE_FLAGS = /^--(?:add|unset|unset-all|replace-all|rename-section|remove-section)$/;

/** Whether `git config <args>` sets, unsets or renames an `alias.*` key (`git config alias.x` alone reads it). */
function configWritesAlias(args) {
  const pos = args.filter((x) => !x.raw.startsWith('-'));
  const verb = CONFIG_VERBS.has(pos[0]?.raw) ? pos.shift() : null;
  const key = pos.findIndex((x) => /^alias\./i.test(x.value ?? x.raw));
  if (key === -1) return false;
  return Boolean(verb) || pos.length > key + 1 || args.some((x) => CONFIG_WRITE_FLAGS.test(x.raw.split('=')[0]));
}

/**
 * Why temper cannot rule out that `c` applies a patch, as `{ kind, why }`, or null. `kind` is:
 * - `git`: a git subcommand it cannot resolve (`git $x`, `git ${CMD:-apply}`, a shell alias, a
 *   `-c` value it cannot read), or one git does not have, which may be an alias set anywhere;
 * - `name`: a command name it cannot read (see readName: `$X`, `"${CARGO:-cargo}"`, `{git,apply,x}`,
 *   `git*`, a wrapper word under `untracked`), or one it cannot resolve that names git or patch, or
 *   comes before `apply` or `am`, and an alias or `hash -p`, which make a name run another command;
 * - `code`: interpreter code (`python -c`, `node -e`, `perl -e`, `ruby -e`, `deno eval`, ...) that
 *   mentions `git apply`, `git am` or a patch command or is built from an expansion (`python3 -c "$X"`),
 *   or code piped or redirected into one that temper cannot read;
 * - `script`: a shell or `source` whose script temper cannot read exactly (see unreadScript:
 *   `cat cmd.sh | bash`, `echo '...' | sh`, `bash /dev/stdin`, `source <(...)`, `xargs bash -c`,
 *   `bash -c "$CMD"`, `bash <<< "$X"`, `bash -c {x,y}`), an `eval` argument built from an expansion
 *   (`eval "$(cat cmd.txt)"`, `eval git*`), or commands nested deeper than temper reads;
 * - `alias`: `git config` setting an alias, which could name git apply later.
 * Patterns, not a parse: for a fail-closed check.
 */
export function opaquePatch(c) {
  if (c.opaque === 'depth') return { kind: 'script', why: 'commands nested deeper than temper reads' };
  const dashed = dashedGit(c);
  if (dashed !== null && dashed !== 'apply' && dashed !== 'am') return { kind: 'git', why: `a dashed git command (git-${dashed})` };
  const g = gitCommand(c);
  if (!g && c.name === null && (/\b(?:git|g?patch)\b/.test(c.argv0?.raw ?? '') || c.args.some((x) => x.value === 'apply' || x.value === 'am'))) {
    return { kind: 'name', why: 'an unresolved command name' };
  }
  // A command name temper cannot read (see readName), or an eval argument built from an expansion, runs text temper cannot read exactly (wi-0e226b).
  if (c.opaque === 'name') return { kind: 'name', why: 'a command name or wrapper word built from an expansion' };
  if (c.named === false) return { kind: 'name', why: 'a command name built from an expansion' };
  // An alias or `hash -p` makes a name run another command.
  if ((c.name === 'alias' && c.args.some((x) => x.raw.includes('='))) || (c.name === 'hash' && c.args.some((x) => /^-\w*p/.test(x.raw)))) {
    return { kind: 'name', why: `${c.name === 'alias' ? 'an alias' : 'a hash -p'}, which makes a command name run another command` };
  }
  if (c.name === 'eval' && c.args.some((x) => x.literal !== true)) return { kind: 'script', why: 'an eval argument built from an expansion' };
  if (g) {
    if (g.dynamic || (g.sub && !GIT_BUILTINS.has(g.sub))) return { kind: 'git', why: 'a git subcommand temper cannot resolve' };
    if (g.sub === 'config' && configWritesAlias(g.args)) return { kind: 'alias', why: 'a git config write to an alias' };
    return null;
  }
  if (SHELLS.has(c.name) || SOURCES.has(c.name)) {
    const why = unreadScript(c);
    return why ? { kind: 'script', why: `a ${c.name} script ${why} that temper cannot read` } : null;
  }
  if (!INTERPRETERS.test(c.name)) return null;
  if (codeArgs(c).some((w) => !w.literal)) return { kind: 'code', why: `${c.name} code built from an expansion` };
  if (interpreterCode(c).some(codePatches)) return { kind: 'code', why: `${c.name} code that runs git apply, git am or patch` };
  // A script on stdin that temper cannot read: `cat x.py | python3`, `python3 - < x.py`, `python3 <<< "$X"`.
  const scriptArg = c.args.some((x) => !x.raw.startsWith('-')) || c.args.some((x) => /^-[a-zA-Z]*[ceEp]$|^(?:-r|--eval|--print|-command)$/i.test(x.raw));
  if (c.stdin && !readStdin(c.stdin) && !scriptArg) return { kind: 'code', why: `a ${c.name} script on stdin that temper cannot read` };
  return null;
}

const SOURCES = new Set(['source', '.']);
/**
 * Whether temper reads a word exactly (#151 review 7, wi-0e226b): a literal, or one variable
 * (`names`, or any) or a double-quoted `$(git rev-parse --show-toplevel)` followed by a literal
 * path (`"$HOME/.cargo/bin/cargo"`, `$PWD/target/debug/a`), on a line that does not write that
 * variable (see writesNames). An unquoted one must have a known value with no space or glob to split.
 */
function pathWord(w, whole, names = null, lead = false) {
  if (!w) return false;
  if (w.literal) return true;
  if (!w.parts || w.glob) return false;
  const [head, ...tail] = w.parts.filter((p) => !p.bs && !(p.t === 'lit' && p.s === ''));
  if (!head || !tail.every((p) => p.t === 'lit' && (p.q || !/[{}[\]*?]/.test(p.s)))) return false;
  if (!/^\/(?:.*\/)?[^/]+$/s.test(tail.map((p) => p.s).join(''))) return false;
  let name;
  if (head.t === 'var' && (head.s === `$${head.name}` || head.s === `\${${head.name}}`) && /^[A-Za-z_]\w*$/.test(head.name)) name = head.name;
  else if (head.t === 'sub' && head.q && /^\s*git\s+rev-parse\s+--show-toplevel\s*$/.test(head.src)) name = 'PWD';
  else return false;
  if (names && head.t === 'var' && !names.includes(name)) return false;
  if (!head.q && (w.value === null || /[\s*?[\]{}]/.test(w.value) || lineFacts(whole, '[', false))) return false;
  return !lineFacts(whole, name, lead);
}
/**
 * Whether temper reads a command name exactly: a literal, or $HOME, $PWD or the quoted git toplevel
 * followed by a literal path (see pathWord). Never a value from an assignment: temper's model of
 * variables is not bash's (`read`, `printf -v`, arrays, `unset`, an assignment that never runs).
 */
const readName = (w, whole) => pathWord(w, whole, ['HOME', 'PWD']) && w.value !== null;
// writesNames for the line being read, kept so a long line is scanned once per name, not once per
// command; `lead` reads it without a `source` or `.` that starts it (see unreadScript).
let facts = { whole: null, seen: new Map() };
function lineFacts(whole, name, lead) {
  if (facts.whole !== whole) facts = { whole, seen: new Map() };
  const key = `${lead ? '+' : ''}${name}`;
  if (!facts.seen.has(key)) facts.seen.set(key, writesNames(lead ? whole.trimStart().replace(/^(?:source|\.)(?=\s)/, '') : whole, name));
  return facts.seen.get(key);
}
/**
 * A wrapper's option or assignment word whose role and split temper reads: it starts with literal
 * text and its expansions are quoted (`A="$X"`), with no glob or brace list (`-I{}` is not one).
 */
const fixedWord = (w) => Boolean(w.parts) && !w.glob && w.parts[0]?.t === 'lit' && w.parts[0].s !== ''
  && w.parts.every((p) => (p.t === 'lit' ? p.q || !p.s.replace(/\{\}/g, '').includes('{') : p.q));
// Builtins that write a variable named by their arguments, and cd and its kin, which set PWD.
const NAME_WRITERS = /(?:^|[\s;&|(){}!`])(?:read|mapfile|readarray|declare|typeset|local|export|readonly|unset|eval|source|\.|getopts|select|let|coproc|trap|wait|printf|cd|pushd|popd|chdir)(?=$|[\s;&|()<>`])/;
/**
 * Whether a command line may write the variable `name` or IFS, read as text with its quotes and
 * backslashes removed, so `HO''ME` is `HOME`: an assignment, `${HOME:=...}`, a builtin that writes a
 * variable it is given (`read`, `printf -v`, `declare`, `unset`, `cd`, ...), `source`, arithmetic,
 * `${!X}` or an ANSI-C string. Patterns, not a parse: it says yes when unsure.
 */
function writesNames(src, name) {
  if (name === '[') return src.includes('[');
  if (/\$'|\(\(|\$\[|\$\{!/.test(src)) return true;
  const s = src.replace(/['"\\]/g, '');
  if (NAME_WRITERS.test(s)) return true;
  for (const m of s.matchAll(new RegExp(`(?<!\\w)(?:${name}|IFS)(?!\\w)`, 'g'))) {
    const read = s[m.index - 1] === '$' || (s.slice(m.index - 2, m.index) === '${' && s[m.index + m[0].length] === '}');
    if (!read) return true;
  }
  return false;
}
// Paths that name the shell's own stdin.
const STDIN_PATH = /^(?:\/dev\/stdin|\/dev\/fd\/\d+|\/proc\/[^/]+\/fd\/\d+)$/;
/** Whether temper read a command's stdin exactly: a literal heredoc or here-string body, or one a plain `cat` passed on. */
const readStdin = (s) => Boolean(s) && (s.kind === 'pipe' ? s.exact === true && s.body !== null && s.body !== undefined
  : (s.kind === 'heredoc' || s.kind === 'string') && s.literal !== null && s.literal !== undefined);

/**
 * Where a shell, or `source` and `.`, reads a script temper cannot read exactly (#151, wi-0e226b),
 * or null: a `-c` script with an expansion in it (`"$CMD"`, `"$(cat x)"`) or none at all, stdin
 * that is not a literal heredoc or here-string body (a pipe, a file, a subshell's or a process
 * substitution's stdin, `<<< "$X"`, `<<EOF` with `$(...)` in it), `/dev/stdin` and the like, a
 * process substitution, or under xargs a script from its input. A literal `bash -c '...'` and a
 * script file (`bash x.sh`) are not flagged.
 */
function unreadScript(c) {
  let src;
  if (SOURCES.has(c.name)) {
    const k = c.args[0]?.value === '--' ? 1 : 0;
    src = c.args[k] ? { from: 'file', word: c.args[k] } : { from: 'stdin' };
  } else src = shellScript(c.args);
  if (src.from === 'none') return null;
  if (src.from === 'arg') {
    if (src.word?.literal) return null;
    if (c.xargs && (!src.word || src.word.value === null)) return 'from xargs input';
    return src.word ? 'built from an expansion' : 'after -c';
  }
  if (src.from === 'file') {
    const w = src.word;
    if (/[<>]\(/.test(w.raw)) return 'from a process substitution';
    if (w.value === null && c.xargs) return 'from xargs input';
    // `source` itself runs after its operand is read: the one that starts the line writes nothing before it.
    if (!pathWord(w, c.whole ?? '', null, SOURCES.has(c.name))) return 'built from an expansion';
    return w.value !== null && STDIN_PATH.test(path.posix.normalize(w.value)) ? `from ${w.value}` : null;
  }
  return readStdin(c.stdin) ? null : 'on stdin';
}

/**
 * Files a unified, context or git diff names, with prefixes left on. A `---`/`+++`/`***`
 * header may end in a tab or space and a timestamp (`diff -u`), and lines may end in CR.
 */
export const patchFiles = (text) => [...String(text).matchAll(/^(?:(?:\+\+\+|---|\*\*\*)[ \t]+("[^"]*"|[^\t\r\n]*?)(?:\t[^\r\n]*)?|diff --git ("[^"]*"|\S+) ("[^"]*"|\S+)|(?:rename|copy) (?:from|to) (.+?)|Index: (.+?))\r?$/gm)]
  .flatMap((m) => m.slice(1).filter(Boolean))
  .map((f) => f.replace(/^"|"$/g, '').replace(/\s+\d{4}-\d\d-\d\d[ \t].*$/, '').trim());
