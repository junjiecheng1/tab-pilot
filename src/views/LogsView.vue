<template>
  <div class="logs-view">
    <!-- 筛选栏 (玻璃质感悬浮) -->
    <div class="filter-glass-bar">
      <div class="filter-left">
        <div class="filter-badge">
          <Filter :size="12" class="text-tertiary" />
          <span>活动日志</span>
        </div>
        
        <div class="select-wrapper">
          <select v-model="filterTool" class="filter-select">
            <option value="">所有工具</option>
            <option value="shell">终端操作</option>
            <option value="file">文件管理</option>
            <option value="browser">浏览器</option>
          </select>
          <ChevronDown :size="12" class="select-icon" />
        </div>

        <div class="select-wrapper">
          <select v-model="filterStatus" class="filter-select">
            <option value="">所有状态</option>
            <option value="allowed">已放行</option>
            <option value="denied">被拦截</option>
            <option value="confirmed">待确认</option>
          </select>
          <ChevronDown :size="12" class="select-icon" />
        </div>
      </div>
      
      <div class="filter-right">
        <div class="auto-refresh-indicator" :class="{ active: autoRefresh }">
          <div class="pulse-dot" v-if="autoRefresh" />
          <span>{{ autoRefresh ? 'LIVE' : 'PAUSED' }}</span>
        </div>
        <button class="icon-btn" @click="toggleAutoRefresh" :title="autoRefresh ? '暂停自动刷新' : '开启自动刷新'">
          <Pause v-if="autoRefresh" :size="14" />
          <Play v-else :size="14" />
        </button>
        <button class="icon-btn" @click="loadLogs" title="立即刷新">
          <RefreshCw :size="14" />
        </button>
      </div>
    </div>

    <!-- 日志列表 -->
    <div class="logs-container">
      <div v-if="store.logs.length === 0" class="empty-state">
        <div class="empty-icon-wrap">
          <DatabaseBackup :size="24" stroke-width="1.5" />
        </div>
        <p>暂无活动日志</p>
      </div>
      
      <div v-else class="logs-list">
        <div 
          v-for="(log, i) in filteredLogs" 
          :key="i"
          class="log-item"
          :class="{ 'is-expanded': expandedRow === i, 'clickable': canExpand(log) }"
        >
          <!-- 摘要行 -->
          <div class="log-summary" @click="toggleExpand(i, log)">
            <div class="log-left">
              <div class="time-col mono">{{ formatTime(log.timestamp) }}</div>
              
              <div class="tool-col">
                <span class="tool-pill" :class="toolPillClass(log.tool_type)">
                  <TerminalSquare v-if="log.tool_type === 'shell'" :size="12" />
                  <FileCode2 v-else-if="log.tool_type === 'file'" :size="12" />
                  <Globe v-else :size="12" />
                  {{ log.tool_type }}
                </span>
              </div>
              
              <div class="action-col">
                <span class="action-name">{{ log.action }}</span>
                <span v-if="formatArgs(log.args_json)" class="action-desc mono">
                  {{ formatArgs(log.args_json) }}
                </span>
              </div>
            </div>

            <div class="log-right">
              <span class="duration mono">{{ formatDuration(log.duration) }}</span>
              <div class="status-col">
                <span class="status-indicator" :class="statusIndicatorClass(log.guard_decision)"></span>
                <span class="status-text">{{ formatGuardDecision(log.guard_decision) }}</span>
              </div>
              <div class="expand-col">
                <ChevronRight 
                  v-if="canExpand(log)" 
                  :size="14" 
                  class="chevron" 
                  :class="{ rotated: expandedRow === i }" 
                />
              </div>
            </div>
          </div>

          <!-- 详情展开面板 -->
          <div v-if="expandedRow === i" class="log-detail-panel">
            <!-- 请求参数 -->
            <div v-if="log.args_json" class="code-block-wrap">
              <div class="code-header">
                <span class="code-title">请求负载 (Payload)</span>
              </div>
              <pre class="code-pre">{{ formatResult(log.args_json) }}</pre>
            </div>
            
            <!-- 执行结果 -->
            <div v-if="hasResult(log)" class="code-block-wrap">
              <div class="code-header">
                <span class="code-title">执行结果 (Result)</span>
                <span 
                  v-if="log.exit_code !== null && log.exit_code !== undefined" 
                  class="exit-badge mono" 
                  :class="{ 'is-error': log.exit_code !== 0 }"
                >
                  退出码 {{ log.exit_code }}
                </span>
              </div>
              <pre class="code-pre">{{ formatResult(log.result) }}</pre>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { usePilotStore } from '../services/pilotStore';
import { 
  Filter, RefreshCw, DatabaseBackup, ChevronDown, ChevronRight,
  TerminalSquare, FileCode2, Globe, Play, Pause,
} from 'lucide-vue-next';

const store = usePilotStore();

const filterTool = ref('');
const filterStatus = ref('');
const expandedRow = ref<number | null>(null);
const autoRefresh = ref(true);
let refreshTimer: ReturnType<typeof setInterval> | null = null;

const filteredLogs = computed(() => {
  return store.logs.filter((log: Record<string, unknown>) => {
    if (filterTool.value && log.tool_type !== filterTool.value) return false;
    if (filterStatus.value && log.guard_decision !== filterStatus.value) return false;
    return true;
  });
});

function hasResult(log: Record<string, unknown>): boolean {
  return !!log.result && String(log.result).length > 0;
}

function canExpand(log: Record<string, unknown>): boolean {
  return hasResult(log) || !!log.args_json;
}

function toggleExpand(i: number, log: Record<string, unknown>) {
  if (!canExpand(log)) return;
  expandedRow.value = expandedRow.value === i ? null : i;
}

function formatArgs(argsJson: string | null | undefined): string {
  if (!argsJson) return '';
  try {
    const parsed = JSON.parse(argsJson);
    if (typeof parsed === 'object' && parsed !== null) {
      if (parsed.command) {
        const cmd = String(parsed.command);
        return cmd.length > 50 ? cmd.slice(0, 50) + '…' : cmd;
      }
      if (parsed.path) return String(parsed.path);
      if (parsed.url) {
        const url = String(parsed.url);
        return url.length > 50 ? url.slice(0, 50) + '…' : url;
      }
      for (const val of Object.values(parsed)) {
        if (typeof val === 'string' && val.length > 0) {
          return val.length > 50 ? val.slice(0, 50) + '…' : val;
        }
      }
    }
  } catch { /* ignore */ }
  return '';
}

function toolPillClass(type: string): string {
  if (type === 'shell') return 'pill-shell';
  if (type === 'file') return 'pill-file';
  return 'pill-browser';
}

function statusIndicatorClass(status: string): string {
  if (status === 'allow' || status === 'confirm') return 'indicator-green';
  if (status === 'deny') return 'indicator-red';
  return 'indicator-yellow';
}

function formatGuardDecision(status: unknown): string {
  if (status === 'allow') return '已放行';
  if (status === 'deny') return '被拦截';
  if (status === 'confirm') return '待确认';
  return String(status || '未知');
}

function formatTime(ts: number): string {
  if (!ts) return '';
  try {
    const d = new Date(ts * 1000);
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
  } catch {
    return String(ts);
  }
}

function formatDuration(dur: number | null | undefined): string {
  if (dur === null || dur === undefined) return '-';
  if (dur < 0.001) return '<1ms';
  if (dur < 1) return `${Math.round(dur * 1000)}ms`;
  return `${dur.toFixed(1)}s`;
}

function formatResult(result: string | null | undefined): string {
  if (!result) return '';
  try {
    const parsed = JSON.parse(result);
    return JSON.stringify(parsed, null, 2);
  } catch {
    return result;
  }
}

function toggleAutoRefresh() {
  autoRefresh.value = !autoRefresh.value;
  if (autoRefresh.value) {
    startAutoRefresh();
  } else {
    stopAutoRefresh();
  }
}

function startAutoRefresh() {
  stopAutoRefresh();
  refreshTimer = setInterval(async () => {
    await store.fetchLogs(100);
  }, 3000);
}

function stopAutoRefresh() {
  if (refreshTimer) {
    clearInterval(refreshTimer);
    refreshTimer = null;
  }
}

async function loadLogs() {
  await store.fetchLogs(100);
}

onMounted(() => {
  loadLogs();
  if (autoRefresh.value) startAutoRefresh();
});

onUnmounted(() => {
  stopAutoRefresh();
});
</script>

<style scoped>
.logs-view {
  flex: 1;
  display: flex;
  flex-direction: column;
  height: 100%;
  animation: fadeIn 0.3s ease;
  background: transparent;
  padding: 16px 20px 24px;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}

.mono {
  font-family: 'SF Mono', 'Cascadia Code', monospace;
}

/* 玻璃质感筛选栏 */
.filter-glass-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 48px;
  padding: 0 16px;
  background: rgba(255, 255, 255, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.8);
  border-bottom: 1px solid rgba(0, 0, 0, 0.08); /* slight separation */
  border-radius: 12px;
  backdrop-filter: blur(20px) saturate(150%);
  -webkit-backdrop-filter: blur(20px) saturate(150%);
  margin-bottom: 16px;
  flex-shrink: 0;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04), 0 1px 2px rgba(0, 0, 0, 0.02);
}

.filter-left, .filter-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.filter-badge {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px 4px 0;
  font-size: 12px;
  font-weight: 700;
  color: #374151; /* Darker for better contrast */
  letter-spacing: 0.2px;
}

.select-wrapper {
  position: relative;
  display: flex;
  align-items: center;
}

.filter-select {
  height: 28px;
  background: rgba(0, 0, 0, 0.04);
  border: 1px solid transparent;
  border-radius: 6px;
  color: #374151;
  padding: 0 24px 0 10px;
  font-size: 12px;
  font-weight: 500;
  outline: none;
  cursor: pointer;
  appearance: none;
  transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1); /* Apple-like easing */
}

.filter-select:hover {
  background: rgba(0, 0, 0, 0.06);
}

.filter-select:focus {
  background: #ffffff;
  border-color: rgba(59, 130, 246, 0.5); /* Blue focus ring */
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.15);
}

.select-icon {
  position: absolute;
  right: 6px;
  pointer-events: none;
  color: #6b7280;
}

/* 右侧控件 */
.auto-refresh-indicator {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  font-weight: 800;
  color: #9ca3af;
  padding: 0 4px;
}

.auto-refresh-indicator.active {
  color: #10b981;
}

.pulse-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #10b981;
  box-shadow: 0 0 0 2px rgba(16, 185, 129, 0.2);
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0%, 100% { transform: scale(1); opacity: 1; }
  50% { transform: scale(0.85); opacity: 0.5; }
}

.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: #6b7280;
  width: 24px;
  height: 24px;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.2s;
}

.icon-btn:hover {
  background: rgba(0, 0, 0, 0.05);
  color: #111827;
}

/* 列表容器 */
.logs-container {
  flex: 1;
  overflow-y: auto;
  border-radius: 12px;
  padding-bottom: 24px;
}

/* 滚动条隐藏或美化 */
.logs-container::-webkit-scrollbar {
  width: 4px;
}
.logs-container::-webkit-scrollbar-thumb {
  background: rgba(0,0,0,0.1);
  border-radius: 4px;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: #9ca3af;
  font-size: 12px;
  gap: 12px;
}

.empty-icon-wrap {
  width: 48px;
  height: 48px;
  border-radius: 12px;
  background: rgba(255, 255, 255, 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: inset 0 0 0 1px rgba(0,0,0,0.05);
}

.logs-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

/* 单条日志 */
.log-item {
  background: rgba(255, 255, 255, 0.6);
  border: 1px solid rgba(0, 0, 0, 0.04);
  border-radius: 10px;
  transition: all 0.2s ease;
  overflow: hidden;
}

.log-item:hover {
  background: rgba(255, 255, 255, 0.9);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.02);
}

.log-item.clickable .log-summary {
  cursor: pointer;
}

.log-item.is-expanded {
  background: #ffffff;
  border-color: rgba(0, 0, 0, 0.08);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.04);
}

.log-summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
}

.log-left {
  display: flex;
  align-items: center;
  gap: 12px;
  max-width: 65%;
}

.time-col {
  font-size: 10px;
  color: #9ca3af;
  min-width: 55px;
}

.tool-col {
  width: 72px;
  flex-shrink: 0;
}

.tool-pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
}

.pill-shell { background: rgba(59, 130, 246, 0.1); color: #2563eb; }
.pill-file { background: rgba(168, 85, 247, 0.1); color: #9333ea; }
.pill-browser { background: rgba(20, 184, 166, 0.1); color: #0d9488; }

.action-col {
  display: flex;
  align-items: center;
  gap: 8px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.action-name {
  font-size: 12px;
  font-weight: 600;
  color: #111827;
}

.action-desc {
  font-size: 11px;
  color: #6b7280;
  /* Code-like presentation for args */
  background: rgba(0,0,0,0.03);
  padding: 2px 6px;
  border-radius: 4px;
  border: 1px solid rgba(0,0,0,0.04);
}

.log-right {
  display: flex;
  align-items: center;
  gap: 16px;
}

.duration {
  font-size: 10px;
  color: #9ca3af;
  text-align: right;
  width: 40px;
}

.status-col {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 70px;
  justify-content: flex-end;
}

.status-indicator {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}

.indicator-green { background: #10b981; box-shadow: 0 0 4px rgba(16, 185, 129, 0.4); }
.indicator-red { background: #ef4444; box-shadow: 0 0 4px rgba(239, 68, 68, 0.4); }
.indicator-yellow { background: #f59e0b; box-shadow: 0 0 4px rgba(245, 158, 11, 0.4); }

.status-text {
  font-size: 11px;
  font-weight: 600;
  color: #4b5563;
  text-transform: capitalize;
}

.expand-col {
  width: 16px;
  display: flex;
  justify-content: flex-end;
}

.chevron {
  color: #9ca3af;
  transition: transform 0.2s ease;
}

.chevron.rotated {
  transform: rotate(90deg);
}

/* 详情面板 */
.log-detail-panel {
  padding: 0 14px 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  border-top: 1px solid rgba(0,0,0,0.03);
  margin-top: 4px;
  padding-top: 12px;
}

.code-block-wrap {
  display: flex;
  flex-direction: column;
  border: 1px solid rgba(0,0,0,0.06);
  border-radius: 8px;
  overflow: hidden;
}

.code-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  background: rgba(0,0,0,0.02);
  border-bottom: 1px solid rgba(0,0,0,0.04);
}

.code-title {
  font-size: 9px;
  font-weight: 800;
  color: #6b7280;
  letter-spacing: 0.5px;
}

.exit-badge {
  font-size: 9px;
  font-weight: 700;
  color: #10b981;
  background: rgba(16, 185, 129, 0.1);
  padding: 1px 6px;
  border-radius: 4px;
}

.exit-badge.is-error {
  color: #ef4444;
  background: rgba(239, 68, 68, 0.1);
}

.code-pre {
  margin: 0;
  padding: 10px 12px;
  background: #fafafa;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  font-size: 11px;
  line-height: 1.5;
  color: #374151;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 250px;
  overflow-y: auto;
}

.code-pre::-webkit-scrollbar {
  width: 4px;
  height: 4px;
}
.code-pre::-webkit-scrollbar-thumb {
  background: rgba(0,0,0,0.15);
  border-radius: 4px;
}
</style>

