<template>
  <div class="tl-narration">
    <div class="nar-icon">
      <svg viewBox="0 0 24 24" width="12" height="12">
        <path d="M12 22C17.5228 22 22 17.5228 22 12C22 6.47715 17.5228 2 12 2C6.47715 2 2 6.47715 2 12C2 17.5228 6.47715 22 12 22Z" fill="none" stroke="currentColor" stroke-width="2"/>
        <circle cx="12" cy="12" r="3" fill="currentColor" opacity="0.6"/>
      </svg>
    </div>
    <span class="nar-text">{{ step.text }}</span>
  </div>
</template>

<script setup lang="ts">
import type { NarrationStep } from '../../services/copilot/types';
defineProps<{ step: NarrationStep }>();
</script>

<style scoped>
.tl-narration {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 4px 6px;
  font-size: 11px;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  color: #9ca3af;
  line-height: 1.5;
}

.nar-icon {
  width: 14px;
  height: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  margin-top: 1px;
  color: #d1d5db;
}

.nar-text {
  flex: 1;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
