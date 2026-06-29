<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import {
  formatFileSize,
  getIndexerPolicySettings,
  type IndexerPolicySettings
} from '../composables/useLocalSearch';

const settings = ref<IndexerPolicySettings | null>(null);
const isLoading = ref(false);
const loadError = ref<string | null>(null);

const contentIndexRows = computed(() => {
  if (!settings.value) {
    return [];
  }

  return [
    { label: '状态', value: settings.value.content_index.enabled ? '启用' : '关闭' },
    { label: '批量', value: settings.value.content_index.batch_size },
    { label: '片段半径', value: settings.value.content_index.snippet_radius },
    { label: '默认上限', value: settings.value.content_index.default_limit }
  ];
});

async function loadSettings() {
  isLoading.value = true;
  loadError.value = null;

  try {
    settings.value = await getIndexerPolicySettings();
  } catch {
    loadError.value = '无法读取索引策略';
  } finally {
    isLoading.value = false;
  }
}

onMounted(() => {
  void loadSettings();
});
</script>

<template>
  <section class="rounded-md border border-line bg-white px-3 py-2">
    <div class="flex items-center justify-between gap-2 text-xs">
      <span class="font-semibold text-slate-600">索引设置</span>
      <button
        class="h-6 rounded border border-line bg-white px-2 text-[11px] font-semibold text-slate-600 transition hover:bg-mist disabled:cursor-not-allowed disabled:opacity-60"
        :disabled="isLoading"
        @click="loadSettings()"
      >
        {{ isLoading ? '读取中' : '刷新' }}
      </button>
    </div>

    <div v-if="loadError" class="mt-2 text-[11px] leading-4 text-warn">
      {{ loadError }}
    </div>
    <div v-else-if="!settings" class="mt-2 text-[11px] leading-4 text-slate-500">
      读取索引策略
    </div>
    <div v-else class="mt-3 space-y-3 text-[11px] leading-4 text-slate-600">
      <div>
        <p class="font-semibold text-slate-500">扫描根目录</p>
        <ul class="mt-1 space-y-1">
          <li v-for="root in settings.scan_roots" :key="root" class="truncate" :title="root">
            {{ root }}
          </li>
        </ul>
      </div>

      <div>
        <p class="font-semibold text-slate-500">监听根目录</p>
        <ul class="mt-1 space-y-1">
          <li v-for="root in settings.watch_roots" :key="root" class="truncate" :title="root">
            {{ root }}
          </li>
        </ul>
      </div>

      <div>
        <p class="font-semibold text-slate-500">排除路径</p>
        <div class="mt-1 flex flex-wrap gap-1">
          <span
            v-for="fragment in settings.exclude_path_fragments"
            :key="fragment"
            class="max-w-full truncate rounded border border-line bg-mist px-1.5 py-0.5"
            :title="fragment"
          >
            {{ fragment }}
          </span>
        </div>
      </div>

      <div>
        <p class="font-semibold text-slate-500">文本扩展名</p>
        <div class="mt-1 flex flex-wrap gap-1">
          <span
            v-for="extension in settings.text_extensions"
            :key="extension"
            class="rounded border border-line bg-mist px-1.5 py-0.5 font-semibold uppercase"
          >
            {{ extension }}
          </span>
        </div>
      </div>

      <dl class="grid grid-cols-[74px_1fr] gap-y-1">
        <dt class="font-semibold text-slate-500">解析上限</dt>
        <dd>{{ formatFileSize(settings.max_parse_size_bytes) }}</dd>
        <template v-for="row in contentIndexRows" :key="row.label">
          <dt class="font-semibold text-slate-500">{{ row.label }}</dt>
          <dd>{{ row.value }}</dd>
        </template>
      </dl>
    </div>
  </section>
</template>
