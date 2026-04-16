<template>
  <div class="tl-tool" :class="statusCls">
    <div class="tool-icon" :class="statusCls">
      <svg v-if="step.status === 'running'" class="icon-ring" viewBox="0 0 24 24" width="22" height="22" fill="none">
        <circle cx="12" cy="12" r="10" stroke="#bcc3ce" stroke-width="1.5" />
        <path d="M12 2a10 10 0 0 1 10 10" stroke="#646a73" stroke-width="1.5" stroke-linecap="round" />
      </svg>
      <svg
        v-if="step.status === 'error' || step.status === 'cancelled'"
        class="icon-failed"
        viewBox="0 0 24 24"
        width="14"
        height="14"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <circle cx="12" cy="12" r="8.5" />
        <path d="M9.5 9.5l5 5M14.5 9.5l-5 5" />
      </svg>
      <svg
        v-else
        class="icon-inner"
        viewBox="0 0 24 24"
        width="13"
        height="13"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        v-html="toolSvgPath"
      />
    </div>

    <div class="tool-content">
      <span class="tool-name">{{ displayName }}</span>
      <span v-if="summaryText" class="tool-summary" :title="summaryText">{{ summaryText }}</span>
    </div>
    <span v-if="durationText" class="tool-duration">{{ durationText }}</span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import type { ToolStep } from '../../services/copilot/types';

const props = defineProps<{ step: ToolStep }>();

// 提取工具图标（对齐 PC PC版）
const TOOL_ICONS: Record<string, string> = {
  search: '<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.35-4.35"/>',
  web_search: '<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.35-4.35"/>',
  knowledge_answer: '<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>',
  read_table: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18"/>',
  write_table: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18"/>',
  batch: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M3 15h18M9 3v18"/>',
  platform: '<path d="M12 20V10"/><path d="M18 20V4"/><path d="M6 20v-4"/>',
  code: '<path d="M16 18l6-6-6-6"/><path d="M8 6l-6 6 6 6"/>',
  python: '<path d="M16 18l6-6-6-6"/><path d="M8 6l-6 6 6 6"/>',
  browser: '<circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15 15 0 0 1 4 10 15 15 0 0 1-4 10 15 15 0 0 1-4-10A15 15 0 0 1 12 2"/>',
  fetch: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="M7 10l5 5 5-5"/><path d="M12 15V3"/>',
  tab_pdf: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/>',
  tab_xlsx: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/>',
  message: '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>',
  shell: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M7 8l4 4-4 4"/><path d="M13 16h4"/>',
  file: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><path d="M14 2v6h6"/>',
  _default: '<circle cx="12" cy="12" r="3"/><path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"/>',
};

const toolSvgPath = computed(() => {
  const name = props.step.name || '';
  return TOOL_ICONS[name]
    || TOOL_ICONS[name.replace(/\./g, '_')]
    || Object.entries(TOOL_ICONS).find(([key]) => name.includes(key))?.[1]
    || TOOL_ICONS._default;
});

const displayName = computed(() => props.step.displayName || props.step.name || '工具');

const statusCls = computed(() => `s-${props.step.status}`);

const summaryText = computed(() => {
  if (props.step.status === 'error') return props.step.errorMessage || props.step.summary;
  return props.step.summary;
});

const durationText = computed(() => {
  const ms = props.step.durationMs;
  if (typeof ms !== 'number' || ms <= 0) return '';
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
});
</script>

<style scoped>
.tl-tool {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 6px;
  border-radius: 8px;
  line-height: 1.5;
  transition: background 0.15s ease;
}

.tl-tool:hover {
  background: rgba(0, 0, 0, 0.02);
}

.tool-icon {
  width: 26px;
  height: 26px;
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  border-radius: 8px;
  background: rgba(15, 23, 42, 0.03); /* 高级微沉降感 */
  color: #64748b; /* --tabapp-text-secondary */
}

.icon-ring {
  position: absolute;
  inset: 0;
  margin: auto;
  width: 22px;
  height: 22px;
  animation: spin 0.25s linear infinite;
}

.icon-inner,
.icon-failed {
  position: relative;
  z-index: 1;
}

.tool-content {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
}

.tool-name {
  font-size: 13px;
  font-weight: 700;
  color: #111827; /* --tabapp-text-primary */
  white-space: nowrap;
  flex-shrink: 0;
}

.tool-summary {
  color: #64748b; /* --tabapp-text-secondary */
  font-size: 12px;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
  min-width: 0;
}

.s-error .tool-summary, .s-cancelled .tool-summary {
  color: #94a3b8; /* --c-text-tertiary */
}

.tool-duration {
  font-size: 10px;
  color: #94a3b8;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  flex-shrink: 0;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
</style>
