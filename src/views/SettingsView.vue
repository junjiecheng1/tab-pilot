<template>
  <div class="settings-view">
    <div v-if="store.status" class="settings-container">
      
      <!-- 账号偏好 -->
      <div class="settings-section">
        <h2 class="section-title">云端账号与连接</h2>
        <div class="section-body">
          <div class="setting-item">
            <div class="setting-info">
              <div class="setting-name">
                <UserCircle2 :size="16" class="setting-icon text-tertiary" />
                主要授权身份
              </div>
              <div class="setting-desc">{{ isLoggedIn ? (userDisplay || '安全连接至云端主服务') : '尚未登录，部分离线功能可能受限' }}</div>
            </div>
            <div class="setting-action">
              <button v-if="!isLoggedIn" class="btn btn-primary btn-sm" @click="login">
                <ExternalLink :size="14" />
                网页授权
              </button>
              <div v-else class="status-group">
                <span class="connected-indicator">
                  <span class="dot-online"></span>
                  已连接
                </span>
                <span class="divider-dot">·</span>
                <button class="btn-link-danger" @click="logout">退出</button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- 工作目录与权限 -->
      <div class="settings-section">
        <h2 class="section-title">系统引擎与权限约束</h2>
        <div class="section-body">
          <div class="setting-item">
            <div class="setting-info">
              <div class="setting-name">
                <FolderOpen :size="16" class="setting-icon text-tertiary" />
                默认安全工作空间
              </div>
              <div class="setting-desc">所有主动探针和文件操作，默认约束在此目标内执行</div>
            </div>
            <div class="setting-action workspace-action">
              <div class="workspace-path mono" :title="store.workspace || '尚未选择工作目录'">
                {{ store.workspace || '未指定' }}
              </div>
              <button class="btn btn-ghost btn-sm" @click="selectWorkspace">更改</button>
            </div>
          </div>

          <div class="setting-item">
            <div class="setting-info">
              <div class="setting-name">桌面浏览器协议接管</div>
              <div class="setting-desc">授权底层引擎接管宿主机视觉任务并驱动高权浏览器</div>
            </div>
            <div class="setting-action">
              <label class="toggle">
                <input type="checkbox" :checked="store.browserEnabled" @change="toggleBrowser" />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>

          <div class="setting-item">
            <div class="setting-info">
              <div class="setting-name">守护进程无感自启</div>
              <div class="setting-desc">跟随系统启动并在托盘常驻，随时接受事件池调度</div>
            </div>
            <div class="setting-action">
              <label class="toggle">
                <input type="checkbox" v-model="autoStart" />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>

          <div class="setting-item">
            <div class="setting-info">
              <div class="setting-name">系统托盘图标</div>
              <div class="setting-desc">在菜单栏 / 任务栏显示常驻图标，可快速唤起窗口</div>
            </div>
            <div class="setting-action">
              <label class="toggle">
                <input type="checkbox" v-model="trayVisible" />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>
        </div>
      </div>

      <!-- 交互与审计 -->
      <div class="settings-section">
        <h2 class="section-title">本地留痕与偏好</h2>
        <div class="section-body">
          <div class="setting-item">
            <div class="setting-info">
              <div class="setting-name">外观倾向</div>
              <div class="setting-desc">跟随操作系统时间调度，或指定全局黑暗质感模式</div>
            </div>
            <div class="setting-action">
              <div class="select-wrapper">
                <select v-model="themePreference" class="setting-select" @change="applyTheme">
                  <option value="system">跟随系统 (Auto)</option>
                  <option value="light">浅色外观 (Light)</option>
                  <option value="dark">深色外观 (Dark)</option>
                </select>
                <ChevronDown :size="14" class="select-icon text-tertiary" />
              </div>
            </div>
          </div>

          <div class="setting-item">
            <div class="setting-info">
              <div class="setting-name">工具调用强落库</div>
              <div class="setting-desc">在本地 SQLite 源上对产生的敏感指令与行为流进行快照留痕</div>
            </div>
            <div class="setting-action">
              <label class="toggle">
                <input type="checkbox" :checked="store.auditEnabled" @change="toggleAudit" />
                <span class="toggle-slider"></span>
              </label>
            </div>
          </div>
        </div>
      </div>

      <!-- 诊断与链路 -->
      <div class="settings-section">
        <h2 class="section-title">引擎态诊断链路</h2>
        <div class="section-body diagnostic-body">
          <div class="about-row">
            <span class="about-label">通信入口点 (Endpoint)</span>
            <span class="about-value mono">{{ endpointHost }}</span>
          </div>
          <div class="about-row">
            <span class="about-label">主管道健康度 (State)</span>
            <span class="about-value" style="gap: 8px;">
              <span :class="['pulse-dot', store.isConnected ? 'connected' : store.serverReachable ? 'reachable' : 'disconnected']"></span>
              <span class="kernel-badge">
                {{ store.isConnected ? '已建立连接' : store.serverReachable ? '在线 · 未授权' : '服务不可达' }}
              </span>
            </span>
          </div>
          <div class="about-row">
            <span class="about-label">Core Kernel</span>
            <span class="about-value kernel-badges">
              <span class="kernel-badge version">{{ kernelVersion }}</span>
              <span class="kernel-badge os">{{ kernelOS }}</span>
              <span class="kernel-badge arch">{{ kernelArch }}</span>
            </span>
          </div>
          <div class="about-row">
            <span class="about-label">CLI 工具 (rg, fd, jq, yq)</span>
            <span class="about-value">
              <span :class="['pulse-dot', store.toolsReady ? 'connected' : 'disconnected']"></span>
              <span class="kernel-badge">
                {{ store.toolsReady ? '已就绪' : '未安装' }}
              </span>
            </span>
          </div>
          <div class="about-row">
            <span class="about-label">当前版本</span>
            <span class="about-value version-row">
              <span class="kernel-badge version">v{{ appVersion }}</span>
              <button
                class="update-btn"
                @click="updateAvailable ? doInstall() : checkUpdate()"
                :disabled="updateChecking || updateInstalling"
              >
                <RefreshCw :size="11" :class="{ spin: updateChecking || updateInstalling }" />
                <span v-if="updateInstalling">安装中 {{ updateProgress }}%</span>
                <span v-else-if="updateAvailable">更新到 v{{ updateVersion }}</span>
                <span v-else>{{ updateStatusText }}</span>
              </button>
            </span>
          </div>
        </div>
      </div>

    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, computed, onMounted } from 'vue';
import {
  logout as doLogout, setWorkspace,
  setBrowserEnabled, setAuditEnabled
} from '../services/bridge';
import { usePilotStore } from '../services/pilotStore';
import {
  UserCircle2, ExternalLink, Check, FolderOpen,
  FolderEdit, Info, Settings2, Activity, ChevronDown, RefreshCw
} from 'lucide-vue-next';

const store = usePilotStore();

// 仅本地 UI 状态
const autoStart = ref(false);
const trayVisible = ref(true);

const themePreference = ref(localStorage.getItem('theme-preference') || 'system');

// 版本 + 更新
const appVersion = ref('0.1.0');
const updateChecking = ref(false);
const updateStatusText = ref('检查更新');
const updateAvailable = ref(false);
const updateVersion = ref('');
const updateInstalling = ref(false);
const updateProgress = ref(0);

let updateObj: any = null;

async function checkUpdate() {
  updateChecking.value = true;
  updateStatusText.value = '检查中...';
  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check({
      headers: { 'Cache-Control': 'no-cache' },
    });
    if (update) {
      updateObj = update;
      updateVersion.value = update.version;
      updateAvailable.value = true;
    } else {
      updateStatusText.value = '已是最新';
      setTimeout(() => { updateStatusText.value = '检查更新'; }, 3000);
    }
  } catch (e) {
    console.error('[Updater]', e);
    updateStatusText.value = '检查失败';
    setTimeout(() => { updateStatusText.value = '检查更新'; }, 3000);
  } finally {
    updateChecking.value = false;
  }
}

async function doInstall() {
  if (!updateObj) return;
  updateInstalling.value = true;
  updateProgress.value = 0;
  try {
    await updateObj.downloadAndInstall((event: any) => {
      if (event.event === 'Progress') {
        updateProgress.value = Math.min(
          99,
          updateProgress.value + Math.round((event.data.chunkLength / (1024 * 1024)) * 10)
        );
      } else if (event.event === 'Finished') {
        updateProgress.value = 100;
      }
    });
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
  } catch (e) {
    console.error('[Updater] Install failed:', e);
    updateInstalling.value = false;
    updateAvailable.value = false;
    updateStatusText.value = '安装失败';
    setTimeout(() => { updateStatusText.value = '检查更新'; }, 3000);
  }
}

// 读取 app 版本
async function loadVersion() {
  try {
    const { getVersion } = await import('@tauri-apps/api/app');
    appVersion.value = await getVersion();
  } catch { /* dev mode */ }
}

// 解析 kernel 版本: "1.0.0 (Darwin_arm64)" → 拆分为 badge
const OS_MAP: Record<string, string> = { Darwin: 'macOS', Windows: 'Windows', Linux: 'Linux' };
const ARCH_MAP: Record<string, string> = { arm64: 'Apple Silicon', x86_64: 'Intel x64', aarch64: 'ARM64' };

const kernelVersion = computed(() => {
  const v = store.version || '';
  return v.split(' ')[0] || 'v?';
});
const kernelOS = computed(() => {
  const m = (store.version || '').match(/\(([\w]+)_/);
  const raw = m?.[1] || '';
  return OS_MAP[raw] || raw || '?';
});
const kernelArch = computed(() => {
  const m = (store.version || '').match(/_([\w]+)\)?/);
  const raw = m?.[1] || '';
  return ARCH_MAP[raw] || raw || '?';
});

// 从 ws://host:port/ws/pilot 提取 host:port
const endpointHost = computed(() => {
  try {
    const cleaned = store.serverUrl
      .replace(/^wss?:\/\//, '');
    return cleaned.split('/')[0] || store.serverUrl;
  } catch {
    return store.serverUrl;
  }
});

const isLoggedIn = computed(() => store.status?.running && store.isConnected);
const userDisplay = computed(() => store.userDisplay);


onMounted(async () => {
  applyTheme();
  await store.fetchStatus();
  await loadVersion();

  // 读取自启动状态
  try {
    const { isEnabled } = await import('@tauri-apps/plugin-autostart');
    autoStart.value = await isEnabled();
  } catch (e) {
    console.error('[Settings] autostart 读取失败:', e);
  }

  // 读取托盘状态
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const visible = await invoke<boolean>('get_tray_visible');
    trayVisible.value = visible;
  } catch { /* 默认 true */ }
});

// 自启动 Toggle 同步
watch(autoStart, async (val) => {
  try {
    const { enable, disable } = await import('@tauri-apps/plugin-autostart');
    if (val) {
      await enable();
    } else {
      await disable();
    }
  } catch (e) {
    console.error('[Settings] autostart 设置失败:', e);
  }
});

// 托盘图标 Toggle
watch(trayVisible, async (val) => {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_tray_visible', { visible: val });
  } catch (e) {
    console.error('[Settings] 托盘设置失败:', e);
  }
});

// 浏览器接管 Toggle
async function toggleBrowser(e: Event) {
  const val = (e.target as HTMLInputElement).checked;
  await store.setSetting(() => setBrowserEnabled(val));
}

// 审计落库 Toggle
async function toggleAudit(e: Event) {
  const val = (e.target as HTMLInputElement).checked;
  await store.setSetting(() => setAuditEnabled(val));
}

async function login() {
  const base = store.serverUrl
    .replace(/^wss:\/\//, 'https://')
    .replace(/^ws:\/\//, 'http://')
    .replace(/\/ws\/.*$/, '');

  let challenge = '';
  try {
    const { getAuthChallenge } = await import('@/services/bridge');
    challenge = await getAuthChallenge();
  } catch { /* ignore */ }

  const authUrl = `${base}/auth/pilot?challenge=${challenge}`;
  try {
    const { open } = await import('@tauri-apps/plugin-shell');
    await open(authUrl);
  } catch {
    window.open(authUrl, '_blank');
  }
}

async function logout() {
  await doLogout();
  await store.refresh();  // 重新获取已断开的状态
}

async function selectWorkspace() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const selected = await open({ directory: true, title: '选择工作空间' });
    if (selected) {
      await store.setSetting(() => setWorkspace(selected as string));
    }
  } catch {
    const path = prompt('为 Agent 指定工作目录:', store.workspace || '/Users');
    if (path) {
      await store.setSetting(() => setWorkspace(path));
    }
  }
}

function applyTheme() {
  localStorage.setItem('theme-preference', themePreference.value);
  if (themePreference.value === 'system') {
    document.documentElement.removeAttribute('data-theme');
    document.documentElement.classList.remove('dark', 'light');
  } else {
    document.documentElement.setAttribute('data-theme', themePreference.value);
    document.documentElement.classList.remove('dark', 'light');
    document.documentElement.classList.add(themePreference.value);
  }
}
</script>

<style scoped>
.settings-view {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  animation: fadeIn 0.3s ease;
  min-height: 0;
  overflow-y: auto;
  padding: 0 4px 40px 0;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: translateY(0); }
}

.settings-container {
  width: 100%;
  max-width: 760px;
  display: flex;
  flex-direction: column;
  gap: 32px;
}

/* 段落体系 */
.settings-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding-left: 4px;
}

.section-body {
  background: var(--bg-card);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-lg);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.02);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* 列表项 */
.setting-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px;
  background: var(--bg-card);
  border-bottom: 1px solid var(--border-subtle);
  transition: background 0.2s;
}

.setting-item:hover {
  background: var(--bg-hover);
}

.setting-item:last-child {
  border-bottom: none;
}

.setting-info {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  gap: 4px;
}

.setting-name {
  font-weight: 500;
  font-size: 14px;
  color: var(--text-primary);
  display: flex;
  align-items: center;
  gap: 8px;
}

.setting-desc {
  font-size: 13px;
  color: var(--text-tertiary);
  line-height: 1.5;
  padding-right: 16px;
}

.setting-action {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 12px;
}

/* 按钮组合 */
.status-group {
  display: flex;
  align-items: center;
  gap: 8px;
}

.connected-indicator {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 500;
  color: var(--green);
}

.dot-online {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--green);
  box-shadow: 0 0 0 2px rgba(35, 195, 67, 0.15);
}

.divider-dot {
  color: var(--text-quaternary);
  font-size: 12px;
}

.btn-link-danger {
  background: none;
  border: none;
  color: var(--text-tertiary);
  font-size: 12px;
  cursor: pointer;
  padding: 0;
  transition: color 0.15s;
}

.btn-link-danger:hover {
  color: var(--red);
}

/* 工作空间块 */
.workspace-action {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--bg-input);
  padding: 4px 4px 4px 12px;
  border-radius: var(--radius);
  border: 1px solid var(--border-subtle);
}

.workspace-path {
  font-size: 13px;
  color: var(--text-secondary);
  max-width: 320px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Select */
.select-wrapper {
  position: relative;
  display: flex;
  align-items: center;
}

.setting-select {
  background: var(--bg-input);
  border: 1px solid var(--border-subtle);
  color: var(--text-primary);
  border-radius: var(--radius);
  padding: 6px 32px 6px 12px;
  font-size: 13px;
  font-weight: 500;
  outline: none;
  cursor: pointer;
  appearance: none;
  transition: all 0.2s;
}

.setting-select:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px rgba(51, 112, 255, 0.1);
  background: var(--bg-card);
}

.select-icon {
  position: absolute;
  right: 10px;
  pointer-events: none;
}

/* Toggle Switch */
.toggle {
  position: relative;
  display: inline-block;
  width: 44px;
  height: 24px;
  margin: 0;
}

.toggle input {
  opacity: 0;
  width: 0;
  height: 0;
}

.toggle-slider {
  position: absolute;
  cursor: pointer;
  inset: 0;
  background-color: var(--border);
  border-radius: 24px;
  transition: .3s;
}

.toggle-slider:before {
  position: absolute;
  content: "";
  height: 20px;
  width: 20px;
  left: 2px;
  bottom: 2px;
  background-color: white;
  border-radius: 50%;
  transition: .3s;
  box-shadow: 0 2px 4px rgba(0,0,0,0.15);
}

.toggle input:checked + .toggle-slider {
  background-color: var(--green);
}

.toggle input:checked + .toggle-slider:before {
  transform: translateX(20px);
}

/* 诊断信息行 */
.diagnostic-body {
  padding: 8px 16px;
}

.about-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 0;
  border-bottom: 1px dashed var(--border-subtle);
}

.about-row:last-child {
  border-bottom: none;
}

.about-label {
  color: var(--text-secondary);
  font-size: 13px;
}

.about-value {
  font-size: 13px;
  color: var(--text-primary);
  display: flex;
  align-items: center;
}

.about-value.mono {
  color: var(--text-tertiary);
}

.pulse-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  display: inline-block;
}

.pulse-dot.connected {
  background: var(--green);
  box-shadow: 0 0 0 2px rgba(35, 195, 67, 0.2);
}

.pulse-dot.reachable {
  background: var(--yellow);
  box-shadow: 0 0 0 2px rgba(255, 180, 0, 0.2);
}

.pulse-dot.disconnected {
  background: var(--red);
}

/* Kernel 版本 badges */
.kernel-badges {
  display: flex;
  gap: 6px;
  align-items: center;
}

.kernel-badge {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
  font-family: 'SF Mono', 'Menlo', monospace;
  letter-spacing: 0.3px;
  background: var(--bg-hover);
  color: var(--text-secondary);
  border: 1px solid var(--border-subtle);
}

/* 版本行 */
.version-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.update-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 10px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--bg-card);
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.update-btn:hover:not(:disabled) {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.update-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
