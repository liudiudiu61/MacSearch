import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import * as localSearch from '../src/composables/useLocalSearch';
import * as previewContent from '../src/composables/usePreviewContent';
import {
  buildSearchFileNamesRequest,
  mapSearchFileNameHits,
  normalizeIndexStatus
} from '../src/composables/useLocalSearch';
import { parseSearchQuery } from '../src/composables/useQueryParser';
import { useSearchStore } from '../src/stores/searchStore';
import type { SearchResult } from '../src/types/search';
import syntaxConfig from '../config/search_syntax.json';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((innerResolve, innerReject) => {
    resolve = innerResolve;
    reject = innerReject;
  });

  return { promise, resolve, reject };
}

function searchResultFixture(overrides: Partial<SearchResult> = {}): SearchResult {
  return {
    id: '/work/default.md',
    name: 'default.md',
    path: '/work/default.md',
    extension: 'md',
    modifiedAt: '2026-06-29 12:00',
    modifiedAtUnix: 1782715200,
    sizeBytes: 1024,
    kind: '文档',
    excerpt: '/work/default.md',
    hitSource: 'filename',
    score: 0,
    snippet: null,
    ...overrides
  };
}

beforeEach(() => {
  setActivePinia(createPinia());
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('buildSearchFileNamesRequest', () => {
  it('maps parsed query terms to the Tauri filename search command payload', () => {
    const parsed = parseSearchQuery('name:roadmap ext:md content:"Task 5"', syntaxConfig);

    expect(buildSearchFileNamesRequest(parsed, 25)).toEqual({
      query: {
        name: 'roadmap',
        content: 'Task 5',
        extension: 'md',
        text: null
      },
      limit: 25,
      mode: 'mixed'
    });
  });

  it('normalizes Tauri index status into frontend casing', () => {
    expect(
      normalizeIndexStatus({
        indexed_files: 12,
        content_files: 5,
        parse_errors: 1,
        content_queue_depth: 2
      })
    ).toEqual({
      indexedFiles: 12,
      contentFiles: 5,
      parseErrors: 1,
      contentQueueDepth: 2
    });
  });

  it('maps content hit metadata and uses snippet as the visible excerpt', () => {
    expect(
      mapSearchFileNameHits([
        {
          id: '/work/notes.md',
          name: 'notes.md',
          path: '/work/notes.md',
          extension: 'md',
          modified_at: 1,
          size_bytes: 4096,
          kind: '文档',
          hit_source: 'content',
          score: -1.25,
          snippet: '...AnyTXT content...'
        },
        {
          id: '/work/roadmap.md',
          name: 'roadmap.md',
          path: '/work/roadmap.md',
          extension: 'md',
          modified_at: 2,
          size_bytes: 512,
          kind: '文档',
          hit_source: 'filename',
          score: 0,
          snippet: null
        }
      ])
    ).toEqual([
      {
        id: '/work/notes.md',
        name: 'notes.md',
        path: '/work/notes.md',
        extension: 'md',
        modifiedAt: '1970-01-01 00:00',
        modifiedAtUnix: 1,
        sizeBytes: 4096,
        kind: '文档',
        excerpt: '...AnyTXT content...',
        hitSource: 'content',
        score: -1.25,
        snippet: '...AnyTXT content...'
      },
      {
        id: '/work/roadmap.md',
        name: 'roadmap.md',
        path: '/work/roadmap.md',
        extension: 'md',
        modifiedAt: '1970-01-01 00:00',
        modifiedAtUnix: 2,
        sizeBytes: 512,
        kind: '文档',
        excerpt: '/work/roadmap.md',
        hitSource: 'filename',
        score: 0,
        snippet: null
      }
    ]);
  });
});

describe('useSearchStore search interactions', () => {
  it('keeps only the latest query results when searches resolve out of order', async () => {
    vi.useFakeTimers();
    vi.spyOn(previewContent, 'readPreviewContent').mockResolvedValue({
      content: 'preview',
      source: 'file'
    });
    const first = deferred<SearchResult[]>();
    const second = deferred<SearchResult[]>();
    vi.spyOn(localSearch, 'searchFileNames').mockImplementation((parsed) => {
      const query = parsed.terms[0]?.value;
      return query === 'second' ? second.promise : first.promise;
    });

    const store = useSearchStore();
    const firstSet = store.setQuery('name:first');
    const secondSet = store.setQuery('name:second');

    await vi.advanceTimersByTimeAsync(80);

    second.resolve([
      searchResultFixture({ id: 'second', name: 'second.md', path: '/work/second.md' })
    ]);
    first.resolve([
      searchResultFixture({ id: 'first', name: 'first.md', path: '/work/first.md' })
    ]);

    await Promise.all([firstSet, secondSet]);

    expect(store.results.map((item) => item.id)).toEqual(['second']);
  });

  it('does not wait for preview content before resolving a query update', async () => {
    vi.useFakeTimers();
    const preview = deferred<{ content: string; source: string }>();
    vi.spyOn(previewContent, 'readPreviewContent').mockReturnValue(preview.promise);
    vi.spyOn(localSearch, 'searchFileNames').mockResolvedValue([
      searchResultFixture({ id: 'roadmap', name: 'roadmap.md', path: '/work/roadmap.md' })
    ]);

    const store = useSearchStore();
    let queryResolved = false;
    const queryUpdate = store.setQuery('roadmap').then(() => {
      queryResolved = true;
    });

    await vi.advanceTimersByTimeAsync(80);
    await queryUpdate;

    expect(store.results.map((item) => item.id)).toEqual(['roadmap']);
    expect(queryResolved).toBe(true);
    expect(store.lastSearchStartedAt).toBeGreaterThanOrEqual(0);
    expect(store.lastSearchElapsedMs).toBeGreaterThanOrEqual(0);
    expect(previewContent.readPreviewContent).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(80);
    expect(previewContent.readPreviewContent).toHaveBeenCalledWith('/work/roadmap.md');

    preview.resolve({ content: 'roadmap preview', source: 'file' });
    await Promise.resolve();
    vi.useRealTimers();
  });

  it('updates query immediately before debounced runtime search resolves', async () => {
    vi.useFakeTimers();
    vi.spyOn(previewContent, 'readPreviewContent').mockResolvedValue({
      content: 'preview',
      source: 'file'
    });
    vi.spyOn(localSearch, 'searchFileNames').mockResolvedValue([]);

    const store = useSearchStore();
    const pending = store.setQuery('roadmap');

    expect(store.query).toBe('roadmap');
    expect(localSearch.searchFileNames).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(79);
    expect(localSearch.searchFileNames).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    await pending;

    expect(localSearch.searchFileNames).toHaveBeenCalledWith(expect.anything(), 100, 'filename');
  });

  it('cancels a debounced search when a newer query arrives before the delay expires', async () => {
    vi.useFakeTimers();
    vi.spyOn(previewContent, 'readPreviewContent').mockResolvedValue({
      content: 'preview',
      source: 'file'
    });
    vi.spyOn(localSearch, 'searchFileNames').mockResolvedValue([]);

    const store = useSearchStore();
    const first = store.setQuery('first');

    await vi.advanceTimersByTimeAsync(40);
    const second = store.setQuery('second');

    await vi.advanceTimersByTimeAsync(40);
    await first;

    expect(localSearch.searchFileNames).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(40);
    await second;

    expect(localSearch.searchFileNames).toHaveBeenCalledWith(
      expect.objectContaining({
        terms: [expect.objectContaining({ field: 'text', value: 'second' })]
      }),
      100,
      'filename'
    );
  });

  it('exposes configured surface notice when runtime search is unavailable', async () => {
    vi.useFakeTimers();
    vi.spyOn(localSearch, 'searchFileNames').mockRejectedValue(new Error('runtime unavailable'));

    const store = useSearchStore();
    const pending = store.setQuery('roadmap');

    await vi.advanceTimersByTimeAsync(80);
    await pending;

    expect(store.surfaceNotice).toEqual({
      code: 'runtime_unavailable',
      tone: 'warning',
      title: '搜索服务不可用',
      message: '桌面运行时暂时无法响应，请稍后重试或重建索引。',
      actionLabel: '重建索引'
    });
  });

  it('renders filename results before merging mixed content results', async () => {
    vi.useFakeTimers();
    vi.spyOn(previewContent, 'readPreviewContent').mockResolvedValue({
      content: 'preview',
      source: 'file'
    });
    const filename = deferred<SearchResult[]>();
    const content = deferred<SearchResult[]>();
    vi.spyOn(localSearch, 'searchFileNames')
      .mockReturnValueOnce(filename.promise)
      .mockReturnValueOnce(content.promise);

    const store = useSearchStore();
    const pending = store.setQuery('roadmap');
    await vi.advanceTimersByTimeAsync(80);

    filename.resolve([
      searchResultFixture({
        id: 'roadmap',
        name: 'roadmap.md',
        path: '/work/roadmap.md',
        hitSource: 'filename',
        excerpt: '/work/roadmap.md'
      })
    ]);
    await pending;

    expect(store.results).toEqual([
      expect.objectContaining({
        id: 'roadmap',
        hitSource: 'filename',
        excerpt: '/work/roadmap.md'
      })
    ]);
    expect(localSearch.searchFileNames).toHaveBeenNthCalledWith(1, expect.anything(), 100, 'filename');
    expect(localSearch.searchFileNames).toHaveBeenNthCalledWith(2, expect.anything(), 100, 'content');

    content.resolve([
      searchResultFixture({
        id: 'roadmap',
        name: 'roadmap.md',
        path: '/work/roadmap.md',
        hitSource: 'content',
        excerpt: '...roadmap content...',
        snippet: '...roadmap content...'
      }),
      searchResultFixture({
        id: 'notes',
        name: 'notes.md',
        path: '/work/notes.md',
        hitSource: 'content',
        excerpt: '...notes content...',
        snippet: '...notes content...'
      })
    ]);
    await Promise.resolve();

    expect(store.results).toEqual([
      expect.objectContaining({
        id: 'roadmap',
        hitSource: 'content',
        excerpt: '...roadmap content...',
        snippet: '...roadmap content...'
      }),
      expect.objectContaining({
        id: 'notes',
        hitSource: 'content',
        excerpt: '...notes content...'
      })
    ]);
  });

  it('applies search mode and configured file type filters without rewriting the query', async () => {
    vi.spyOn(previewContent, 'readPreviewContent').mockResolvedValue({
      content: 'preview',
      source: 'file'
    });
    vi.spyOn(localSearch, 'searchFileNames').mockResolvedValue([
      searchResultFixture({
        id: 'plan',
        name: 'plan.md',
        path: '/work/plan.md',
        extension: 'md',
        hitSource: 'content',
        excerpt: 'roadmap phase notes'
      }),
      searchResultFixture({
        id: 'table',
        name: 'roadmap.csv',
        path: '/work/roadmap.csv',
        extension: 'csv',
        kind: '表格',
        hitSource: 'filename',
        excerpt: '/work/roadmap.csv'
      }),
      searchResultFixture({
        id: 'script',
        name: 'roadmap.ts',
        path: '/work/roadmap.ts',
        extension: 'ts',
        kind: '代码',
        hitSource: 'content',
        excerpt: 'roadmap renderer'
      })
    ]);

    const store = useSearchStore();
    await store.setQuery('roadmap');
    store.setSearchMode('content');
    store.setFileTypeGroup('documents');

    expect(store.query).toBe('roadmap');
    expect(store.visibleResults.map((item) => item.id)).toEqual(['plan']);
  });

  it('sorts visible results by relevance, name, modified time, size, and path', () => {
    const store = useSearchStore();
    store.results = [
      searchResultFixture({
        id: 'beta',
        name: 'beta.md',
        path: '/work/z/beta.md',
        modifiedAtUnix: 30,
        sizeBytes: 300,
        score: -1
      }),
      searchResultFixture({
        id: 'alpha',
        name: 'alpha.md',
        path: '/work/a/alpha.md',
        modifiedAtUnix: 10,
        sizeBytes: 100,
        score: -5
      }),
      searchResultFixture({
        id: 'gamma',
        name: 'gamma.md',
        path: '/work/m/gamma.md',
        modifiedAtUnix: 20,
        sizeBytes: 200,
        score: -5
      })
    ];

    expect(store.sortMode).toBe('relevance');
    expect(store.visibleResults.map((item) => item.id)).toEqual(['alpha', 'gamma', 'beta']);

    store.setSortMode('name');
    expect(store.visibleResults.map((item) => item.id)).toEqual(['alpha', 'beta', 'gamma']);

    store.setSortMode('modifiedAt');
    expect(store.visibleResults.map((item) => item.id)).toEqual(['beta', 'gamma', 'alpha']);

    store.setSortMode('size');
    expect(store.visibleResults.map((item) => item.id)).toEqual(['beta', 'gamma', 'alpha']);

    store.setSortMode('path');
    expect(store.visibleResults.map((item) => item.id)).toEqual(['alpha', 'gamma', 'beta']);
  });

  it('keeps the copy action wired to the selected result path', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText }
    });
    const store = useSearchStore();
    store.results = [
      searchResultFixture({
        id: 'selected',
        path: '/work/selected.md'
      })
    ];
    store.selectResult('selected');

    await store.copySelectedPath();

    expect(writeText).toHaveBeenCalledWith('/work/selected.md');
  });
});
