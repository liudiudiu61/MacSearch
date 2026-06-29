import { defineStore } from 'pinia';
import syntaxConfig from '../../config/search_syntax.json';
import { buildIndexerStatusView, buildSearchSurfaceNotice } from '../composables/useIndexerStatus';
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
import { filterSearchResults, type SearchModeFilter } from '../composables/useSearchFilter';
import { parseSearchQuery } from '../composables/useQueryParser';
import type { IndexerStatus, SearchResult } from '../types/search';

export type SearchSortMode = 'relevance' | 'name' | 'modifiedAt' | 'size' | 'path';

const emptyIndexStatus: SearchIndexStatusView = {
  indexedFiles: 0,
  contentFiles: 0,
  parseErrors: 0,
  contentQueueDepth: 0
};
const searchDebounceMs = typeof syntaxConfig.searchDebounceMs === 'number' ? syntaxConfig.searchDebounceMs : 0;
const fileTypeGroups: Record<string, string[]> = syntaxConfig.fileTypeGroups ?? {};

function waitForSearchDebounce(): Promise<void> {
  if (searchDebounceMs <= 0) {
    return Promise.resolve();
  }

  return new Promise((resolve) => {
    window.setTimeout(resolve, searchDebounceMs);
  });
}

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
    lastSearchStartedAt: 0,
    lastSearchElapsedMs: 0,
    searchMode: 'all' as SearchModeFilter,
    fileTypeGroup: 'all',
    sortMode: 'relevance' as SearchSortMode,
    pendingPreviewPath: null as string | null,
    previewContentByPath: {} as Record<string, string>
  }),
  getters: {
    parsed(state) {
      return parseSearchQuery(state.query, syntaxConfig);
    },
    visibleResults(state) {
      const filtered = filterSearchResults(state.results, parseSearchQuery(state.query, syntaxConfig), {
        searchMode: state.searchMode,
        fileTypeExtensions: resolveFileTypeExtensions(state.fileTypeGroup)
      });

      return sortSearchResults(filtered, state.sortMode);
    },
    selectedResult(state): SearchResult | null {
      return state.results.find((result) => result.id === state.selectedId) ?? null;
    },
    statusView(state) {
      return buildIndexerStatusView(state.status, state.indexStatus);
    },
    emptyMessage(state) {
      return buildSearchSurfaceNotice({
        status: state.status,
        query: state.query,
        visibleResultCount: filterSearchResults(
          state.results,
          parseSearchQuery(state.query, syntaxConfig),
          {
            searchMode: state.searchMode,
            fileTypeExtensions: resolveFileTypeExtensions(state.fileTypeGroup)
          }
        ).length,
        indexedFiles: state.indexStatus.indexedFiles,
        parseErrors: state.indexStatus.parseErrors,
        isSearching: state.isSearching,
        isRebuildingIndex: state.isRebuildingIndex,
        runtimeSearchAvailable: state.runtimeSearchAvailable
      })?.message ?? '';
    },
    surfaceNotice(state) {
      return buildSearchSurfaceNotice({
        status: state.status,
        query: state.query,
        visibleResultCount: filterSearchResults(
          state.results,
          parseSearchQuery(state.query, syntaxConfig),
          {
            searchMode: state.searchMode,
            fileTypeExtensions: resolveFileTypeExtensions(state.fileTypeGroup)
          }
        ).length,
        indexedFiles: state.indexStatus.indexedFiles,
        parseErrors: state.indexStatus.parseErrors,
        isSearching: state.isSearching,
        isRebuildingIndex: state.isRebuildingIndex,
        runtimeSearchAvailable: state.runtimeSearchAvailable
      });
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
      const startedAt = performance.now();
      const parsed = parseSearchQuery(this.query, syntaxConfig);

      try {
        await waitForSearchDebounce();
        if (requestId !== this.searchRequestId) {
          return;
        }
        const runtimeResults = await searchFileNames(parsed, 100, 'filename');
        if (requestId !== this.searchRequestId) {
          return;
        }
        this.results = runtimeResults;
        this.runtimeSearchAvailable = true;
        this.status = 'Watching';
        void this.loadMixedContentResults(parsed, requestId);
      } catch {
        if (requestId !== this.searchRequestId) {
          return;
        }
        this.results = [];
        this.runtimeSearchAvailable = false;
        this.status = 'Suspended';
      } finally {
        if (requestId === this.searchRequestId) {
          this.lastSearchStartedAt = startedAt;
          this.lastSearchElapsedMs = Math.round(performance.now() - startedAt);
          this.isSearching = false;
        }
      }

      this.selectFirstVisibleResult();
    },
    selectResult(id: string) {
      this.selectedId = id;
      void this.loadSelectedPreview();
    },
    setStatus(status: IndexerStatus) {
      this.status = status;
    },
    setSearchMode(mode: SearchModeFilter) {
      this.searchMode = mode;
      this.selectFirstVisibleResult();
    },
    setFileTypeGroup(group: string) {
      this.fileTypeGroup = group;
      this.selectFirstVisibleResult();
    },
    setSortMode(mode: SearchSortMode) {
      this.sortMode = mode;
      this.selectFirstVisibleResult();
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
    },
    async loadMixedContentResults(
      parsed: ReturnType<typeof parseSearchQuery>,
      requestId: number
    ) {
      try {
        const contentResults = await searchFileNames(parsed, 100, 'content');
        if (requestId !== this.searchRequestId) {
          return;
        }
        this.results = mergeSearchResults(this.results, contentResults);
        this.selectFirstVisibleResult();
      } catch {
        if (requestId === this.searchRequestId) {
          this.runtimeSearchAvailable = false;
        }
      }
    },
    scheduleSelectedPreviewLoad(path: string | null) {
      this.pendingPreviewPath = path;

      if (!path) {
        return;
      }

      window.setTimeout(() => {
        if (this.pendingPreviewPath === path && this.selectedResult?.path === path) {
          void this.loadSelectedPreview();
        }
      }, 80);
    },
    selectFirstVisibleResult() {
      const first = this.visibleResults[0];
      this.selectedId = first?.id ?? null;
      this.scheduleSelectedPreviewLoad(first?.path ?? null);
    }
  }
});

function mergeSearchResults(filenameResults: SearchResult[], contentResults: SearchResult[]) {
  const mergedByPath = new Map<string, SearchResult>();

  for (const result of filenameResults) {
    mergedByPath.set(result.path, result);
  }

  for (const result of contentResults) {
    mergedByPath.set(result.path, result);
  }

  return Array.from(mergedByPath.values());
}

function resolveFileTypeExtensions(group: string): string[] {
  if (group === 'all') {
    return [];
  }

  return fileTypeGroups[group] ?? [];
}

function sortSearchResults(results: SearchResult[], mode: SearchSortMode): SearchResult[] {
  return results
    .map((result, index) => ({ result, index }))
    .sort((left, right) => {
      const compared = compareSearchResults(left.result, right.result, mode);

      if (compared !== 0) {
        return compared;
      }

      return left.index - right.index;
    })
    .map((item) => item.result);
}

function compareSearchResults(
  left: SearchResult,
  right: SearchResult,
  mode: SearchSortMode
): number {
  if (mode === 'name') {
    return compareText(left.name, right.name);
  }

  if (mode === 'modifiedAt') {
    return right.modifiedAtUnix - left.modifiedAtUnix;
  }

  if (mode === 'size') {
    return right.sizeBytes - left.sizeBytes;
  }

  if (mode === 'path') {
    return compareText(left.path, right.path);
  }

  return left.score - right.score;
}

function compareText(left: string, right: string): number {
  return left.localeCompare(right, undefined, { sensitivity: 'base' });
}
