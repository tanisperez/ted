// Syntax highlighting and markdown rendering for the ted prototype.
const KW = {
  rust: new Set('as async await break const continue crate dyn else enum extern fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait type unsafe use where while'.split(' ')),
  js: new Set('async await break case catch class const continue default delete do else export extends finally for from function if import in instanceof let new of return static super switch this throw try typeof var void while yield'.split(' ')),
};
const CONSTS = new Set(['true', 'false', 'null', 'undefined', 'None', 'Some', 'Ok', 'Err', 'NaN']);

export const COLORS = {
  comment: 'var(--fg-muted)', keyword: 'var(--s-keyword)', function: 'var(--info)',
  property: 'var(--accent-text)', attribute: 'var(--accent-text)', string: 'var(--s-string)',
  number: 'var(--warning)', type: 'var(--warning)', constant: 'var(--s-constant)', tag: 'var(--danger)',
};

export function langOf(path) {
  const ext = path.split('.').pop().toLowerCase();
  return ({ rs: 'rust', js: 'js', mjs: 'js', ts: 'js', json: 'json', toml: 'toml', html: 'html', htm: 'html',
    xml: 'xml', svg: 'xml', md: 'markdown', log: 'log' })[ext] || 'text';
}
export const LANG_NAMES = { rust: 'Rust', js: 'JavaScript', json: 'JSON', toml: 'TOML', html: 'HTML', xml: 'XML', markdown: 'Markdown', log: 'Log', text: 'Texto plano' };

const word = (lang) => ({
  re: /[A-Za-z_][\w]*/y,
  k: (m, line, end) => {
    const w = m[0], next = line[end];
    if (KW[lang] && KW[lang].has(w)) return 'keyword';
    if (CONSTS.has(w)) return 'constant';
    if (next === '(' || (lang === 'rust' && next === '!')) return 'function';
    if (/^[A-Z]/.test(w) && lang !== 'log') return 'type';
    return '';
  },
});
const STR = { re: /"(?:[^"\\]|\\.)*"?/y, k: 'string' };
const NUM = { re: /\b\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?(?:[iuf]\d+|ms|s|KB|MB)?\b/y, k: 'number' };

const RULES = {
  rust: [{ re: /\/\/.*/y, k: 'comment' }, STR, { re: /#!?\[[^\]]*\]/y, k: 'attribute' }, NUM, word('rust')],
  js: [{ re: /\/\/.*/y, k: 'comment' }, { re: /\/\*.*?(\*\/|$)/y, k: 'comment' }, STR, { re: /'(?:[^'\\]|\\.)*'?/y, k: 'string' },
    { re: /`[^`]*`?/y, k: 'string' }, { re: /=>/y, k: 'keyword' }, NUM, word('js')],
  json: [{ re: /"(?:[^"\\]|\\.)*"(?=\s*:)/y, k: 'property' }, STR, { re: /-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/y, k: 'number' },
    { re: /\b(?:true|false|null)\b/y, k: 'constant' }],
  toml: [{ re: /#.*/y, k: 'comment' }, { re: /\[\[?[\w.\-]+\]\]?/y, k: (m, line, end) => line.slice(0, end - m[0].length).trim() === '' ? 'keyword' : '' },
    { re: /[\w.\-]+(?=\s*=)/y, k: 'property' }, STR, { re: /\b(?:true|false)\b/y, k: 'constant' }, NUM],
  xml: [{ re: /<!--.*?(-->|$)/y, k: 'comment' }, { re: /<[!?][\w\-]+/y, k: 'keyword' }, { re: /<\/?[\w:\-]+/y, k: 'tag' },
    { re: /\??\/?>/y, k: 'tag' }, { re: /[\w:\-]+(?==)/y, k: 'attribute' }, STR, { re: /&\w+;/y, k: 'constant' }],
  log: [{ re: /\d{4}-\d{2}-\d{2}[T ][\d:.]+Z?/y, k: 'comment' }, { re: /\b(?:ERROR|FATAL)\b/y, k: 'tag' },
    { re: /\bWARN\b/y, k: 'number' }, { re: /\bINFO\b/y, k: 'function' }, { re: /\b(?:DEBUG|TRACE)\b/y, k: 'comment' },
    { re: /\[[^\]]+\]/y, k: 'property' }, STR, NUM, { re: /[A-Za-z_]\w*/y, k: '' }],
  md: [{ re: /`[^`]+`/y, k: 'string' }, { re: /\*\*[^*]+\*\*/y, k: 'constant' }, { re: /\[[^\]]+\]\([^)]*\)/y, k: 'property' },
    { re: /\*[^*\s][^*]*\*/y, k: 'keyword' }],
};
RULES.html = RULES.xml;

function scan(line, rules) {
  const out = [];
  let i = 0, plain = '';
  const flush = () => { if (plain) { out.push({ t: plain, k: '' }); plain = ''; } };
  while (i < line.length) {
    let hit = null;
    for (const r of rules) {
      r.re.lastIndex = i;
      const m = r.re.exec(line);
      if (m && m[0].length) { hit = [m, r]; break; }
    }
    if (!hit) { plain += line[i++]; continue; }
    const [m, r] = hit;
    const end = i + m[0].length;
    const k = typeof r.k === 'function' ? r.k(m, line, end) : r.k;
    if (k) { flush(); out.push({ t: m[0], k }); } else plain += m[0];
    i = end;
  }
  flush();
  return out;
}

function mdLine(line, st) {
  if (/^```/.test(line)) { st.fence = !st.fence; return [{ t: line, k: 'comment' }]; }
  if (st.fence) return [{ t: line, k: 'string' }];
  if (/^#{1,6}\s/.test(line)) return [{ t: line, k: 'keyword', w: 600 }];
  if (/^>/.test(line)) return [{ t: line, k: 'comment' }];
  if (/^\s*\|/.test(line)) return scan(line, [{ re: /\|/y, k: 'comment' }, ...RULES.md]);
  const m = line.match(/^(\s*)([-*]|\d+\.)(\s)/);
  if (m) return [{ t: m[1], k: '' }, { t: m[2], k: 'number' }, { t: m[3], k: '' }, ...scan(line.slice(m[0].length), RULES.md)];
  return scan(line, RULES.md);
}

// Lines of tokens: [{ t, c, w }].
export function highlight(lang, text) {
  const st = { fence: false };
  return text.split('\n').map((line) => {
    const toks = lang === 'markdown' ? mdLine(line, st) : RULES[lang] ? scan(line, RULES[lang]) : [{ t: line, k: '' }];
    return toks.map((x) => ({ t: x.t, c: COLORS[x.k] || 'inherit', w: x.w || 400 }));
  });
}

const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const MONO = "'Geist Mono', ui-monospace, monospace";
function inline(s) {
  return esc(s)
    .replace(/`([^`]+)`/g, `<code style="font-family:${MONO};font-size:12.5px;background:var(--raised);border:1px solid var(--border);border-radius:4px;padding:1px 5px">$1</code>`)
    .replace(/\*\*([^*]+)\*\*/g, '<strong style="font-weight:600">$1</strong>')
    .replace(/\*([^*\s][^*]*)\*/g, '<em>$1</em>')
    .replace(/\[([^\]]+)\]\(([^)]*)\)/g, '<a style="color:var(--accent-text);text-decoration:none">$1</a>');
}
function codeBlock(lang, lines) {
  const l = langOf('x.' + (lang || 'txt'));
  const body = highlight(l, lines.join('\n')).map((toks) => toks.map((x) => `<span style="color:${x.c}">${esc(x.t)}</span>`).join('') || ' ').join('\n');
  return `<pre style="margin:0 0 16px;padding:12px 14px;background:var(--raised);border:1px solid var(--border);border-radius:8px;font-family:${MONO};font-size:12.5px;line-height:1.6;overflow:auto;color:var(--fg)">${body}</pre>`;
}

export function renderMarkdown(text) {
  const lines = text.split('\n');
  const out = [];
  let i = 0;
  const P = 'margin:0 0 14px;font-size:14px;line-height:1.65;text-wrap:pretty';
  while (i < lines.length) {
    const line = lines[i];
    let m;
    if (/^```/.test(line)) {
      const lang = line.slice(3).trim(); const buf = []; i++;
      while (i < lines.length && !/^```/.test(lines[i])) buf.push(lines[i++]);
      i++; out.push(codeBlock(lang, buf)); continue;
    }
    if ((m = line.match(/^(#{1,3})\s+(.*)/))) {
      const lvl = m[1].length;
      const st = lvl === 1 ? 'font-size:26px;margin:0 0 14px;padding-bottom:10px;border-bottom:1px solid var(--border)'
        : lvl === 2 ? 'font-size:19px;margin:28px 0 10px' : 'font-size:15px;margin:22px 0 8px';
      out.push(`<h${lvl} style="${st};font-weight:600;line-height:1.3">${inline(m[2])}</h${lvl}>`); i++; continue;
    }
    if (/^---+\s*$/.test(line)) { out.push('<hr style="border:0;border-top:1px solid var(--border);margin:24px 0">'); i++; continue; }
    if (/^>/.test(line)) {
      const buf = []; while (i < lines.length && /^>/.test(lines[i])) buf.push(lines[i++].replace(/^>\s?/, ''));
      out.push(`<blockquote style="margin:0 0 14px;padding:2px 0 2px 14px;border-left:3px solid var(--border-strong);color:var(--fg-muted);font-size:14px;line-height:1.65">${inline(buf.join(' '))}</blockquote>`); continue;
    }
    if (/^\s*\|/.test(line)) {
      const rows = []; while (i < lines.length && /^\s*\|/.test(lines[i])) rows.push(lines[i++]);
      const cells = (r) => r.trim().replace(/^\||\|$/g, '').split('|').map((c) => c.trim());
      const head = cells(rows[0]); const body = rows.slice(2).map(cells);
      const th = head.map((c) => `<th style="text-align:left;padding:7px 12px;font-weight:500;color:var(--fg-muted);border-bottom:1px solid var(--border);font-size:12.5px">${inline(c)}</th>`).join('');
      const tb = body.map((r) => `<tr>${r.map((c) => `<td style="padding:7px 12px;border-bottom:1px solid var(--border);font-size:13px">${inline(c)}</td>`).join('')}</tr>`).join('');
      out.push(`<table style="border-collapse:collapse;margin:0 0 16px;width:100%;border:1px solid var(--border);border-radius:8px;overflow:hidden"><thead style="background:var(--surface)"><tr>${th}</tr></thead><tbody>${tb}</tbody></table>`); continue;
    }
    if (/^\s*([-*]|\d+\.)\s/.test(line)) {
      const ordered = /^\s*\d+\./.test(line); const items = [];
      while (i < lines.length && /^\s*([-*]|\d+\.)\s/.test(lines[i])) items.push(lines[i++].replace(/^\s*([-*]|\d+\.)\s/, ''));
      const tag = ordered ? 'ol' : 'ul';
      out.push(`<${tag} style="margin:0 0 14px;padding-left:22px;font-size:14px;line-height:1.65">${items.map((x) => `<li style="margin:2px 0">${inline(x)}</li>`).join('')}</${tag}>`); continue;
    }
    if (!line.trim()) { i++; continue; }
    const buf = []; while (i < lines.length && lines[i].trim() && !/^(#|```|>|\s*\||\s*[-*]\s|\s*\d+\.\s)/.test(lines[i])) buf.push(lines[i++]);
    out.push(`<p style="${P}">${inline(buf.join(' '))}</p>`);
  }
  return out.join('');
}

export function wordCount(text) {
  return (text.replace(/```[\s\S]*?```/g, '').match(/[\p{L}\p{N}]+/gu) || []).length;
}
