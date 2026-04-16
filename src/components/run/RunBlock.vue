<template>
  <!-- 用户交互：ask / confirm / clarification → 专属组件 -->
  <RunAsk
    v-if="step.blockType === 'human_ask'"
    :payload="(step.payload as Record<string, unknown>)"
  />

  <!-- 其他结构化块：折叠 JSON 预览 -->
  <div v-else class="run-block">
    <div class="block-header" @click="expanded = !expanded">
      <FileText :size="12" />
      <span class="block-type">{{ step.blockType }}</span>
      <span class="block-chevron" :class="{ open: expanded }">▸</span>
    </div>
    <pre v-if="expanded" class="block-preview">{{ previewText }}</pre>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { FileText } from 'lucide-vue-next';
import type { BlockItem } from '../../services/copilot/types';
import RunAsk from './RunAsk.vue';

const props = defineProps<{ step: BlockItem }>();
const expanded = ref(false);

const previewText = computed(() => {
  const p = props.step.payload;
  if (typeof p === 'string') return p;
  try {
    const s = JSON.stringify(p, null, 2);
    return s.length > 1200 ? s.slice(0, 1200) + '…' : s;
  } catch {
    return String(p);
  }
});
</script>

<style scoped>
.run-block {
  margin: 4px 0;
}

.block-header {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  border-radius: 6px;
  color: #6b7280;
  font-size: 11px;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  cursor: pointer;
  user-select: none;
  background: rgba(0, 0, 0, 0.02);
  border: 1px solid rgba(0, 0, 0, 0.04);
  transition: all 0.2s ease;
}

.block-header:hover { background: rgba(0, 0, 0, 0.04); color: #374151; }

.block-type { flex: 1; font-weight: 500; }

.block-chevron { font-size: 10px; transition: transform 0.2s ease; opacity: 0.6; }
.block-chevron.open { transform: rotate(90deg); }

.block-preview {
  margin: 4px 0 0 0;
  padding: 8px 12px;
  border-radius: 6px;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  font-size: 11px;
  color: #4b5563;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 220px;
  overflow: auto;
  background: rgba(0, 0, 0, 0.03);
  border: 1px solid rgba(0, 0, 0, 0.05);
}

.block-preview::-webkit-scrollbar { width: 4px; }
.block-preview::-webkit-scrollbar-thumb { background: rgba(0,0,0,0.1); border-radius: 4px; }
</style>
