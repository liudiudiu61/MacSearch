import { describe, expect, it } from 'vitest';
import { buildIndexerStatusView } from '../src/composables/useIndexerStatus';

describe('buildIndexerStatusView', () => {
  it('maps core indexer states to stable UI feedback', () => {
    expect(buildIndexerStatusView('Init')).toEqual({
      label: 'Init',
      tone: 'muted',
      message: '准备本地索引'
    });
    expect(buildIndexerStatusView('Building')).toEqual({
      label: 'Building',
      tone: 'active',
      message: '正在建立索引'
    });
    expect(buildIndexerStatusView('Watching')).toEqual({
      label: 'Watching',
      tone: 'ready',
      message: '正在监听文件变化'
    });
    expect(buildIndexerStatusView('Suspended')).toEqual({
      label: 'Suspended',
      tone: 'warning',
      message: '资源受限，已暂停内容解析'
    });
  });
});
