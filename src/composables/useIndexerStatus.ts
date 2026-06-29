import syntaxConfig from '../../config/search_syntax.json';
import type { IndexerStatus } from '../types/search';
import type { SearchIndexStatusView } from './useLocalSearch';

export type IndexerStatusTone = 'muted' | 'active' | 'ready' | 'warning';
export type SearchSurfaceNoticeCode =
  | 'index_empty'
  | 'loading'
  | 'suspended'
  | 'parse_errors'
  | 'no_results'
  | 'runtime_unavailable';

export type IndexerStatusView = {
  label: IndexerStatus;
  tone: IndexerStatusTone;
  message: string;
};

export type SearchSurfaceNotice = {
  code: SearchSurfaceNoticeCode;
  tone: Extract<IndexerStatusTone, 'muted' | 'active' | 'warning'>;
  title: string;
  message: string;
  actionLabel: string | null;
};

export type SearchSurfaceNoticeInput = {
  status: IndexerStatus;
  query: string;
  visibleResultCount: number;
  indexedFiles: number;
  parseErrors: number;
  isSearching: boolean;
  isRebuildingIndex: boolean;
  runtimeSearchAvailable: boolean;
};

export function buildIndexerStatusView(
  status: IndexerStatus,
  progress: SearchIndexStatusView
): IndexerStatusView {
  if (status === 'Init') {
    return { label: status, tone: 'muted', message: '准备本地索引' };
  }

  if (status === 'Building') {
    if (progress.contentQueueDepth > 0) {
      return {
        label: status,
        tone: 'active',
        message: `正在建立索引，待解析 ${progress.contentQueueDepth}`
      };
    }
    return { label: status, tone: 'active', message: '正在建立索引' };
  }

  if (status === 'Suspended') {
    if (progress.contentQueueDepth > 0) {
      return {
        label: status,
        tone: 'warning',
        message: `资源受限，已暂停内容解析，待解析 ${progress.contentQueueDepth}`
      };
    }
    return { label: status, tone: 'warning', message: '资源受限，已暂停内容解析' };
  }

  return { label: status, tone: 'ready', message: '正在监听文件变化' };
}

export function buildSearchSurfaceNotice(
  input: SearchSurfaceNoticeInput
): SearchSurfaceNotice | null {
  if (!input.runtimeSearchAvailable) {
    return noticeFor('runtime_unavailable');
  }

  if (input.isSearching || input.isRebuildingIndex || input.status === 'Building') {
    return noticeFor('loading');
  }

  if (input.indexedFiles === 0) {
    return noticeFor('index_empty');
  }

  if (input.status === 'Suspended') {
    return noticeFor('suspended');
  }

  if (input.parseErrors > 0) {
    return noticeFor('parse_errors');
  }

  if (input.query.trim() && input.visibleResultCount === 0) {
    return noticeFor('no_results');
  }

  return null;
}

function noticeFor(code: SearchSurfaceNoticeCode): SearchSurfaceNotice {
  const configured = syntaxConfig.surfaceNotices[code];

  return {
    code,
    tone: normalizeNoticeTone(configured.tone),
    title: configured.title,
    message: configured.message,
    actionLabel: configured.actionLabel
  };
}

function normalizeNoticeTone(tone: string): SearchSurfaceNotice['tone'] {
  if (tone === 'active' || tone === 'warning') {
    return tone;
  }

  return 'muted';
}
