<template>
  <div class="security-view">
    <!-- 运行信任模式 -->
    <div class="card">
      <div class="mode-options">
        <label
          v-for="mode in modes"
          :key="mode.value"
          class="mode-option"
          :class="{ active: currentMode === mode.value }"
        >
          <div class="mode-header-wrap">
            <input
              type="radio"
              :value="mode.value"
              v-model="currentMode"
              class="mode-radio"
            />
            <div class="mode-header">
              <component :is="mode.icon" :size="16" :class="mode.colorClass" />
              <span class="mode-name">{{ mode.label }}</span>
            </div>
          </div>
          <div class="mode-content">
            <p class="mode-desc">{{ mode.description }}</p>
          </div>
        </label>
      </div>
    </div>

    <!-- 免疫指令白名单 & 锁定路径 并排 -->
    <div class="security-bottom">
      <div class="card commands-card">
        <div class="card-title justify-between">
          <div class="flex-row gap-2">
            <Fingerprint :size="16" class="text-tertiary" />
            已授权指令白名单
          </div>
          <button v-if="rememberedCommands.length > 0" class="btn btn-ghost btn-danger btn-sm" @click="clearAll">
            <Trash2 :size="12" /> 排空
          </button>
        </div>

        <div v-if="rememberedCommands.length === 0" class="empty-state">
          <Bird :size="32" stroke-width="1.5" class="text-tertiary" />
          暂无用户手动放行的危险指令
        </div>
        
        <div v-else class="command-list-wrap">
          <div class="command-list">
            <div
              v-for="(cmd, i) in rememberedCommands"
              :key="i"
              class="command-item"
            >
              <div class="command-text mono">{{ cmd }}</div>
              <button class="btn-icon" @click="removeCommand(i)" title="撤销授权">
                <XCircle :size="14" />
              </button>
            </div>
          </div>
        </div>
      </div>

      <div class="card paths-card">
        <div class="card-title">
          <LockKeyhole :size="16" class="text-tertiary" />
          硬编码锁定路径
        </div>
        <div class="path-grid">
          <div v-for="path in protectedPaths" :key="path" class="path-item mono">
            <Lock :size="10" class="text-tertiary" />
            {{ path }}
          </div>
        </div>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
import { 
  clearGuard, getRemembered, removeRemembered, 
  setGuardMode, getProtectedPaths
} from '../services/bridge';
import { usePilotStore } from '../services/pilotStore';
import { 
  ShieldAlert, Fingerprint, LockKeyhole, Lock, 
  Trash2, XCircle, Bird, ShieldHalf, Shield, Unlock 
} from 'lucide-vue-next';

import { GUARD_MODES } from '../constants/guardModes';
import type { GuardModeInfo } from '../constants/guardModes';

const store = usePilotStore();
const modes = GUARD_MODES;

const currentMode = ref(store.guardMode);
const rememberedCommands = ref<string[]>([]);
const protectedPaths = ref<string[]>([]);

// 初始化: store 已有 guard_mode, 只需加载其他数据
onMounted(async () => {
  await store.fetchStatus(); // 带缓存
  currentMode.value = store.guardMode;
  try {
    const [remembered, paths] = await Promise.all([
      getRemembered(),
      getProtectedPaths(),
    ]);
    rememberedCommands.value = remembered;
    protectedPaths.value = paths;
  } catch (e) {
    console.error('[Security] 初始化失败:', e);
  }
});

// 模式切换同步后端
watch(currentMode, async (mode) => {
  await store.setSetting(() => setGuardMode(mode));
});

async function clearAll() {
  await clearGuard();
  rememberedCommands.value = [];
}

async function removeCommand(index: number) {
  const cmd = rememberedCommands.value[index];
  if (cmd) {
    await removeRemembered(cmd);
    rememberedCommands.value.splice(index, 1);
  }
}
</script>
<style scoped>
.security-view {
  flex: 1;
  display: flex;
  flex-direction: column;
  animation: fadeIn 0.3s ease;
  min-height: 0;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}

.mode-options {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
}

.mode-option {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 10px;
  padding: 12px;
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  cursor: pointer;
  transition: all 0.2s ease;
  background: var(--bg-input);
}

.mode-header-wrap {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}

.mode-option:hover {
  border-color: rgba(51, 112, 255, 0.3);
  background: var(--bg-hover);
}

.mode-option.active {
  border-color: var(--accent);
  background: rgba(51, 112, 255, 0.04);
  box-shadow: 0 2px 8px rgba(51, 112, 255, 0.08);
}

.mode-radio {
  flex-shrink: 0;
  accent-color: var(--accent);
  width: 20px;
  height: 20px;
  min-width: 20px;
  min-height: 20px;
  margin: 0;
  cursor: pointer;
}

.mode-content {
  flex: 1;
  min-width: 0;
}

.mode-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 600;
  font-size: 13px;
  color: var(--text-primary);
  margin-bottom: 2px;
  min-width: 0;
  white-space: nowrap;
}

.color-blue { color: var(--blue); }
.color-green { color: var(--green); }
.color-yellow { color: var(--yellow); }

.mode-desc {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.4;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 32px 0;
  color: var(--text-tertiary);
  gap: 12px;
  font-size: 13px;
}

.security-bottom {
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: 1;
  min-height: 0;
}

.commands-card {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
  margin-bottom: 0;
}

.paths-card {
  margin-bottom: 0;
  flex-shrink: 0;
}

.command-list-wrap {
  flex: 1;
  overflow-y: auto;
  padding-right: 4px;
}

.command-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.command-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 16px;
  background: var(--bg-input);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius);
}

.command-text {
  font-size: 13px;
  color: var(--green);
  text-shadow: 0 0 1px rgba(35, 195, 67, 0.2);
}

.btn-icon {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 4px;
  border-radius: 4px;
  transition: all 0.15s;
}

.btn-icon:hover {
  background: var(--red-dim);
  color: var(--red);
}

.path-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.path-item {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-secondary);
  background: var(--bg-input);
  padding: 4px 8px;
  border-radius: 4px;
  border: 1px dashed var(--border);
}
</style>
