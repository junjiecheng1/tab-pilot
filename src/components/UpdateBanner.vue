<template>
  <!-- 浮动更新卡片 — 右下角弹出 -->
  <Teleport to="body">
    <Transition name="float-up">
      <div v-if="showCard" class="update-float">
        <!-- 有更新 -->
        <template v-if="updateStatus === 'available'">
          <div class="float-header">
            <div class="float-badge">
              <ArrowUpCircle :size="15" />
            </div>
            <div class="float-title">
              <span class="title-text">发现新版本</span>
              <span class="version-tag">v{{ newVersion }}</span>
            </div>
            <button class="close-btn" @click="dismiss">
              <X :size="12" />
            </button>
          </div>
          <div class="float-body">
            <button
              class="update-action"
              @click="installUpdate"
              :disabled="downloading"
            >
              <template v-if="downloading">
                <RefreshCw :size="13" class="spin" />
                <span>下载中 {{ downloadProgress }}%</span>
                <div class="progress-track">
                  <div class="progress-fill" :style="{ width: downloadProgress + '%' }" />
                </div>
              </template>
              <template v-else>
                <Download :size="13" />
                <span>下载并安装</span>
              </template>
            </button>
          </div>
        </template>

        <!-- 错误 -->
        <template v-else-if="updateStatus === 'error'">
          <div class="float-header">
            <div class="float-badge error">
              <XCircle :size="15" />
            </div>
            <span class="title-text">检查更新失败</span>
            <button class="close-btn" @click="dismiss">
              <X :size="12" />
            </button>
          </div>
        </template>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { ArrowUpCircle, XCircle, RefreshCw, Download, X } from 'lucide-vue-next';

type UpdateStatus = 'idle' | 'checking' | 'available' | 'uptodate' | 'error';

const updateStatus = ref<UpdateStatus>('idle');
const newVersion = ref('');
const downloading = ref(false);
const downloadProgress = ref(0);

let updateObj: Awaited<ReturnType<typeof check>> | null = null;

const showCard = computed(() =>
  ['available', 'error'].includes(updateStatus.value)
);

async function checkForUpdate() {
  updateStatus.value = 'checking';
  try {
    // 先 fetch CDN JSON (加时间戳绕过缓存)
    const update = await check({
      headers: {
        'Cache-Control': 'no-cache',
      },
    });
    if (update) {
      updateObj = update;
      newVersion.value = update.version;
      updateStatus.value = 'available';
    } else {
      updateStatus.value = 'idle';
    }
  } catch (e) {
    console.error('[Updater]', e);
    updateStatus.value = 'error';
    setTimeout(() => { updateStatus.value = 'idle'; }, 8000);
  }
}

async function installUpdate() {
  if (!updateObj) return;
  downloading.value = true;
  downloadProgress.value = 0;

  try {
    await updateObj.downloadAndInstall((event) => {
      if (event.event === 'Started' && event.data.contentLength) {
        downloadProgress.value = 0;
      } else if (event.event === 'Progress') {
        downloadProgress.value = Math.min(
          99,
          downloadProgress.value + Math.round((event.data.chunkLength / (1024 * 1024)) * 10)
        );
      } else if (event.event === 'Finished') {
        downloadProgress.value = 100;
      }
    });
    await relaunch();
  } catch (e) {
    console.error('[Updater] Install failed:', e);
    downloading.value = false;
    updateStatus.value = 'error';
  }
}

function dismiss() {
  updateStatus.value = 'idle';
}

onMounted(() => {
  setTimeout(checkForUpdate, 5000);
});

defineExpose({ checkForUpdate });
</script>

<style scoped>
/* ── 浮动卡片 ── */
.update-float {
  position: fixed;
  bottom: 20px;
  right: 20px;
  z-index: 9000;
  width: 260px;
  background: var(--bg-card, #fff);
  border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
  border-radius: 12px;
  padding: 14px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
}

/* ── Header ── */
.float-header {
  display: flex;
  align-items: center;
  gap: 8px;
}

.float-badge {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border-radius: 7px;
  background: var(--bg-hover, rgba(0, 0, 0, 0.04));
  color: var(--text-secondary);
  flex-shrink: 0;
}

.float-badge.error {
  color: var(--text-tertiary);
}

.float-title {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
}

.title-text {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}

.version-tag {
  font-size: 11px;
  font-weight: 500;
  color: var(--text-secondary);
  background: var(--bg-hover, rgba(0, 0, 0, 0.04));
  padding: 1px 6px;
  border-radius: 4px;
}

.close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 6px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  opacity: 0.4;
  transition: all 0.15s;
  margin-left: auto;
  flex-shrink: 0;
}

.close-btn:hover {
  opacity: 1;
  background: var(--bg-hover, rgba(0, 0, 0, 0.04));
}

/* ── Body ── */
.float-body {
  margin-top: 10px;
}

/* ── 更新按钮 ── */
.update-action {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 100%;
  padding: 7px 0;
  border-radius: 8px;
  border: 1px solid var(--border, rgba(0, 0, 0, 0.08));
  background: var(--bg-card, #fff);
  color: var(--text-primary);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  position: relative;
  overflow: hidden;
}

.update-action:hover:not(:disabled) {
  background: var(--bg-hover, rgba(0, 0, 0, 0.04));
}

.update-action:active:not(:disabled) {
  transform: scale(0.98);
}

.update-action:disabled {
  opacity: 0.5;
  cursor: default;
}

/* 下载进度条 */
.progress-track {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--border, rgba(0, 0, 0, 0.06));
}

.progress-fill {
  height: 100%;
  background: var(--text-secondary);
  transition: width 0.3s ease;
  border-radius: 0 2px 2px 0;
}

/* ── 动画 ── */
.float-up-enter-active {
  transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
}
.float-up-leave-active {
  transition: all 0.2s ease;
}
.float-up-enter-from {
  opacity: 0;
  transform: translateY(12px) scale(0.97);
}
.float-up-leave-to {
  opacity: 0;
  transform: translateY(6px);
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  100% { transform: rotate(360deg); }
}
</style>
