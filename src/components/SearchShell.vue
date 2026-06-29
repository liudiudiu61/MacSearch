<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import syntaxConfig from '../../config/search_syntax.json';
import { formatFileSize } from '../composables/useLocalSearch';
import { calculateVirtualWindow } from '../composables/useVirtualWindow';
import { useSearchStore, type SearchSortMode } from '../stores/searchStore';
import IndexSettingsPanel from './IndexSettingsPanel.vue';
import type { SearchSidebarItem } from '../types/search';

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
  surfaceNotice,
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
const viewportHeight = 560;

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

const searchScopeItems = computed<SearchSidebarItem[]>(() => {
  const scopes = [
    {
      id: 'all',
      label: '全部',
      count: store.results.length
    },
    ...syntaxConfig.fieldDirectives
      .filter((directive) => directive.name === 'name' || directive.name === 'content')
      .map((directive) => ({
        id: directive.name === 'name' ? 'filename' : directive.name,
        label: directive.description,
        count: directive.name === 'content' ? indexStatus.value.contentFiles : indexStatus.value.indexedFiles
      }))
  ];

  return scopes.map((scope) => ({
    ...scope,
    tone: store.searchMode === scope.id ? 'active' : 'default' as SearchSidebarItem['tone']
  }));
});

const extensionItems = computed<SearchSidebarItem[]>(() => {
  const groupEntries = Object.entries(syntaxConfig.fileTypeGroups ?? {});

  if (groupEntries.length === 0) {
    return [];
  }

  const items = groupEntries.map(([group, extensions]) => {
    const extensionSet = new Set(extensions.map((extension) => extension.trim().replace(/^\./, '').toLowerCase()));
    const count = store.results.filter((result) =>
      extensionSet.has(result.extension.trim().replace(/^\./, '').toLowerCase())
    ).length;

    return {
      id: group,
      label: fileTypeGroupLabel(group),
      count,
      tone: store.fileTypeGroup === group ? 'active' : 'default' as SearchSidebarItem['tone']
    };
  });

  return [
    {
      id: 'all',
      label: '全部类型',
      count: store.results.length,
      tone: store.fileTypeGroup === 'all' ? 'active' : 'default' as SearchSidebarItem['tone']
    },
    ...items
  ];
});

const sortOptions: Array<{ id: SearchSortMode; label: string }> = [
  { id: 'relevance', label: '相关性' },
  { id: 'name', label: '名称' },
  { id: 'modifiedAt', label: '修改时间' },
  { id: 'size', label: '大小' },
  { id: 'path', label: '路径' }
];

const statusMetrics = computed(() => [
  { label: '结果', value: visibleResults.value.length },
  { label: '耗时', value: `${store.lastSearchElapsedMs} ms` },
  { label: '已索引', value: indexStatus.value.indexedFiles },
  { label: '待解析', value: indexStatus.value.contentQueueDepth }
]);

function updateScroll(event: Event) {
  listScrollTop.value = (event.currentTarget as HTMLElement).scrollTop;
}

function fileTypeGroupLabel(group: string): string {
  const labels: Record<string, string> = {
    documents: '文档',
    tables: '表格',
    presentations: '演示',
    code: '代码',
    images: '图片'
  };

  return labels[group] ?? group;
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

async function copyResultPath(id: string) {
  closeContextMenu();
  store.selectResult(id);
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
  <main class="min-h-screen bg-mist p-3 text-ink sm:p-4">
    <section class="mx-auto flex h-[calc(100vh-2rem)] min-h-[680px] max-w-7xl flex-col overflow-hidden rounded-lg border border-line bg-white shadow-panel">
      <header class="flex min-h-16 flex-wrap items-center gap-3 border-b border-line bg-white px-4 py-3 lg:flex-nowrap">
        <div class="flex min-w-[160px] items-center gap-3">
          <div class="grid size-8 shrink-0 place-items-center rounded-md bg-ink text-sm font-semibold text-white">
            麦
          </div>
          <div class="min-w-0">
            <h1 class="truncate text-sm font-semibold leading-5">麦搜</h1>
            <p class="truncate text-xs leading-4 text-slate-500">Local-first search</p>
          </div>
        </div>

        <div class="min-w-[280px] flex-1">
          <input
            :value="query"
            class="h-10 w-full rounded-md border border-line bg-mist px-3 text-[15px] font-medium outline-none transition focus:border-accent focus:bg-white"
            placeholder="name:roadmap ext:md 或 /(phase-3)/"
            autofocus
            @input="store.setQuery(($event.target as HTMLInputElement).value)"
          />
        </div>

        <div class="flex items-center gap-2">
          <div class="inline-flex h-8 overflow-hidden rounded-md border border-line bg-mist p-0.5 text-xs font-semibold">
            <span class="rounded bg-white px-3 py-1.5 text-ink shadow-sm">本机</span>
            <span class="px-3 py-1.5 text-slate-500">内容</span>
          </div>
          <button
            class="h-8 rounded-md border border-line bg-white px-3 text-xs font-semibold text-ink transition hover:bg-mist disabled:cursor-not-allowed disabled:opacity-60"
            :disabled="isRebuildingIndex"
            @click="store.rebuildIndex()"
          >
            {{ isRebuildingIndex ? '索引中' : '重建索引' }}
          </button>
        </div>

        <div class="ml-auto min-w-[150px] text-right">
          <div class="text-xs font-medium" :class="statusTone">{{ statusView.label }}</div>
          <div class="truncate text-[11px] leading-4 text-slate-500">{{ statusView.message }}</div>
        </div>
      </header>

      <div class="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[220px_minmax(0,1fr)_340px]">
        <aside class="min-h-0 border-b border-line bg-slate-50/80 px-3 py-3 lg:border-b-0 lg:border-r">
          <div class="space-y-5">
            <nav aria-label="搜索范围">
              <p class="px-2 text-[11px] font-semibold uppercase leading-6 text-slate-500">搜索范围</p>
              <div class="space-y-1">
                <button
                  v-for="item in searchScopeItems"
                  :key="item.id"
                  class="flex h-8 w-full items-center justify-between rounded-md px-2 text-left text-xs font-medium transition hover:bg-white"
                  :class="item.tone === 'active' ? 'bg-white text-ink shadow-sm' : 'text-slate-600'"
                  @click="store.setSearchMode(item.id)"
                >
                  <span class="truncate">{{ item.label }}</span>
                  <span v-if="item.count !== null" class="ml-2 text-[11px] text-slate-500">
                    {{ item.count }}
                  </span>
                </button>
              </div>
            </nav>

            <nav aria-label="文件类型">
              <p class="px-2 text-[11px] font-semibold uppercase leading-6 text-slate-500">文件类型</p>
              <div class="space-y-1">
                <div
                  v-if="extensionItems.length === 0"
                  class="px-2 py-1.5 text-xs text-slate-500"
                >
                  等待结果
                </div>
                <button
                  v-for="item in extensionItems"
                  :key="item.id"
                  class="flex h-8 w-full items-center justify-between rounded-md px-2 text-left text-xs font-medium transition hover:bg-white"
                  :class="item.tone === 'active' ? 'bg-white text-ink shadow-sm' : 'text-slate-600'"
                  @click="store.setFileTypeGroup(item.id)"
                >
                  <span class="truncate">{{ item.label }}</span>
                  <span class="ml-2 text-[11px] text-slate-500">{{ item.count }}</span>
                </button>
              </div>
            </nav>

            <div class="rounded-md border border-line bg-white px-3 py-2">
              <div class="flex items-center justify-between text-xs">
                <span class="font-semibold text-slate-600">索引</span>
                <span :class="statusTone">{{ statusView.label }}</span>
              </div>
              <p
                v-if="surfaceNotice?.code === 'runtime_unavailable' || surfaceNotice?.code === 'parse_errors'"
                class="mt-2 text-[11px] leading-4"
                :class="surfaceNotice.tone === 'warning' ? 'text-warn' : 'text-slate-500'"
              >
                {{ surfaceNotice.title }}
              </p>
              <div class="mt-2 grid grid-cols-2 gap-2 text-[11px] text-slate-500">
                <span>文件 {{ indexStatus.indexedFiles }}</span>
                <span>内容 {{ indexStatus.contentFiles }}</span>
                <span>待解析 {{ indexStatus.contentQueueDepth }}</span>
                <span :class="indexStatus.parseErrors > 0 ? 'text-warn' : ''">
                  失败 {{ indexStatus.parseErrors }}
                </span>
              </div>
            </div>

            <IndexSettingsPanel />
          </div>
        </aside>

        <section class="flex min-h-0 min-w-0 flex-col border-b border-line lg:border-b-0 lg:border-r">
          <div class="border-b border-line bg-white px-4 py-3">
            <div class="flex min-h-7 flex-wrap items-center justify-between gap-3">
              <div class="flex min-w-0 flex-1 flex-wrap items-center gap-2">
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
                <span v-if="parsed.terms.length === 0 && parsed.errors.length === 0" class="text-xs text-slate-500">
                  输入查询后显示解析条件
                </span>
              </div>
              <label class="flex h-8 shrink-0 items-center gap-2 text-xs font-semibold text-slate-600">
                <span>排序</span>
                <select
                  :value="store.sortMode"
                  class="h-8 rounded-md border border-line bg-white px-2 text-xs font-semibold text-ink outline-none transition focus:border-accent"
                  @change="store.setSortMode(($event.target as HTMLSelectElement).value as SearchSortMode)"
                >
                  <option v-for="option in sortOptions" :key="option.id" :value="option.id">
                    {{ option.label }}
                  </option>
                </select>
              </label>
            </div>
          </div>

          <div
            class="relative min-h-0 flex-1 overflow-auto bg-white"
            :style="{ height: `${viewportHeight}px` }"
            @scroll="updateScroll"
          >
            <div
              v-if="visibleResults.length === 0"
              class="grid h-full place-items-center px-5"
            >
              <div
                v-if="surfaceNotice"
                class="w-full max-w-sm rounded-md border border-line bg-white px-4 py-3 text-left shadow-sm"
              >
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <p
                      class="truncate text-sm font-semibold"
                      :class="surfaceNotice.tone === 'warning'
                        ? 'text-warn'
                        : surfaceNotice.tone === 'active'
                          ? 'text-accent'
                          : 'text-ink'"
                    >
                      {{ surfaceNotice.title }}
                    </p>
                    <p class="mt-1 text-xs leading-5 text-slate-500">{{ surfaceNotice.message }}</p>
                  </div>
                  <button
                    v-if="surfaceNotice.actionLabel"
                    class="h-7 shrink-0 rounded-md border border-line bg-white px-2.5 text-xs font-semibold text-ink transition hover:bg-mist disabled:cursor-not-allowed disabled:opacity-60"
                    :disabled="isRebuildingIndex"
                    @click="store.rebuildIndex()"
                  >
                    {{ surfaceNotice.actionLabel }}
                  </button>
                </div>
              </div>
              <span v-else class="text-sm text-slate-500">{{ emptyMessage }}</span>
            </div>
            <div v-else :style="{ height: `${virtualWindow.totalHeight}px` }">
              <div :style="{ transform: `translateY(${virtualWindow.offsetTop}px)` }">
                <button
                  v-for="result in renderedResults"
                  :key="result.id"
                  class="grid w-full grid-cols-[minmax(0,1fr)_210px] gap-3 border-b border-line px-4 py-2.5 text-left transition hover:bg-mist"
                  :class="result.id === selectedResult?.id ? 'bg-blue-50' : 'bg-white'"
                  :style="{ height: `${rowHeight}px` }"
                  @click="store.selectResult(result.id)"
                  @dblclick="openResult(result.id)"
                  @contextmenu="openContextMenu($event, result.id)"
                >
                  <span class="min-w-0">
                    <span class="flex min-w-0 items-center gap-2">
                      <span class="truncate text-sm font-semibold">{{ result.name }}</span>
                      <span
                        class="shrink-0 rounded border px-1.5 py-0.5 text-[10px] font-semibold uppercase"
                        :class="result.hitSource === 'content'
                          ? 'border-yellow-200 bg-yellow-50 text-yellow-700'
                          : 'border-line bg-white text-slate-500'"
                      >
                        {{ result.hitSource === 'content' ? 'CONTENT' : 'NAME' }}
                      </span>
                    </span>
                    <span
                      class="block truncate text-xs"
                      :class="result.hitSource === 'content' ? 'text-slate-600' : 'text-slate-500'"
                    >
                      {{ result.hitSource === 'content' ? result.excerpt : result.path }}
                    </span>
                  </span>
                  <span class="grid min-w-0 content-start justify-items-end gap-1 text-[11px] text-slate-500">
                    <span class="flex max-w-full items-center gap-1">
                      <span class="truncate font-semibold text-slate-600">{{ result.kind }}</span>
                      <span class="shrink-0 rounded border border-line bg-white px-1.5 py-0.5 font-semibold uppercase">
                        {{ result.extension }}
                      </span>
                    </span>
                    <span class="max-w-full truncate">{{ result.modifiedAt || '未知时间' }}</span>
                    <span class="flex max-w-full items-center gap-1">
                      <span class="max-w-[64px] truncate">{{ formatFileSize(result.sizeBytes) }}</span>
                      <span class="inline-flex overflow-hidden rounded border border-line bg-white">
                        <button
                          class="h-5 px-1.5 font-semibold text-slate-600 hover:bg-mist"
                          @click.stop="openResult(result.id)"
                        >
                          打开
                        </button>
                        <button
                          class="h-5 border-l border-line px-1.5 font-semibold text-slate-600 hover:bg-mist"
                          @click.stop="revealResult(result.id)"
                        >
                          显示
                        </button>
                        <button
                          class="h-5 border-l border-line px-1.5 font-semibold text-slate-600 hover:bg-mist"
                          @click.stop="copyResultPath(result.id)"
                        >
                          复制
                        </button>
                      </span>
                    </span>
                  </span>
                </button>
              </div>
            </div>
          </div>
        </section>

        <aside class="min-h-0 bg-slate-50 px-4 py-4">
          <div v-if="selectedResult" class="flex h-full min-h-0 flex-col gap-4">
            <div>
              <p class="text-xs font-semibold uppercase text-slate-500">QuickLook</p>
              <h2 class="mt-1 break-words text-base font-semibold">{{ selectedResult.name }}</h2>
              <p class="mt-1 break-words text-xs leading-5 text-slate-500">{{ selectedResult.path }}</p>
            </div>
            <div ref="previewPanel" class="min-h-0 flex-1 overflow-auto rounded-md border border-line bg-white p-4">
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
              <dt class="font-medium text-slate-500">Size</dt>
              <dd>{{ formatFileSize(selectedResult.sizeBytes) }}</dd>
              <dt class="font-medium text-slate-500">Kind</dt>
              <dd>{{ selectedResult.kind }}</dd>
              <dt class="font-medium text-slate-500">Extension</dt>
              <dd>{{ selectedResult.extension }}</dd>
            </dl>
          </div>
          <div v-else class="grid h-full place-items-center text-sm text-slate-500">
            无匹配结果
          </div>
        </aside>
      </div>

      <footer class="flex min-h-9 flex-wrap items-center justify-between gap-3 border-t border-line bg-white px-4 py-2 text-xs text-slate-500">
        <div class="flex flex-wrap gap-x-4 gap-y-1">
          <span v-for="metric in statusMetrics" :key="metric.label">
            {{ metric.label }} {{ metric.value }}
          </span>
        </div>
        <span v-if="indexStatus.parseErrors > 0" class="text-warn">
          解析失败 {{ indexStatus.parseErrors }}
        </span>
      </footer>
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
