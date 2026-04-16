<template>
  <div class="result-content markdown-body" v-html="rendered"></div>
</template>

<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue';
import { createMarkdownEngine } from '../../utils/markdown';

const props = defineProps<{ markdown: string }>();

const md = createMarkdownEngine({ allowHtml: false, enableHighlight: true });

/**
 * 流式场景下 content_delta 可能 100+/s，每次 md.render + hljs 同步
 * 执行非常昂贵。用 rAF 合并到一帧一次，避免主线程卡顿和 DOM 抖动。
 */
const rendered = ref('');
let rafId: number | null = null;

function schedule() {
  if (rafId !== null) return;
  rafId = requestAnimationFrame(() => {
    rafId = null;
    try {
      rendered.value = md.render(props.markdown || '');
    } catch (e) {
      console.warn('[RunContent] markdown render error', e);
    }
  });
}

watch(
  () => props.markdown,
  () => schedule(),
  { immediate: true },
);

onUnmounted(() => {
  if (rafId !== null) cancelAnimationFrame(rafId);
});
</script>

<style scoped>
.result-content {
  font-size: var(--text-base);
  line-height: var(--leading-relaxed);
  color: var(--c-text-primary);
  word-wrap: break-word;
  overflow-wrap: anywhere;
  padding: var(--space-1) 0;
  min-width: 0;
}

/* ── Markdown（对齐 PC TextBlock 子集，全部用 tokens） ── */
.markdown-body :deep(p) { margin: 6px 0; }
.markdown-body :deep(p:first-child) { margin-top: 0; }
.markdown-body :deep(p:last-child) { margin-bottom: 0; }

.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3),
.markdown-body :deep(h4) {
  font-weight: var(--weight-semibold);
  margin: 14px 0 6px;
  line-height: var(--leading-tight);
  color: var(--c-text-primary);
}
.markdown-body :deep(h1) { font-size: var(--text-lg); }
.markdown-body :deep(h2) { font-size: 16px; }
.markdown-body :deep(h3) { font-size: var(--text-md); }
.markdown-body :deep(h4) { font-size: var(--text-base); }

.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  margin: 6px 0;
  padding-left: 22px;
}
.markdown-body :deep(li) { margin: 2px 0; }

.markdown-body :deep(strong) { font-weight: var(--weight-semibold); }
.markdown-body :deep(em) { font-style: italic; color: var(--c-text-secondary); }

.markdown-body :deep(a) {
  color: var(--c-text-primary);
  text-decoration: underline;
  text-decoration-color: var(--c-border-strong);
  text-underline-offset: 2px;
  transition: color var(--duration-fast) var(--easing);
}
.markdown-body :deep(a:hover) {
  color: var(--brand-blue);
  text-decoration-color: var(--brand-blue);
}

.markdown-body :deep(code) {
  background: var(--c-bg-tertiary);
  padding: 1px 5px;
  border-radius: var(--radius-sm);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--c-text-primary);
  word-break: break-word;
}

.markdown-body :deep(blockquote) {
  margin: 8px 0;
  padding: 6px 10px;
  border-left: 3px solid var(--c-border-strong);
  background: var(--c-bg-secondary);
  color: var(--c-text-secondary);
  border-radius: 0 var(--radius-md) var(--radius-md) 0;
}

.markdown-body :deep(hr) {
  border: none;
  border-top: 1px solid var(--c-border);
  margin: 12px 0;
}

.markdown-body :deep(table) {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  border-collapse: collapse;
  margin: 8px 0;
  font-size: 12.5px;
}
.markdown-body :deep(th),
.markdown-body :deep(td) {
  border: 1px solid var(--c-border);
  padding: 4px 8px;
  text-align: left;
}
.markdown-body :deep(th) { background: var(--c-bg-secondary); font-weight: var(--weight-semibold); }

/* ── 代码块 fence ── */
.markdown-body :deep(.code-block-wrapper) {
  margin: 8px 0;
  border-radius: var(--radius-md);
  overflow: hidden;
  border: 1px solid var(--c-border);
  background: #0d1117;
}
.markdown-body :deep(.code-block-header) {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 10px;
  background: rgba(13, 17, 23, 0.95);
  color: #94a3b8;
  font-size: var(--text-xs);
  font-family: var(--font-mono);
  letter-spacing: 0.04em;
}
.markdown-body :deep(.code-block-copy-btn) {
  background: transparent;
  border: 1px solid rgba(148, 163, 184, 0.3);
  color: #cbd5e1;
  padding: 1px 8px;
  border-radius: var(--radius-sm);
  font-size: 10px;
  cursor: pointer;
  transition: all var(--duration-fast) var(--easing);
}
.markdown-body :deep(.code-block-copy-btn:hover) {
  background: rgba(148, 163, 184, 0.2);
  color: #fff;
}
.markdown-body :deep(.code-block-body) {
  background: #0d1117;
  overflow-x: auto;
}
.markdown-body :deep(.code-block-body pre) {
  margin: 0;
  padding: 10px 12px;
  color: #e2e8f0;
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  line-height: var(--leading-normal);
  word-break: normal;
}
.markdown-body :deep(.code-block-body code) {
  background: transparent;
  padding: 0;
  color: inherit;
  font-size: inherit;
}

/* GitHub Alerts — 用 copilot-tokens.css 里已定义的 .gh-alert 样式，这里做局部覆盖 */
.markdown-body :deep(.gh-alert) {
  margin: 8px 0;
  padding: 8px 12px;
  border-radius: 0 var(--radius-md) var(--radius-md) 0;
}
</style>
