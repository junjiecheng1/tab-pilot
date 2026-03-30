<template>
  <div class="logs-view">
    <!-- 筛选栏 -->
    <div class="filter-bar">
      <div class="filter-left">
        <div class="filter-label">
          <Filter :size="14" class="text-tertiary" />
          <span>审计筛选</span>
        </div>
        
        <div class="select-wrapper">
          <select v-model="filterTool" class="filter-select">
            <option value="">全渠道引控 (All)</option>
            <option value="shell">终端操作 (Shell)</option>
            <option value="file">文件操作 (File)</option>
            <option value="browser">浏览器 (Browser)</option>
          </select>
          <ChevronDown :size="14" class="select-icon text-tertiary" />
        </div>

        <div class="select-wrapper">
          <select v-model="filterStatus" class="filter-select">
            <option value="">全状态 (All)</option>
            <option value="allowed">已放行 (Allowed)</option>
            <option value="denied">被拦截 (Denied)</option>
            <option value="confirmed">需介入 (Confirmed)</option>
          </select>
          <ChevronDown :size="14" class="select-icon text-tertiary" />
        </div>
      </div>
      
      <div class="filter-right">
        <div class="auto-refresh-indicator" :class="{ active: autoRefresh }">
          <div class="pulse-dot" v-if="autoRefresh" />
          <span>{{ autoRefresh ? 'LIVE' : 'PAUSED' }}</span>
        </div>
        <button class="filter-refresh-btn" @click="toggleAutoRefresh" :title="autoRefresh ? '暂停自动刷新' : '开启自动刷新'">
          <Pause v-if="autoRefresh" :size="14" />
          <Play v-else :size="14" />
        </button>
        <button class="filter-refresh-btn" @click="loadLogs" title="立即刷新">
          <RefreshCw :size="14" />
        </button>
      </div>
    </div>

    <!-- 日志表格 -->
    <div class="card logs-card">
      <div v-if="store.logs.length === 0" class="empty-state">
        <DatabaseBackup :size="32" stroke-width="1.5" class="text-tertiary" />
        <p>审计日志为空，尚未记录任何自动化操作</p>
      </div>
      
      <div v-else class="table-container">
        <table class="logs-table">
          <thead>
            <tr>
              <th class="col-expand"></th>
              <th class="col-time">时间戳</th>
              <th class="col-tool">工具引擎</th>
              <th>执行负载 (Payload)</th>
              <th class="col-duration">耗时</th>
              <th class="col-status">安全断言</th>
            </tr>
          </thead>
          <tbody>
            <template v-for="(log, i) in filteredLogs" :key="i">
              <tr
                :class="{ 'row-expandable': canExpand(log), 'row-expanded': expandedRow === i }"
                @click="toggleExpand(i, log)"
              >
                <td class="col-expand">
                  <ChevronRight
                    v-if="hasResult(log)"
                    :size="14"
                    class="expand-icon"
                    :class="{ rotated: expandedRow === i }"
                  />
                </td>
                <td class="col-time mono">{{ formatTime(log.timestamp) }}</td>
                <td class="col-tool">
                  <span class="tool-badge" :class="toolBadgeClass(log.tool_type)">
                    <TerminalSquare v-if="log.tool_type === 'shell'" :size="12" />
                    <FileCode2 v-else-if="log.tool_type === 'file'" :size="12" />
                    <Globe v-else :size="12" />
                    {{ log.tool_type }}
                  </span>
                </td>
                <td class="col-action mono">
                  <span class="action-label">{{ log.action }}</span>
                  <span v-if="formatArgs(log.args_json)" class="action-args">{{ formatArgs(log.args_json) }}</span>
                </td>
                <td class="col-duration mono">{{ formatDuration(log.duration) }}</td>
                <td class="col-status">
                  <span class="badge" :class="statusBadgeClass(log.guard_decision)">
                    <Check v-if="log.guard_decision === 'allow' || log.guard_decision === 'confirm'" :size="12" />
                    <Ban v-else-if="log.guard_decision === 'deny'" :size="12" />
                    <Clock v-else :size="12" />
                    {{ log.guard_decision }}
                  </span>
                </td>
              </tr>
              <!-- 展开行: 请求 + 结果 -->
              <tr v-if="expandedRow === i" class="result-row">
                <td colspan="6">
                  <div class="result-content">
                    <!-- 请求参数 -->
                    <div v-if="log.args_json" class="result-section">
                      <div class="result-header">
                        <span class="result-label">请求参数</span>
                      </div>
                      <pre class="result-pre">{{ formatResult(log.args_json) }}</pre>
                    </div>
                    <!-- 执行结果 -->
                    <div v-if="hasResult(log)" class="result-section">
                      <div class="result-header">
                        <span class="result-label">执行结果</span>
                        <span v-if="log.exit_code !== null && log.exit_code !== undefined" class="exit-code" :class="{ error: log.exit_code !== 0 }">
                          exit={{ log.exit_code }}
                        </span>
                      </div>
                      <pre class="result-pre">{{ formatResult(log.result) }}</pre>
                    </div>
                  </div>
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { usePilotStore } from '../services/pilotStore';
import { 
  Filter, RefreshCw, DatabaseBackup, ChevronDown, ChevronRight,
  TerminalSquare, FileCode2, Globe, Check, Ban, Clock,
  Play, Pause,
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
      // shell: 显示 command
      if (parsed.command) {
        const cmd = String(parsed.command);
        return cmd.length > 80 ? cmd.slice(0, 80) + '…' : cmd;
      }
      // file: 显示 path
      if (parsed.path) return String(parsed.path);
      // browser: 显示 url
      if (parsed.url) {
        const url = String(parsed.url);
        return url.length > 80 ? url.slice(0, 80) + '…' : url;
      }
      // 通用: 第一个字符串值
      for (const val of Object.values(parsed)) {
        if (typeof val === 'string' && val.length > 0) {
          return val.length > 80 ? val.slice(0, 80) + '…' : val;
        }
      }
    }
  } catch { /* ignore */ }
  return '';
}

function toolBadgeClass(type: string): string {
  if (type === 'shell') return 'bg-blue';
  if (type === 'file') return 'bg-purple';
  return 'bg-teal';
}

function statusBadgeClass(status: string): string {
  if (status === 'allow' || status === 'confirm') return 'badge-green';
  if (status === 'deny') return 'badge-red';
  return 'badge-yellow';
}

function formatTime(ts: number): string {
  if (!ts) return '';
  try {
    const d = new Date(ts * 1000);
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}.${d.getMilliseconds().toString().padStart(3, '0')}`;
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
  // 尝试格式化 JSON
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
  min-height: 0;
  animation: fadeIn 0.3s ease;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}

.filter-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
  gap: 12px;
  flex-shrink: 0;
}

.filter-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.filter-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.filter-label {
  display: flex;
  align-items: center;
  gap: 6px;
  padding-right: 4px;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
}

.select-wrapper {
  position: relative;
  display: flex;
  align-items: center;
}

.filter-select {
  background: var(--bg-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  color: var(--text-primary);
  padding: 6px 32px 6px 12px;
  font-size: 13px;
  font-weight: 500;
  outline: none;
  cursor: pointer;
  appearance: none;
  transition: all 0.2s;
  box-shadow: var(--shadow-sm);
}

.filter-select:hover {
  border-color: rgba(51, 112, 255, 0.3);
  background: var(--bg-hover);
}

.filter-select:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px rgba(51, 112, 255, 0.1);
  background: var(--bg-card);
}

.select-icon {
  position: absolute;
  right: 10px;
  pointer-events: none;
}

/* 自动刷新指示器 */
.auto-refresh-indicator {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.5px;
  color: var(--text-tertiary);
  padding: 4px 10px;
  border-radius: var(--radius-lg);
  background: var(--bg-input);
  transition: all 0.3s;
}

.auto-refresh-indicator.active {
  color: #10b981;
  background: rgba(16, 185, 129, 0.08);
}

.pulse-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #10b981;
  animation: pulse 2s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.4; transform: scale(0.8); }
}

.filter-refresh-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--bg-card);
  border: 1px solid var(--border-subtle);
  box-shadow: var(--shadow-sm);
  color: var(--text-secondary);
  width: 32px;
  height: 32px;
  border-radius: var(--radius-lg);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.2s;
}

.filter-refresh-btn:hover {
  color: var(--accent);
  background: rgba(51, 112, 255, 0.04);
  border-color: rgba(51, 112, 255, 0.3);
}

.logs-card {
  flex: 1;
  display: flex;
  flex-direction: column;
  padding: 0;
  margin-bottom: 0;
  min-height: 0;
  border-radius: var(--radius-lg);
  overflow: hidden;
}

.table-container {
  flex: 1;
  overflow: auto;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  color: var(--text-tertiary);
  padding: 60px 20px;
  font-size: 13px;
}

.logs-table {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 13px;
}

.logs-table th {
  text-align: left;
  padding: 12px 16px;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--border);
  background: var(--bg-input);
  position: sticky;
  top: 0;
  z-index: 1;
}

.logs-table td {
  padding: 10px 16px;
  border-bottom: 1px solid var(--border-subtle);
  vertical-align: middle;
}

.logs-table tr:last-child td {
  border-bottom: none;
}

.logs-table tbody tr:hover td {
  background: var(--bg-hover);
}

.row-expandable {
  cursor: pointer;
}

.row-expanded td {
  background: rgba(51, 112, 255, 0.02);
  border-bottom-color: transparent;
}

/* 列宽 */
.col-expand {
  width: 28px;
  padding: 10px 4px 10px 12px !important;
}

.col-time {
  width: 110px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.col-tool {
  width: 100px;
}

.col-duration {
  width: 70px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.col-status {
  width: 110px;
}

.col-action {
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12.5px;
}

.action-label {
  font-weight: 600;
  margin-right: 8px;
}

.action-args {
  color: var(--text-tertiary);
  font-weight: 400;
}

/* 展开箭头 */
.expand-icon {
  color: var(--text-tertiary);
  transition: transform 0.2s ease;
}

.expand-icon.rotated {
  transform: rotate(90deg);
}

/* 结果展开行 */
.result-row td {
  padding: 0 16px 12px 16px !important;
  background: rgba(51, 112, 255, 0.02);
}

.result-content {
  background: var(--bg-input);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  overflow: hidden;
}

.result-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border-subtle);
  background: rgba(0, 0, 0, 0.02);
}

.result-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.exit-code {
  font-size: 11px;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  color: #10b981;
  font-weight: 500;
}

.exit-code.error {
  color: #ef4444;
}

.result-pre {
  margin: 0;
  padding: 10px 12px;
  font-size: 12px;
  font-family: 'SF Mono', 'Cascadia Code', monospace;
  line-height: 1.5;
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 300px;
  overflow-y: auto;
}

.tool-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.bg-blue { background: rgba(51, 112, 255, 0.1); color: var(--blue); }
.bg-purple { background: rgba(147, 51, 234, 0.1); color: #9333ea; }
.bg-teal { background: rgba(20, 184, 166, 0.1); color: #14b8a6; }
</style>
