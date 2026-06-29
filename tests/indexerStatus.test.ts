import { describe, expect, it } from 'vitest';
import { buildIndexerStatusView, buildSearchSurfaceNotice } from '../src/composables/useIndexerStatus';
import { normalizeIndexStatus } from '../src/composables/useLocalSearch';

describe('buildIndexerStatusView', () => {
  it('maps core indexer states to stable UI feedback', () => {
    expect(buildIndexerStatusView('Init', {
      indexedFiles: 0,
      contentFiles: 0,
      parseErrors: 0,
      contentQueueDepth: 0
    })).toEqual({
      label: 'Init',
      tone: 'muted',
      message: '准备本地索引'
    });
    expect(buildIndexerStatusView('Building', {
      indexedFiles: 10,
      contentFiles: 4,
      parseErrors: 0,
      contentQueueDepth: 2
    })).toEqual({
      label: 'Building',
      tone: 'active',
      message: '正在建立索引，待解析 2'
    });
    expect(buildIndexerStatusView('Watching', {
      indexedFiles: 10,
      contentFiles: 4,
      parseErrors: 0,
      contentQueueDepth: 0
    })).toEqual({
      label: 'Watching',
      tone: 'ready',
      message: '正在监听文件变化'
    });
    expect(buildIndexerStatusView('Suspended', {
      indexedFiles: 10,
      contentFiles: 4,
      parseErrors: 1,
      contentQueueDepth: 3
    })).toEqual({
      label: 'Suspended',
      tone: 'warning',
      message: '资源受限，已暂停内容解析，待解析 3'
    });
  });

  it('maps snake case command status into the frontend progress model', () => {
    expect(normalizeIndexStatus({
      indexed_files: 12,
      content_files: 7,
      parse_errors: 1,
      content_queue_depth: 3
    })).toEqual({
      indexedFiles: 12,
      contentFiles: 7,
      parseErrors: 1,
      contentQueueDepth: 3
    });
  });
});

describe('buildSearchSurfaceNotice', () => {
  it('maps search surface states from configured UI copy', () => {
    expect(buildSearchSurfaceNotice({
      status: 'Init',
      query: '',
      visibleResultCount: 0,
      indexedFiles: 0,
      parseErrors: 0,
      isSearching: false,
      isRebuildingIndex: false,
      runtimeSearchAvailable: true
    })).toEqual({
      code: 'index_empty',
      tone: 'muted',
      title: '尚未建立索引',
      message: '重建索引后即可搜索本机文件。',
      actionLabel: '重建索引'
    });

    expect(buildSearchSurfaceNotice({
      status: 'Building',
      query: '',
      visibleResultCount: 0,
      indexedFiles: 12,
      parseErrors: 0,
      isSearching: true,
      isRebuildingIndex: false,
      runtimeSearchAvailable: true
    })?.code).toBe('loading');

    expect(buildSearchSurfaceNotice({
      status: 'Suspended',
      query: '',
      visibleResultCount: 0,
      indexedFiles: 12,
      parseErrors: 0,
      isSearching: false,
      isRebuildingIndex: false,
      runtimeSearchAvailable: true
    })?.code).toBe('suspended');

    expect(buildSearchSurfaceNotice({
      status: 'Watching',
      query: '',
      visibleResultCount: 0,
      indexedFiles: 12,
      parseErrors: 2,
      isSearching: false,
      isRebuildingIndex: false,
      runtimeSearchAvailable: true
    })?.code).toBe('parse_errors');

    expect(buildSearchSurfaceNotice({
      status: 'Watching',
      query: 'name:missing',
      visibleResultCount: 0,
      indexedFiles: 12,
      parseErrors: 0,
      isSearching: false,
      isRebuildingIndex: false,
      runtimeSearchAvailable: true
    })?.code).toBe('no_results');

    expect(buildSearchSurfaceNotice({
      status: 'Suspended',
      query: 'roadmap',
      visibleResultCount: 0,
      indexedFiles: 12,
      parseErrors: 0,
      isSearching: false,
      isRebuildingIndex: false,
      runtimeSearchAvailable: false
    })).toEqual({
      code: 'runtime_unavailable',
      tone: 'warning',
      title: '搜索服务不可用',
      message: '桌面运行时暂时无法响应，请稍后重试或重建索引。',
      actionLabel: '重建索引'
    });
  });
});
