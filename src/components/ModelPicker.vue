<template>
  <div class="model-selector" ref="rootRef" data-tauri-no-drag>
    <button
      type="button"
      class="selector-trigger"
      :class="{ open: isOpen }"
      :disabled="disabled"
      :title="triggerTitle"
      @click="toggle"
    >
      <img
        v-if="currentIcon"
        class="provider-icon"
        :src="currentIcon"
        :alt="triggerLabel"
        @error="onIconError"
      />
      <span v-else class="provider-fallback">{{ triggerLabel.slice(0, 1) }}</span>
      <svg class="chevron" width="9" height="9" viewBox="0 0 12 12" fill="none">
        <path d="M3 4.5L6 7.5L9 4.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </button>

    <Transition name="dropdown">
      <div v-if="isOpen" class="dropdown-menu">
        <div v-if="pref.loading && !pref.providers.length" class="dd-empty">加载中…</div>
        <div v-else-if="!pref.providers.length" class="dd-empty">
          <span>暂无可用模型</span>
          <button class="dd-retry" @click.stop="pref.fetchProviders(true)">重试</button>
        </div>
        <template v-else>
          <button
            v-for="p in orderedProviders"
            :key="p.id"
            type="button"
            class="dropdown-item"
            :class="{ active: p.id === pref.activeProviderId, disabled: !p.available }"
            :disabled="!p.available"
            @click="select(p)"
          >
            <img
              v-if="iconFor(p.icon)"
              class="provider-icon"
              :src="iconFor(p.icon)"
              :alt="p.name"
              @error="onIconError"
            />
            <span v-else class="provider-fallback">{{ p.name.slice(0, 1) }}</span>
            <span class="provider-name">{{ p.name }}</span>
            <svg v-if="p.id === pref.activeProviderId" class="check" width="13" height="13" viewBox="0 0 14 14" fill="none">
              <path d="M11.5 4L5.5 10L2.5 7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>
        </template>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { usePreferenceStore } from '../stores/preferenceStore';
import type { ProviderInfo } from '../services/copilot/catalog';

const props = withDefaults(defineProps<{ disabled?: boolean }>(), { disabled: false });

const pref = usePreferenceStore();
const isOpen = ref(false);
const rootRef = ref<HTMLElement | null>(null);

// ── 图标：用 public/icons/providers/*.svg（与 PC 资产一致的 LobeHub 图标）──
const ICON_BASE = '/icons/providers';
const KNOWN_ICONS = new Set(['openai', 'claude', 'gemini', 'deepseek', 'doubao', 'glm']);
const brokenIcons = ref(new Set<string>());

function iconFor(key: string | undefined | null): string {
  if (!key) return '';
  if (brokenIcons.value.has(key)) return '';
  if (KNOWN_ICONS.has(key)) return `${ICON_BASE}/${key}.svg`;
  return '';
}

function onIconError(e: Event) {
  const img = e.target as HTMLImageElement;
  const src = img.getAttribute('src') || '';
  const match = src.match(/\/([^/]+)\.svg$/);
  if (match) brokenIcons.value.add(match[1]);
}

// ── 派生 ──
const current = computed(() => pref.activeProvider);
const triggerLabel = computed(() => current.value?.name || '选择模型');
const triggerTitle = computed(() => current.value?.name || '选择模型');
const currentIcon = computed(() => iconFor(current.value?.icon));

/** 可用的排前，不可用排后 */
const orderedProviders = computed(() => {
  const available = pref.providers.filter((p) => p.available);
  const disabled = pref.providers.filter((p) => !p.available);
  return [...available, ...disabled];
});

// ── 动作 ──
function toggle() {
  if (props.disabled) return;
  if (!isOpen.value) pref.fetchProviders();
  isOpen.value = !isOpen.value;
}

function close() { isOpen.value = false; }

function select(p: ProviderInfo) {
  if (!p.available) return;
  pref.setProvider(p.id);
  close();
}

function onOutside(e: MouseEvent) {
  if (rootRef.value && !rootRef.value.contains(e.target as Node)) close();
}

onMounted(() => {
  pref.fetchProviders();
  document.addEventListener('click', onOutside);
});
onUnmounted(() => document.removeEventListener('click', onOutside));
</script>

<style scoped>
.model-selector {
  position: relative;
  display: inline-flex;
}

/* ── Trigger（对齐 PC: 透明 + hover 浅灰） ── */
.selector-trigger {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 4px 6px;
  background: transparent;
  border: none;
  border-radius: var(--radius-md);
  cursor: pointer;
  color: var(--c-text-secondary);
  transition: all var(--duration-fast) var(--easing);
  flex-shrink: 0;
}

.selector-trigger:hover:not(:disabled) {
  background: var(--c-bg-hover);
  color: var(--c-text-primary);
}

.selector-trigger.open {
  background: var(--c-bg-active);
  color: var(--c-text-primary);
}

.selector-trigger:disabled { opacity: 0.5; cursor: not-allowed; }

.provider-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  border-radius: var(--radius-sm);
}

.provider-fallback {
  width: 16px;
  height: 16px;
  border-radius: var(--radius-sm);
  background: var(--c-bg-tertiary);
  color: var(--c-text-secondary);
  font-size: 10px;
  font-weight: var(--weight-semibold);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  text-transform: uppercase;
}

.provider-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chevron {
  transition: transform var(--duration-fast) var(--easing);
  flex-shrink: 0;
  color: var(--c-text-tertiary);
}
.selector-trigger.open .chevron { transform: rotate(180deg); }

/* ── Dropdown（玻璃质感） ── */
.dropdown-menu {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  min-width: 220px;
  max-width: 300px;
  background: rgba(255, 255, 255, 0.85);
  -webkit-backdrop-filter: blur(24px) saturate(1.8);
  backdrop-filter: blur(24px) saturate(1.8);
  border: 1px solid rgba(255, 255, 255, 1);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
  padding: 6px;
  z-index: 100;
  max-height: 340px;
  overflow-y: auto;
}

.dropdown-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 10px;
  background: transparent;
  border: none;
  border-radius: 10px;
  cursor: pointer;
  font-size: var(--text-sm);
  color: var(--c-text-primary);
  transition: all var(--duration-normal) var(--easing);
  text-align: left;
}

.dropdown-item:hover:not(:disabled) { background: var(--c-bg-hover); }
.dropdown-item.active { background: var(--c-bg-active); }
.dropdown-item:disabled { opacity: 0.4; cursor: not-allowed; }

.dropdown-item .provider-name {
  flex: 1;
  font-weight: var(--weight-medium);
}

.check { color: var(--c-accent); flex-shrink: 0; }

/* 空态 */
.dd-empty {
  padding: 10px 12px;
  color: var(--c-text-tertiary);
  font-size: var(--text-xs);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.dd-retry {
  padding: 2px 8px;
  border: 1px solid var(--c-border);
  background: var(--c-bg-elevated);
  border-radius: var(--radius-sm);
  font-size: 11px;
  cursor: pointer;
  color: var(--c-text-primary);
}

/* Transition */
.dropdown-enter-active, .dropdown-leave-active {
  transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}
.dropdown-enter-from, .dropdown-leave-to {
  opacity: 0;
  transform: translateY(-6px) scale(0.97);
  pointer-events: none;
}
</style>
