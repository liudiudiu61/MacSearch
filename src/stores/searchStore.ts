import { defineStore } from 'pinia';
import syntaxConfig from '../../config/search_syntax.json';
import { buildIndexerStatusView } from '../composables/useIndexerStatus';
import {
  getSearchIndexStatus,
  openSearchResult,
  rebuildSearchIndex,
  revealSearchResult,
  searchFileNames,
  type SearchIndexStatusView
} from '../composables/useLocalSearch';
import { readPreviewContent } from '../composables/usePreviewContent';
import { buildQuickLookPreview } from '../composables/useQuickLookPreview';
import { filterSearchResults } from '../composables/useSearchFilter';
import { parseSearchQuery } from '../composables/useQueryParser';
import type { IndexerStatus, SearchResult } from '../types/search';

const emptyIndexStatus: SearchIndexStatusView = {
  indexedFiles: 0,
  contentFiles: 0,
  parseErrors: 0
};

export const useSearchStore = defineStore('search', {
  state: () => ({
    query: '',
    selectedId: null as string | null,
    status: 'Init' as IndexerStatus,
    results: [] as SearchResult[],
    isSearching: false,
    isRebuildingIndex: false,
    runtimeSearchAvailable: true,
    indexStatus: emptyIndexStatus,
    searchRequestId: 0,
    previewRequestId: 0,
    previewContentByPath: {} as Record<string, string>
  }),
  getters: {
    parsed(state) {
      return parseSearchQuery(state.query, syntaxConfig);
    },
    visibleResults(state) {
      return filterSearchResults(state.results, parseSearchQuery(state.query, syntaxConfig));
    },
    selectedResult(state): SearchResult | null {
      return state.results.find((result) => result.id === state.selectedId) ?? null;
    },
    statusView(state) {
      return buildIndexerStatusView(state.status);
    },
    emptyMessage(state) {
      if (state.isRebuildingIndex) {
        return '正在建立本机文件索引';
      }
      if (state.indexStatus.indexedFiles === 0) {
        return '还没有索引文件，请先重建索引';
      }
      if (state.query.trim()) {
        return '没有找到匹配文件';
      }
      return '输入文件名或内容开始搜索';
    },
    quickLookPreview(state) {
      const selected = state.results.find((result) => result.id === state.selectedId);

      if (!selected) {
        return null;
      }

      const parsed = parseSearchQuery(state.query, syntaxConfig);
      const matchText =
        parsed.terms.find((term) => term.valid && term.field === 'content')?.value ??
        parsed.terms.find((term) => term.valid && term.field === 'name')?.value ??
        parsed.terms.find((term) => term.valid && term.field === 'text')?.value ??
        selected.name;
      const content = state.previewContentByPath[selected.path] ?? selected.excerpt;

      return buildQuickLookPreview(content, matchText, 500);
    }
  },
  actions: {
    async setQuery(query: string) {
      this.query = query;
      const requestId = this.searchRequestId + 1;
      this.searchRequestId = requestId;
      this.isSearching = true;
      this.status = 'Building';
      const parsed = parseSearchQuery(this.query, syntaxConfig);

      try {
        const runtimeResults = await searchFileNames(parsed, 100);
        if (requestId !== this.searchRequestId) {
          return;
        }
        this.results = runtimeResults;
        this.runtimeSearchAvailable = true;
        this.status = 'Watching';
      } catch {
        if (requestId !== this.searchRequestId) {
          return;
        }
        this.results = [];
        this.runtimeSearchAvailable = false;
        this.status = 'Suspended';
      } finally {
        if (requestId === this.searchRequestId) {
          this.isSearching = false;
        }
      }

      const first = this.visibleResults[0];
      this.selectedId = first?.id ?? null;
      await this.loadSelectedPreview();
    },
    selectResult(id: string) {
      this.selectedId = id;
      void this.loadSelectedPreview();
    },
    setStatus(status: IndexerStatus) {
      this.status = status;
    },
    async openResult(id: string) {
      const result = this.results.find((item) => item.id === id);
      if (result) {
        await openSearchResult(result.path);
      }
    },
    async revealResult(id: string) {
      const result = this.results.find((item) => item.id === id);
      if (result) {
        await revealSearchResult(result.path);
      }
    },
    async openSelected() {
      if (this.selectedId) {
        await this.openResult(this.selectedId);
      }
    },
    async revealSelected() {
      if (this.selectedId) {
        await this.revealResult(this.selectedId);
      }
    },
    async copySelectedPath() {
      const selected = this.selectedResult;
      if (selected) {
        await navigator.clipboard.writeText(selected.path);
      }
    },
    async refreshIndexStatus() {
      try {
        this.indexStatus = await getSearchIndexStatus();
        this.status = this.indexStatus.indexedFiles > 0 ? 'Watching' : 'Init';
      } catch {
        this.runtimeSearchAvailable = false;
        this.status = 'Suspended';
      }
    },
    async rebuildIndex() {
      this.isRebuildingIndex = true;
      this.status = 'Building';

      try {
        this.indexStatus = await rebuildSearchIndex();
        this.runtimeSearchAvailable = true;
        this.status = 'Watching';
        await this.setQuery(this.query);
      } catch {
        this.runtimeSearchAvailable = false;
        this.status = 'Suspended';
      } finally {
        this.isRebuildingIndex = false;
      }
    },
    async loadSelectedPreview() {
      const selected = this.selectedResult;

      if (!selected || this.previewContentByPath[selected.path]) {
        return;
      }

      const requestId = this.previewRequestId + 1;
      this.previewRequestId = requestId;

      try {
        const preview = await readPreviewContent(selected.path);
        if (requestId !== this.previewRequestId) {
          return;
        }
        this.previewContentByPath[selected.path] = preview.content;
      } catch {
        if (requestId !== this.previewRequestId) {
          return;
        }
        this.previewContentByPath[selected.path] = selected.excerpt;
      }
    }
  }
});
