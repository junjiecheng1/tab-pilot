import MarkdownIt from 'markdown-it';
import hljs from 'highlight.js';

export interface MarkdownEngineOptions {
  allowHtml?: boolean;
  enableHighlight?: boolean;
}

export function normalizeHighlightLanguage(language?: string): string | null {
  if (!language) return null;
  const normalized = language.toLowerCase();
  const aliases: Record<string, string> = {
    text: 'plaintext',
    plain: 'plaintext',
    plaintext: 'plaintext',
    md: 'markdown',
    shell: 'bash',
    sh: 'bash',
    zsh: 'bash',
    yml: 'yaml',
    ts: 'typescript',
    js: 'javascript',
    py: 'python',
  };
  const candidate = aliases[normalized] || normalized;
  return hljs.getLanguage(candidate) ? candidate : null;
}

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

function githubAlertsPlugin(md: any) {
  md.core.ruler.after('block', 'github_alerts', (state: any) => {
    const tokens = state.tokens;
    for (let i = 0; i < tokens.length; i++) {
        if (tokens[i].type === 'blockquote_open') {
            const pOpen = tokens[i + 1];
            if (pOpen && pOpen.type === 'paragraph_open') {
                const inline = tokens[i + 2];
                if (inline && inline.type === 'inline' && inline.content) {
                    const match = inline.content.match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]/i);
                    if (match) {
                        const type = match[1].toLowerCase();
                        
                        tokens[i].tag = 'div';
                        tokens[i].attrJoin('class', `gh-alert gh-alert-${type}`);
                        
                        let level = 1;
                        for (let j = i + 1; j < tokens.length; j++) {
                            if (tokens[j].type === 'blockquote_open') level++;
                            else if (tokens[j].type === 'blockquote_close') level--;
                            if (level === 0) {
                                tokens[j].tag = 'div';
                                break;
                            }
                        }
                        
                        inline.content = inline.content.replace(/^\[!.*?\]\s*/i, '');
                        if (inline.children && inline.children.length > 0) {
                           if (inline.children[0].type === 'text') {
                              inline.children[0].content = inline.children[0].content.replace(/^\[!.*?\]\s*/i, '');
                           }
                           if (inline.children[1] && (inline.children[1].type === 'softbreak' || inline.children[1].type === 'hardbreak')) {
                              inline.children.splice(1, 1);
                           }
                        }
                        
                        const titleOpen = new state.Token('html_inline', '', 0);
                        const titleText = match[1].charAt(0).toUpperCase() + match[1].slice(1).toLowerCase();
                        const emojiMap: Record<string, string> = {
                            note: 'ℹ️', tip: '💡', important: '💬', warning: '⚠️', caution: '🛑'
                        };
                        const icon = emojiMap[type] || 'ℹ️';
                        titleOpen.content = `<div class="gh-alert-title"><span style="margin-right:6px">${icon}</span>${titleText}</div>`;
                        if (inline.children) {
                           inline.children.unshift(titleOpen);
                        }
                    }
                }
            }
        }
    }
  });
}

export function createMarkdownEngine(options: MarkdownEngineOptions = {}) {
  const md = new MarkdownIt({
    html: !!options.allowHtml,
    linkify: true,
    breaks: true,
    typographer: true,
    highlight: undefined // 禁用默认 highlight，由 fence rule 接管
  });

  md.renderer.rules.fence = function(tokens, idx, _mdOptions, _env, _slf) {
    const token = tokens[idx];
    const code = token.content;
    const language = token.info ? token.info.trim().split(/\s+/)[0] : '';

    if (language === 'mermaid') {
      return `<div class="mermaid">${escapeHtml(code)}</div>`;
    }

    const normalized = normalizeHighlightLanguage(language);
    let highlightedCode = '';

    if (options.enableHighlight !== false) {
      if (normalized) {
        try {
          highlightedCode = hljs.highlight(code, { language: normalized, ignoreIllegals: true }).value;
        } catch (__) {
          highlightedCode = escapeHtml(code);
        }
      } else {
        highlightedCode = escapeHtml(code);
      }
    } else {
      highlightedCode = escapeHtml(code);
    }
    
    const langLabel = (normalized || language || 'text').toUpperCase();
    const encodedCode = encodeURIComponent(code);

    return `<div class="code-block-wrapper">
      <div class="code-block-header">
         <div class="code-block-lang">${escapeHtml(langLabel)}</div>
         <button class="code-block-copy-btn" onclick="let btn = this; navigator.clipboard.writeText(decodeURIComponent('${encodedCode}')).then(() => { btn.innerText='已复制'; setTimeout(()=>btn.innerText='复制',2000) })">复制</button>
      </div>
      <div class="code-block-body">
        <pre class="hljs"><code>${highlightedCode}</code></pre>
      </div>
    </div>`;
  };

  const defaultRender = md.renderer.rules.link_open || function(tokens, idx, options, _env, self) {
    return self.renderToken(tokens, idx, options);
  };
  md.renderer.rules.link_open = function(tokens, idx, options, env, self) {
    tokens[idx].attrSet('target', '_blank');
    tokens[idx].attrSet('rel', 'noopener noreferrer');
    return defaultRender(tokens, idx, options, env, self);
  };

  md.use(githubAlertsPlugin);

  return md;
}
