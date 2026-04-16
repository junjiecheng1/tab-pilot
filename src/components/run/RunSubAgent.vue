<template>
  <div class="sub-activity" :class="`s-${step.status}`">
    <button
      class="sub-row"
      :class="{ expandable: hasSteps }"
      @click="hasSteps && (expanded = !expanded)"
    >
      <div class="sub-icon" :class="`s-${step.status}`">
        <svg
          v-if="step.status === 'running'"
          class="icon-ring"
          viewBox="0 0 24 24"
          width="22"
          height="22"
          fill="none"
        >
          <circle cx="12" cy="12" r="10" stroke="#bcc3ce" stroke-width="1.5" />
          <path
            d="M12 2a10 10 0 0 1 10 10"
            stroke="#646a73"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
        <svg
          v-else-if="step.status === 'error'"
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

      <div class="sub-content">
        <span class="sub-name">{{ displayName }}</span>
        <span v-if="step.description" class="sub-desc" :title="step.description">{{ step.description }}</span>
      </div>
      <span v-if="durationText" class="sub-duration">{{ durationText }}</span>
      <svg
        v-if="hasSteps"
        class="sub-chevron"
        :class="{ open: expanded }"
        viewBox="0 0 24 24"
        width="12"
        height="12"
      >
        <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
      </svg>
    </button>

    <!-- 嵌套 steps -->
    <div v-if="expanded && hasSteps" class="sub-nested">
      <template v-for="inner in step.steps" :key="inner.id">
        <RunToolCall v-if="inner.type === 'tool'" :step="inner" />
        <RunNarration v-else-if="inner.type === 'narration'" :step="inner" />
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import RunToolCall from './RunToolCall.vue';
import RunNarration from './RunNarration.vue';
import type { SubAgentStep } from '../../services/copilot/types';

const props = defineProps<{ step: SubAgentStep; isLast?: boolean }>();

// 默认展开：running 中或父 turn 是最新 turn；完成后非最新自动收起
const expanded = ref(
  props.step.status === 'running' || Boolean(props.isLast),
);

watch(
  () => props.step.status,
  (s) => {
    if (s !== 'running' && !props.isLast) expanded.value = false;
  },
);

const hasSteps = computed(() => (props.step.steps?.length || 0) > 0);

const displayName = computed(
  () => props.step.displayName || props.step.name || 'Agent',
);

const durationText = computed(() => {
  const d = props.step.durationMs;
  if (!d || d <= 0) return '';
  return d < 1000 ? `${d}ms` : `${(d / 1000).toFixed(1)}s`;
});

// 工具图标映射（对齐 PC SubAgentPanel.TOOL_ICONS）
const TOOL_ICONS: Record<string, string> = {
  music: '<circle cx="12" cy="12" r="2"/><path d="M12 2v8m0 4v8"/><path d="M4 9h16M4 15h16"/>',
  audio: '<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>',
  video: '<rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18"/><line x1="7" y1="2" x2="7" y2="22"/><line x1="17" y1="2" x2="17" y2="22"/><line x1="2" y1="12" x2="22" y2="12"/>',
  data: '<line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/>',
  deep_search: '<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.35-4.35"/>',
  search: '<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.35-4.35"/>',
  copywriter: '<path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/>',
  _default: '<circle cx="12" cy="8" r="4"/><path d="M4 20c0-4 3.6-7 8-7s8 3 8 7"/>',
};

const toolSvgPath = computed(() => {
  const n = props.step.name || '';
  return (
    TOOL_ICONS[n] ||
    TOOL_ICONS[n.replace(/\./g, '_')] ||
    Object.entries(TOOL_ICONS).find(([k]) => n.includes(k))?.[1] ||
    TOOL_ICONS._default
  );
});
</script>

<style scoped>
.sub-activity {
  display: flex;
  flex-direction: column;
}

.sub-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 4px 6px;
  background: transparent;
  border: none;
  border-radius: 8px;
  line-height: 1.5;
  text-align: left;
  cursor: default;
  transition: background 0.15s ease;
}

.sub-row.expandable { cursor: pointer; }
.sub-row.expandable:hover { background: rgba(0, 0, 0, 0.02); }

.sub-icon {
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
  animation: sub-spin 0.25s linear infinite;
}
@keyframes sub-spin { to { transform: rotate(360deg); } }

.icon-inner,
.icon-failed {
  position: relative;
  z-index: 1;
}

.sub-content {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
}

.sub-name {
  font-size: 13px;
  font-weight: 700;
  color: #111827; /* --tabapp-text-primary */
  white-space: nowrap;
  flex-shrink: 0;
}

.sub-desc {
  color: #64748b; /* --tabapp-text-secondary */
  font-size: 12px;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
  min-width: 0;
}

.s-error .sub-desc, .s-cancelled .sub-desc {
  color: #94a3b8; /* --c-text-tertiary */
}

.sub-duration {
  font-size: 10px;
  color: #94a3b8;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  flex-shrink: 0;
}

.sub-chevron {
  color: #9ca3af;
  transition: transform 0.2s ease;
  flex-shrink: 0;
  margin-left: 2px;
}
.sub-chevron.open { transform: rotate(180deg); }

/* 嵌套区（左边框引导线） */
.sub-nested {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 2px;
  padding-left: 10px;
  position: relative;
}
.sub-nested::before {
  content: '';
  position: absolute;
  left: 20px; /* 对齐 26x26 icon 中心 */
  top: 0;
  bottom: 0;
  width: 1px;
  background: rgba(0, 0, 0, 0.06);
}
.sub-nested > * {
  padding-left: 20px;
}
</style>
