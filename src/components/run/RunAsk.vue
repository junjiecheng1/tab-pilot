<template>
  <div class="run-ask" :class="{ answered }">
    <div class="ask-header">
      <MessageCircleQuestion :size="13" />
      <span>{{ answered ? '已发起用户交互' : '需要你的回应' }}</span>
    </div>

    <div class="ask-question">{{ questionText }}</div>

    <!-- 已回答：只显示摘要 -->
    <div v-if="answered" class="ask-answer-preview">
      <span class="preview-label">你的回答：</span>
      <span class="preview-text">{{ answerPreview }}</span>
    </div>

    <!-- 选项式（choice / confirm） -->
    <div v-else-if="hasOptions" class="ask-options">
      <button
        v-for="(opt, i) in options"
        :key="`${i}-${optValue(opt)}`"
        class="opt"
        :class="{ selected: isSelected(opt), recommended: opt.recommended }"
        @click="toggle(opt)"
      >
        <span class="opt-idx">{{ i + 1 }}</span>
        <span class="opt-body">
          <span class="opt-label">{{ opt.label }}</span>
          <span v-if="opt.description" class="opt-desc">{{ opt.description }}</span>
        </span>
        <span v-if="isSelected(opt)" class="opt-check">✓</span>
      </button>
    </div>

    <!-- 自由输入 -->
    <div v-if="!answered && showInput" class="ask-input-wrap">
      <input
        v-model="textInput"
        type="text"
        class="ask-input"
        :placeholder="hasOptions ? '补充说明（可选）' : '输入回答…'"
        @keydown.enter.exact="onAskEnter"
      />
    </div>

    <!-- 提交 -->
    <div v-if="!answered" class="ask-actions">
      <button
        class="submit-btn"
        :disabled="!canSubmit || submitting"
        @click="onSubmit"
      >
        {{ submitting ? '提交中…' : '确认' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { MessageCircleQuestion } from 'lucide-vue-next';
import { useChatStore } from '../../stores/chatStore';

interface AskOption {
  label: string;
  value?: string;
  id?: string;
  description?: string;
  recommended?: boolean;
}

const props = defineProps<{ payload: Record<string, unknown> }>();

const chat = useChatStore();
const submitting = ref(false);

const toolCallId = computed(() => String(props.payload.tool_call_id ?? ''));

const questionText = computed(() => {
  const q = props.payload.question;
  if (typeof q === 'string' && q) return q;
  const questions = props.payload.questions as Array<{ question?: string }> | undefined;
  return questions?.[0]?.question || '需要你的回应';
});

const questionType = computed(() => {
  const t = props.payload.question_type;
  if (typeof t === 'string') return t;
  const questions = props.payload.questions as Array<{ question_type?: string }> | undefined;
  return questions?.[0]?.question_type || 'open';
});

const options = computed<AskOption[]>(() => {
  const opts = props.payload.options as unknown;
  if (Array.isArray(opts) && opts.length) {
    return opts as AskOption[];
  }
  const questions = props.payload.questions as Array<{ options?: unknown }> | undefined;
  const qOpts = questions?.[0]?.options;
  if (Array.isArray(qOpts) && qOpts.length) {
    // options 可能是字符串数组（staged_confirmation 的默认）
    if (typeof qOpts[0] === 'string') {
      return (qOpts as string[]).map((s) => ({ label: s, value: s }));
    }
    return qOpts as AskOption[];
  }
  return [];
});

const hasOptions = computed(() => options.value.length > 0);
const isMulti = computed(() => questionType.value === 'multi_select');
const showInput = computed(() => !isMulti.value);

const answered = computed(() => Boolean(props.payload.answered));
const answerPreview = computed(() => String(props.payload.answer_preview ?? ''));

// 选项选择状态
const chosen = ref<string[]>([]);
const textInput = ref('');

function optValue(o: AskOption): string {
  return String(o.value ?? o.id ?? o.label);
}

function isSelected(o: AskOption): boolean {
  return chosen.value.includes(optValue(o));
}

function toggle(o: AskOption) {
  const v = optValue(o);
  if (isMulti.value) {
    const idx = chosen.value.indexOf(v);
    if (idx >= 0) chosen.value.splice(idx, 1);
    else chosen.value.push(v);
  } else {
    chosen.value = [v];
  }
}

const canSubmit = computed(() => {
  if (hasOptions.value && chosen.value.length > 0) return true;
  if (textInput.value.trim().length > 0) return true;
  return false;
});

// IME 合成期忽略 Enter, 避免输入中文/韩文确认候选时误提交
function onAskEnter(e: KeyboardEvent) {
  if (e.isComposing || (e as any).keyCode === 229) return;
  onSubmit();
}

async function onSubmit() {
  if (!canSubmit.value || submitting.value) return;
  const tcid = toolCallId.value;
  if (!tcid) return;

  // 构造 answers 数组（单问题）
  const custom = textInput.value.trim();
  // 从 payload 取 question_id: 优先 payload.question_id, 其次 payload.questions[0].id
  const qid = (() => {
    const direct = props.payload.question_id;
    if (typeof direct === 'string' && direct) return direct;
    const qs = props.payload.questions as Array<{ id?: string; question_id?: string }> | undefined;
    const first = qs?.[0];
    return String(first?.id || first?.question_id || 'q0');
  })();
  const ans: {
    question_id: string;
    answer_value?: string;
    answer_values?: string[];
  } = { question_id: qid };

  if (isMulti.value && chosen.value.length) {
    ans.answer_values = [...chosen.value];
  } else if (chosen.value.length) {
    ans.answer_value = chosen.value[0];
  }
  if (custom) {
    // 有选项 + 附加输入 → 塞到 answer_value；纯 open → answer_value
    if (!ans.answer_value && !ans.answer_values) {
      ans.answer_value = custom;
    } else if (ans.answer_value) {
      ans.answer_value = `${ans.answer_value}（备注：${custom}）`;
    }
  }

  // 预览文本（用于显示 "你的回答：…"）
  const preview = (() => {
    if (ans.answer_values) {
      return chosen.value
        .map((v) => options.value.find((o) => optValue(o) === v)?.label || v)
        .join('、');
    }
    if (ans.answer_value) {
      const opt = options.value.find((o) => optValue(o) === ans.answer_value);
      return opt?.label || ans.answer_value;
    }
    return custom;
  })();

  submitting.value = true;
  try {
    await chat.answerAsk(tcid, [ans], preview);
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
/* 降噪版：中性灰框 + 细边 warning 色条，对齐 PC 的克制用色 */
.run-ask {
  border: 1px solid var(--c-border);
  border-left: 3px solid var(--c-warning);
  background: var(--c-bg-secondary);
  border-radius: var(--radius-md);
  padding: 10px 12px;
  font-size: var(--text-sm);
}

.run-ask.answered {
  border-left-color: var(--c-border-strong);
}

.ask-header {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  color: var(--c-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  margin-bottom: 6px;
}

.ask-question {
  font-weight: var(--weight-medium);
  color: var(--c-text-primary);
  line-height: var(--leading-normal);
  white-space: pre-wrap;
  word-break: break-word;
  margin-bottom: var(--space-2);
}

.ask-answer-preview {
  padding: 6px 10px;
  background: var(--c-bg-primary);
  border: 1px solid var(--c-border);
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  color: var(--c-text-primary);
}
.preview-label { color: var(--c-text-tertiary); margin-right: 6px; }
.preview-text { font-weight: var(--weight-medium); }

.ask-options {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: var(--space-2);
}

.opt {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 10px;
  background: var(--c-bg-elevated);
  border: 1px solid var(--c-border);
  border-radius: var(--radius-md);
  cursor: pointer;
  font-size: var(--text-sm);
  text-align: left;
  transition: all var(--duration-fast) var(--easing);
  color: var(--c-text-primary);
}
.opt:hover { border-color: var(--c-border-strong); background: var(--c-bg-hover); }
.opt.selected { border-color: var(--c-accent); background: var(--c-accent-subtle); }
.opt.recommended { border-color: var(--brand-blue); }

.opt-idx {
  font-size: var(--text-xs);
  color: var(--c-text-tertiary);
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
  margin-top: 1px;
}
.opt-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.opt-label { font-weight: var(--weight-medium); word-break: break-word; }
.opt-desc { font-size: 11.5px; color: var(--c-text-secondary); line-height: 1.4; }
.opt-check { color: var(--c-accent); font-weight: var(--weight-semibold); margin-top: 1px; }

.ask-input-wrap { margin-bottom: var(--space-2); }
.ask-input {
  width: 100%;
  padding: 6px 10px;
  border: 1px solid var(--c-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  color: var(--c-text-primary);
  background: var(--c-bg-elevated);
  outline: none;
  transition: border-color var(--duration-fast) var(--easing);
}
.ask-input:focus { border-color: var(--c-accent); }

.ask-actions {
  display: flex;
  justify-content: flex-end;
}
.submit-btn {
  padding: 5px 14px;
  background: var(--c-accent);
  color: var(--c-text-inverse);
  border: none;
  border-radius: var(--radius-sm);
  font-size: 12.5px;
  font-weight: var(--weight-medium);
  cursor: pointer;
  transition: all var(--duration-fast) var(--easing);
}
.submit-btn:hover:not(:disabled) { background: var(--c-accent-hover); }
.submit-btn:disabled {
  background: var(--c-bg-hover);
  color: var(--c-text-tertiary);
  cursor: not-allowed;
}
</style>
