// browser-bridge.js — TabPilot 浏览器桥接
//
// 通过 stdin/stdout JSON-RPC 与 Rust BrowserExecutor 通信
// 底层使用 Playwright 控制 Chromium
//
// 协议: 每行一个 JSON, id + method + params

const { chromium } = require('playwright-core');
const readline = require('readline');

// ── 状态 ──────────────────────

let context = null;  // BrowserContext (持久化)
let page = null;     // 当前页面
let recording = false; // 是否在录制
let tracePath = null;  // trace 文件路径

// ── Action 分派表 ──────────────

const handlers = {

  // 启动浏览器 (持久化上下文 — 保留登录态)
  async launch({ headless = false, userDataDir, viewport, recordDir }) {
    // 幂等: 已启动则验证还活着
    if (context && page) {
      try {
        const url = page.url(); // 验证 page 还活着
        return { ok: true, url, recording, already_running: true };
      } catch {
        // page/context 已关闭, 重置后重新启动
        context = null;
        page = null;
        recording = false;
        tracePath = null;
      }
    }

    context = await chromium.launchPersistentContext(userDataDir || '', {
      headless,
      viewport: viewport || { width: 1280, height: 800 },
      locale: 'zh-CN',
      timezoneId: 'Asia/Shanghai',
      args: [
        '--disable-blink-features=AutomationControlled',
      ],
    });
    page = context.pages()[0] || await context.newPage();

    // 录制: 启动 Playwright Tracing
    if (recordDir) {
      const fs = require('fs');
      fs.mkdirSync(recordDir, { recursive: true });
      tracePath = require('path').join(
        recordDir,
        `trace-${Date.now()}.zip`,
      );
      await context.tracing.start({
        screenshots: true,
        snapshots: true,
      });
      recording = true;
    }

    return { ok: true, url: page.url(), recording };
  },

  // 导航
  async navigate({ url, waitUntil = 'domcontentloaded' }) {
    await page.goto(url, { waitUntil, timeout: 30000 });
    return { url: page.url(), title: await page.title() };
  },

  // 点击
  async click({ selector, timeout = 5000 }) {
    await page.click(selector, { timeout });
    await page.waitForTimeout(500);
    return { ok: true };
  },

  // 填写输入框
  async fill({ selector, text, timeout = 5000 }) {
    await page.fill(selector, text, { timeout });
    return { ok: true };
  },

  // 逐字符输入 (搜索联想)
  async type({ selector, text, delay = 50 }) {
    await page.type(selector, text, { delay });
    return { ok: true };
  },

  // 按键 (Enter, Tab, Escape 等)
  async press({ key }) {
    await page.keyboard.press(key);
    await page.waitForTimeout(300);
    return { ok: true };
  },

  // 滚动
  async scroll({ direction = 'down', amount = 500 }) {
    const delta = direction === 'down' ? amount : -amount;
    await page.mouse.wheel(0, delta);
    await page.waitForTimeout(300);
    return { ok: true };
  },

  // 等待
  async wait({ ms = 1000 }) {
    await page.waitForTimeout(ms);
    return { ok: true };
  },

  // 等待元素出现
  async wait_for({ selector, timeout = 10000, state = 'visible' }) {
    await page.waitForSelector(selector, { timeout, state });
    return { ok: true };
  },

  // ARIA 无障碍树快照 (给 LLM 看)
  async snapshot({ maxNodes = 150 }) {
    let tree = null;

    // 方式 1: Playwright accessibility API
    if (page.accessibility && typeof page.accessibility.snapshot === 'function') {
      tree = await page.accessibility.snapshot({ interestingOnly: true });
    }

    // 方式 2: 通过 CDP 直接获取
    if (!tree) {
      try {
        const client = await page.context().newCDPSession(page);
        const { nodes } = await client.send('Accessibility.getFullAXTree');
        await client.detach();
        // 简化 CDP 树为平面结构
        const simplified = [];
        for (const node of nodes.slice(0, maxNodes * 2)) {
          const name = (node.name || {}).value || '';
          const role = (node.role || {}).value || '';
          const value = (node.value || {}).value || '';
          if (name || value) {
            simplified.push({
              ref: simplified.length,
              role,
              name: name.slice(0, 80),
              value: value.slice(0, 80),
              level: node.depth || 0,
              focused: false,
            });
          }
          if (simplified.length >= maxNodes) break;
        }
        return {
          url: page.url(),
          title: await page.title(),
          nodes: simplified,
          node_count: simplified.length,
        };
      } catch (cdpErr) {
        // CDP 也失败, 兜底
      }
    }

    const nodes = flattenTree(tree, maxNodes);
    return {
      url: page.url(),
      title: await page.title(),
      nodes,
      node_count: nodes.length,
    };
  },

  // 截图 (PNG → base64)
  async screenshot({ fullPage = false }) {
    const buf = await page.screenshot({ type: 'png', fullPage });
    return {
      image_base64: buf.toString('base64'),
      width: 1280,
      height: 800,
    };
  },

  // 获取页面文本
  async get_text() {
    const text = await page.innerText('body');
    return {
      text: text.slice(0, 5000),
      url: page.url(),
      title: await page.title(),
    };
  },

  // 执行 JS
  async evaluate({ script }) {
    const result = await page.evaluate(script);
    return { result: JSON.stringify(result) };
  },

  // 关闭浏览器
  async close() {
    let savedTrace = null;
    if (context) {
      // 停止录制并保存 trace
      if (recording && tracePath) {
        try {
          await context.tracing.stop({ path: tracePath });
          savedTrace = tracePath;
        } catch (e) {
          // tracing 可能未启动
        }
        recording = false;
      }
      await context.close();
      context = null;
      page = null;
    }
    return { ok: true, trace_path: savedTrace };
  },

  // 获取录制文件路径 (不关闭浏览器)
  async get_recording() {
    if (!recording || !tracePath) {
      return { recording: false };
    }
    // 停止当前 trace, 保存文件
    try {
      await context.tracing.stop({ path: tracePath });
    } catch (e) {
      return { recording: false, error: e.message };
    }
    const saved = tracePath;
    // 重新开始新的 trace (继续录制)
    tracePath = tracePath.replace(/\.zip$/, `-${Date.now()}.zip`);
    await context.tracing.start({ screenshots: true, snapshots: true });
    return { recording: true, trace_path: saved };
  },

  // 悬停 (触发下拉菜单等)
  async hover({ selector, timeout = 5000 }) {
    await page.hover(selector, { timeout });
    await page.waitForTimeout(300);
    return { ok: true };
  },

  // 选择下拉选项
  async select_option({ selector, value, label, timeout = 5000 }) {
    const options = {};
    if (value !== undefined) options.value = value;
    if (label !== undefined) options.label = label;
    await page.selectOption(selector, options, { timeout });
    return { ok: true };
  },

  // 后退
  async go_back() {
    await page.goBack({ timeout: 10000 });
    return { url: page.url(), title: await page.title() };
  },

  // 获取元素属性
  async get_attribute({ selector, attribute, timeout = 5000 }) {
    const el = await page.waitForSelector(selector, { timeout });
    const val = await el.getAttribute(attribute);
    return { value: val };
  },

  // 获取指定选择器的文本
  async get_text_by({ selector, timeout = 5000 }) {
    const el = await page.waitForSelector(selector, { timeout });
    const text = await el.innerText();
    return { text: text.slice(0, 3000) };
  },
};

// ── ARIA 树扁平化 ──────────────

function flattenTree(node, maxNodes, result = [], depth = 0) {
  if (!node || result.length >= maxNodes) return result;

  if (node.name || node.value) {
    const ref = result.length;  // ref 编号 = 数组索引
    result.push({
      ref,
      role: node.role || '',
      name: (node.name || '').slice(0, 80),
      value: (node.value || '').slice(0, 80),
      level: depth,
      focused: node.focused || false,
    });
  }

  if (node.children) {
    for (const child of node.children) {
      if (result.length >= maxNodes) break;
      flattenTree(child, maxNodes, result, depth + 1);
    }
  }

  return result;
}

// ── JSON-RPC 循环 ──────────────

const rl = readline.createInterface({ input: process.stdin });

rl.on('line', async (line) => {
  let id = 0;
  try {
    const msg = JSON.parse(line.trim());
    id = msg.id || 0;
    const handler = handlers[msg.method];
    if (!handler) {
      throw new Error(`未知方法: ${msg.method}`);
    }
    // page 未初始化时, launch/close/get_recording 仍可调用
    const noPageOk = ['launch', 'close', 'get_recording'];
    if (!page && !noPageOk.includes(msg.method)) {
      throw new Error('浏览器未启动, 请先调用 launch');
    }
    const result = await handler(msg.params || {});
    process.stdout.write(JSON.stringify({ id, result }) + '\n');
  } catch (e) {
    process.stdout.write(
      JSON.stringify({ id, error: { message: e.message } }) + '\n'
    );
  }
});

// 优雅退出
process.on('SIGTERM', async () => {
  await handlers.close();
  process.exit(0);
});

process.on('SIGINT', async () => {
  await handlers.close();
  process.exit(0);
});
