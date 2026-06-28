import { describe, expect, it } from 'vitest';
import { buildPreviewContentRequest } from '../src/composables/usePreviewContent';

describe('buildPreviewContentRequest', () => {
  it('uses the selected result physical path for the Tauri preview command', () => {
    expect(buildPreviewContentRequest('/tmp/maisou-note.md')).toEqual({
      path: '/tmp/maisou-note.md'
    });
  });
});
