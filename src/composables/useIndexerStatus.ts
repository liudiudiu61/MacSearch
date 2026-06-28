import type { IndexerStatus } from '../types/search';

export type IndexerStatusTone = 'muted' | 'active' | 'ready' | 'warning';

export type IndexerStatusView = {
  label: IndexerStatus;
  tone: IndexerStatusTone;
  message: string;
};

export function buildIndexerStatusView(status: IndexerStatus): IndexerStatusView {
  if (status === 'Init') {
    return { label: status, tone: 'muted', message: '准备本地索引' };
  }

  if (status === 'Building') {
    return { label: status, tone: 'active', message: '正在建立索引' };
  }

  if (status === 'Suspended') {
    return { label: status, tone: 'warning', message: '资源受限，已暂停内容解析' };
  }

  return { label: status, tone: 'ready', message: '正在监听文件变化' };
}
