<template>
  <div class="turn-body">
    <!-- ① 时间线（Cursor风格：无边框展开项，非常克制） -->
    <section
      v-if="turn.steps.length || (isActive && !turn.content && !turn.blocks.length && !turn.error)"
      class="task-timeline"
    >
      <button class="tl-node cursor-style" @click="expanded = !expanded">
        <div class="tl-icon-wrapper">
          <div v-if="turn.status === 'streaming'" class="tl-spinner" />
          <svg v-else-if="turn.status === 'done'" viewBox="0 0 24 24" width="12" height="12" class="tl-check">
            <path d="M1 12c0 6.075 4.925 11 11 11s11-4.925 11-11S18.075 1 12 1 1 5.925 1 12Zm14.794-2.674a.994.994 0 0 1 1.405.01.994.994 0 0 1 .01 1.404c-1.888 1.886-3.776 3.77-5.66 5.659a1.001 1.001 0 0 1-1.418 0c-.984-.986-1.97-1.971-2.956-2.956a.993.993 0 0 1 .01-1.403.992.992 0 0 1 1.403-.012l2.252 2.252 4.954-4.954Z" fill="currentColor"/>
          </svg>
          <div v-else class="tl-circle error" />
        </div>
        <span class="tl-label mono">{{ timelineLabel }}</span>
        <span v-if="durationText" class="tl-duration mono">{{ durationText }}</span>
        <svg class="tl-chevron" :class="{ open: expanded }" viewBox="0 0 24 24" width="10" height="10">
          <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
        </svg>
      </button>

      <div v-if="expanded && turn.steps.length" class="tl-body">
        <div class="tl-line-guide"></div>
        <div class="tl-steps-wrapper" ref="stepsRef">
          <div v-for="step in turn.steps" :key="step.id" class="tl-step">
            <RunToolCall v-if="step.type === 'tool'" :step="step" />
            <RunNarration v-else-if="step.type === 'narration'" :step="step" />
            <RunSubAgent
              v-else-if="step.type === 'subagent'"
              :step="step"
              :is-last="isActive"
            />
          </div>
          <div v-if="isActive && turn.status === 'streaming'" class="tl-streaming">
            <span class="dot" /><span class="dot" /><span class="dot" />
          </div>
        </div>
      </div>
    </section>

    <!-- ② 独立块区 -->
    <section v-if="turn.blocks.length" class="blocks-area">
      <RunBlock v-for="b in turn.blocks" :key="b.id" :step="b" />
    </section>

    <!-- ③ 最终内容（无边框，直接 markdown） -->
    <RunContent v-if="turn.content" :markdown="turn.content" />

    <!-- 错误 -->
    <div v-if="turn.error" class="turn-error">
      <AlertTriangle :size="12" class="err-icon" />
      <div class="err-content">
        <div class="err-msg">{{ turn.error.message }}</div>
        <div class="err-code mono">{{ turn.error.code }}</div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { AlertTriangle } from 'lucide-vue-next';
import RunToolCall from './RunToolCall.vue';
import RunNarration from './RunNarration.vue';
import RunContent from './RunContent.vue';
import RunBlock from './RunBlock.vue';
import RunSubAgent from './RunSubAgent.vue';
import type { ChatTurn } from '../../services/copilot/types';

const props = withDefaults(defineProps<{ turn: ChatTurn; isActive?: boolean }>(), {
  isActive: false,
});

/** 当前轮默认展开；完成后若不是最新，自动收起（对齐 PC AssistantMessage）*/
const expanded = ref(
  props.turn.status === 'streaming' || props.isActive,
);

const stepsRef = ref<HTMLDivElement | null>(null);

watch(
  () => props.turn.steps.length,
  () => {
    if (expanded.value && stepsRef.value) {
      nextTick(() => {
        if (stepsRef.value) {
          stepsRef.value.scrollTop = stepsRef.value.scrollHeight;
        }
      });
    }
  }
);

watch(
  () => props.turn.status,
  (s) => {
    if ((s === 'done' || s === 'error') && !props.isActive) {
      expanded.value = false;
    }
  },
);

watch(
  () => props.isActive,
  (active) => {
    if (!active && (props.turn.status === 'done' || props.turn.status === 'error')) {
      expanded.value = false;
    }
  },
);

const timelineLabel = computed(() => {
  if (props.turn.status === 'error') return '执行失败';
  if (props.turn.status === 'streaming') return '正在处理...';
  // 如果是 done，获取最后执行的任务类型，如果没有则显示 Completed
  const steps = props.turn.steps;
  if (steps.length > 0) {
    const tools = steps.filter(s => s.type === 'tool').length;
    if (tools > 0) return `使用了 ${tools} 项工具`;
  }
  return '任务已完成';
});

const durationText = computed(() => {
  const d = props.turn.durationMs;
  if (!d || d <= 0) return '';
  if (d < 1000) return `${d}ms`;
  return `${(d / 1000).toFixed(1)}s`;
});
</script>

<style scoped>
.turn-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.mono {
  font-family: 'SF Mono', 'Cascadia Code', monospace;
}

/* ── 极简时间线 (Cursor 风格) ── */
.task-timeline {
  display: flex;
  flex-direction: column;
  /* 去掉强边框，只依靠轻量文字和缩进 */
  margin-top: 2px;
}

.tl-node.cursor-style {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  width: auto;
  align-self: flex-start;
  padding: 4px 8px 4px 4px;
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 11px;
  color: #6b7280;
  transition: all 0.2s ease;
}

.tl-node.cursor-style:hover {
  background: rgba(0, 0, 0, 0.04);
  color: #374151;
}

.tl-icon-wrapper {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
}

.tl-spinner {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  border: 1.5px solid rgba(0,0,0,0.1);
  border-top-color: #3b82f6;
  animation: spin 0.8s linear infinite;
}

.tl-check {
  color: #10b981;
}

.tl-circle {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #9ca3af;
}
.tl-circle.error { background: #ef4444; }

.tl-label {
  font-weight: 500;
  letter-spacing: 0.2px;
}

.tl-duration {
  font-size: 10px;
  color: #9ca3af;
  margin-left: 2px;
  opacity: 0.8;
}

.tl-chevron {
  opacity: 0.5;
  transition: transform 0.2s ease;
}
.tl-chevron.open { transform: rotate(180deg); }

/* Section 体（内嵌，带有左侧弱引导线） */
.tl-body {
  position: relative;
  display: flex;
  margin-top: 4px;
  padding-left: 10px; /* 让内容刚好在图标右侧对齐 */
}

.tl-line-guide {
  position: absolute;
  left: 11px; /* 对齐顶部图标的中心 */
  top: 4px;
  bottom: 4px;
  width: 1px;
  background: rgba(0, 0, 0, 0.06);
}

.tl-steps-wrapper {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
  padding-left: 12px;
  max-height: 280px;
  overflow-y: auto;
  overflow-x: hidden;
}

/* 隐藏或极简滚动条 */
.tl-steps-wrapper::-webkit-scrollbar {
  width: 4px;
}
.tl-steps-wrapper::-webkit-scrollbar-thumb {
  background: rgba(0, 0, 0, 0.08);
  border-radius: 4px;
}

.tl-step {
  font-size: 11px;
}

.tl-streaming {
  display: flex;
  align-items: center;
  gap: 3px;
  height: 16px;
  padding-left: 4px;
}

.tl-streaming .dot {
  width: 3px;
  height: 3px;
  border-radius: 50%;
  background: #9ca3af;
  animation: bounce 1.2s ease-in-out infinite;
}
.tl-streaming .dot:nth-child(2) { animation-delay: 0.15s; }
.tl-streaming .dot:nth-child(3) { animation-delay: 0.3s; }

/* ── 块区 ── */
.blocks-area {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/* ── 错误（轻量级显示） ── */
.turn-error {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 12px;
  background: rgba(239, 68, 68, 0.04);
  border: 1px solid rgba(239, 68, 68, 0.1);
  border-radius: 8px;
  color: #111827;
  font-size: 12px;
}

.err-icon {
  color: #ef4444;
  flex-shrink: 0;
  margin-top: 1px;
}

.err-content {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.err-msg {
  font-weight: 500;
  line-height: 1.4;
}

.err-code {
  font-size: 10px;
  color: #ef4444;
  opacity: 0.8;
}

@keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
@keyframes bounce {
  0%, 80%, 100% { transform: scale(0.6); opacity: 0.5; }
  40% { transform: scale(1); opacity: 1; }
}
</style>
