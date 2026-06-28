<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { calculateVirtualWindow } from '../composables/useVirtualWindow';
import { useSearchStore } from '../stores/searchStore';

const store = useSearchStore();
const {
  emptyMessage,
  indexStatus,
  isRebuildingIndex,
  query,
  parsed,
  quickLookPreview,
  selectedResult,
  statusView,
  visibleResults
} =
  storeToRefs(store);

const listScrollTop = ref(0);
const previewPanel = ref<HTMLElement | null>(null);
const highlightMark = ref<HTMLElement | null>(null);
const contextMenu = ref<{
  id: string;
  x: number;
  y: number;
} | null>(null);
const rowHeight = 68;
const viewportHeight = 340;

const virtualWindow = computed(() =>
  calculateVirtualWindow({
    total: visibleResults.value.length,
    rowHeight,
    viewportHeight,
    scrollTop: listScrollTop.value,
    overscan: 3
  })
);

const renderedResults = computed(() =>
  visibleResults.value.slice(virtualWindow.value.start, virtualWindow.value.end)
);

const statusTone = computed(() => {
  if (statusView.value.tone === 'warning') return 'text-warn';
  if (statusView.value.tone === 'active') return 'text-accent';
  if (statusView.value.tone === 'muted') return 'text-slate-500';
  return 'text-ink';
});

const highlightedPreview = computed(() => {
  if (!quickLookPreview.value?.highlight) {
    return null;
  }

  const { text, highlight } = quickLookPreview.value;
  return {
    before: text.slice(0, highlight.start),
    match: text.slice(highlight.start, highlight.end),
    after: text.slice(highlight.end)
  };
});

function updateScroll(event: Event) {
  listScrollTop.value = (event.currentTarget as HTMLElement).scrollTop;
}

function closeContextMenu() {
  contextMenu.value = null;
}

function openContextMenu(event: MouseEvent, id: string) {
  event.preventDefault();
  store.selectResult(id);
  contextMenu.value = {
    id,
    x: event.clientX,
    y: event.clientY
  };
}

async function openResult(id: string) {
  closeContextMenu();
  await store.openResult(id);
}

async function revealResult(id: string) {
  closeContextMenu();
  await store.revealResult(id);
}

async function copyPath() {
  closeContextMenu();
  await store.copySelectedPath();
}

async function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    closeContextMenu();
    return;
  }

  if (!selectedResult.value) {
    return;
  }

  if (event.key === 'Enter' && event.metaKey) {
    event.preventDefault();
    await store.revealSelected();
  } else if (event.key === 'Enter') {
    event.preventDefault();
    await store.openSelected();
  } else if (event.key.toLowerCase() === 'c' && event.metaKey) {
    event.preventDefault();
    await store.copySelectedPath();
  }
}

watch(
  () => selectedResult.value?.id,
  () => {
    void store.loadSelectedPreview();
  },
  { immediate: true }
);

watch(
  () => quickLookPreview.value?.highlight,
  async () => {
    await nextTick();
    highlightMark.value?.scrollIntoView({ block: 'center' });
    if (!highlightMark.value) {
      previewPanel.value?.scrollTo({ top: 0 });
    }
  },
  { immediate: true }
);

onMounted(() => {
  void store.refreshIndexStatus();
  window.addEventListener('keydown', handleKeydown);
  window.addEventListener('click', closeContextMenu);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', handleKeydown);
  window.removeEventListener('click', closeContextMenu);
});
</script>

<template>
  <main class="min-h-screen bg-mist px-4 py-8 text-ink sm:px-8">
    <section class="mx-auto flex max-w-5xl flex-col overflow-hidden rounded-lg border border-line bg-white shadow-panel">
      <header class="flex items-center justify-between border-b border-line px-5 py-3">
        <div class="flex items-center gap-3">
          <div class="grid size-8 place-items-center rounded-md bg-ink text-sm font-semibold text-white">
            麦
          </div>
          <div>
            <h1 class="text-sm font-semibold leading-5">麦搜</h1>
            <p class="text-xs leading-4 text-slate-500">Local-first search command center</p>
          </div>
        </div>
        <div class="text-right">
          <div class="text-xs font-medium" :class="statusTone">{{ statusView.label }}</div>
          <div class="text-[11px] leading-4 text-slate-500">{{ statusView.message }}</div>
        </div>
      </header>

      <div class="grid min-h-[520px] grid-cols-1 lg:grid-cols-[minmax(0,1fr)_320px]">
        <section class="min-w-0 border-r border-line">
          <div class="border-b border-line px-5 py-4">
            <div class="mb-3 flex flex-wrap items-center justify-between gap-3">
              <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-slate-500">
                <span>文件 {{ indexStatus.indexedFiles }}</span>
                <span>内容 {{ indexStatus.contentFiles }}</span>
                <span v-if="indexStatus.parseErrors > 0" class="text-warn">
                  解析失败 {{ indexStatus.parseErrors }}
                </span>
              </div>
              <button
                class="rounded-md border border-line bg-white px-3 py-1.5 text-xs font-semibold text-ink transition hover:bg-mist disabled:cursor-not-allowed disabled:opacity-60"
                :disabled="isRebuildingIndex"
                @click="store.rebuildIndex()"
              >
                {{ isRebuildingIndex ? '索引中' : '重建索引' }}
              </button>
            </div>
            <input
              :value="query"
              class="h-12 w-full rounded-md border border-line bg-mist px-4 text-[15px] font-medium outline-none transition focus:border-accent focus:bg-white"
              placeholder="name:roadmap ext:md 或 /(phase-3)/"
              autofocus
              @input="store.setQuery(($event.target as HTMLInputElement).value)"
            />
            <div class="mt-3 flex flex-wrap gap-2">
              <span
                v-for="term in parsed.terms"
                :key="`${term.field}:${term.value}`"
                class="rounded border px-2 py-1 text-xs font-medium"
                :class="term.valid ? 'border-line bg-white text-slate-600' : 'border-red-200 bg-red-50 text-warn'"
              >
                {{ term.field }}: {{ term.value }}
              </span>
              <span
                v-for="error in parsed.errors"
                :key="error.code"
                class="rounded border border-red-200 bg-red-50 px-2 py-1 text-xs font-medium text-warn"
              >
                {{ error.message }}
              </span>
            </div>
          </div>

          <div
            class="relative h-[340px] overflow-auto"
            :style="{ height: `${viewportHeight}px` }"
            @scroll="updateScroll"
          >
            <div
              v-if="visibleResults.length === 0"
              class="grid h-full place-items-center px-5 text-sm text-slate-500"
            >
              {{ emptyMessage }}
            </div>
            <div v-else :style="{ height: `${virtualWindow.totalHeight}px` }">
              <div :style="{ transform: `translateY(${virtualWindow.offsetTop}px)` }">
                <button
                  v-for="result in renderedResults"
                  :key="result.id"
                  class="grid w-full grid-cols-[1fr_auto] gap-3 border-b border-line px-5 py-3 text-left transition hover:bg-mist"
                  :class="result.id === selectedResult?.id ? 'bg-blue-50' : 'bg-white'"
                  :style="{ height: `${rowHeight}px` }"
                  @click="store.selectResult(result.id)"
                  @dblclick="openResult(result.id)"
                  @contextmenu="openContextMenu($event, result.id)"
                >
                  <span class="min-w-0">
                    <span class="block truncate text-sm font-semibold">{{ result.name }}</span>
                    <span class="block truncate text-xs text-slate-500">{{ result.path }}</span>
                  </span>
                  <span class="self-start rounded border border-line px-2 py-1 text-[11px] font-semibold uppercase text-slate-500">
                    {{ result.extension }}
                  </span>
                </button>
              </div>
            </div>
          </div>
        </section>

        <aside class="bg-slate-50 px-5 py-4">
          <div v-if="selectedResult" class="space-y-4">
            <div>
              <p class="text-xs font-semibold uppercase text-slate-500">QuickLook</p>
              <h2 class="mt-1 break-words text-base font-semibold">{{ selectedResult.name }}</h2>
              <p class="mt-1 break-words text-xs leading-5 text-slate-500">{{ selectedResult.path }}</p>
            </div>
            <div ref="previewPanel" class="max-h-64 overflow-auto rounded-md border border-line bg-white p-4">
              <p class="whitespace-pre-wrap text-sm leading-6 text-slate-700">
                <span v-if="quickLookPreview?.wasTruncatedBefore">...</span>
                <template v-if="highlightedPreview">
                  {{ highlightedPreview.before }}<mark
                    ref="highlightMark"
                    class="rounded bg-yellow-100 px-0.5 text-ink"
                  >{{ highlightedPreview.match }}</mark>{{ highlightedPreview.after }}
                </template>
                <template v-else>{{ quickLookPreview?.text }}</template>
                <span v-if="quickLookPreview?.wasTruncatedAfter">...</span>
              </p>
            </div>
            <dl class="grid grid-cols-[88px_1fr] gap-y-2 text-xs">
              <dt class="font-medium text-slate-500">Modified</dt>
              <dd>{{ selectedResult.modifiedAt }}</dd>
              <dt class="font-medium text-slate-500">Extension</dt>
              <dd>{{ selectedResult.extension }}</dd>
            </dl>
          </div>
          <div v-else class="grid h-full place-items-center text-sm text-slate-500">
            无匹配结果
          </div>
        </aside>
      </div>
    </section>

    <div
      v-if="contextMenu"
      class="fixed z-50 w-44 overflow-hidden rounded-md border border-line bg-white py-1 text-sm shadow-panel"
      :style="{ left: `${contextMenu.x}px`, top: `${contextMenu.y}px` }"
      @click.stop
    >
      <button
        class="block w-full px-3 py-2 text-left hover:bg-mist"
        @click="openResult(contextMenu.id)"
      >
        打开文件
      </button>
      <button
        class="block w-full px-3 py-2 text-left hover:bg-mist"
        @click="revealResult(contextMenu.id)"
      >
        在 Finder 中显示
      </button>
      <button
        class="block w-full px-3 py-2 text-left hover:bg-mist"
        @click="copyPath()"
      >
        复制路径
      </button>
    </div>
  </main>
</template>
