import { describe, expect, it } from 'vitest';
import { buildSearchFileNamesRequest, normalizeIndexStatus } from '../src/composables/useLocalSearch';
import { parseSearchQuery } from '../src/composables/useQueryParser';
import syntaxConfig from '../config/search_syntax.json';

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
      limit: 25
    });
  });

  it('normalizes Tauri index status into frontend casing', () => {
    expect(
      normalizeIndexStatus({
        indexed_files: 12,
        content_files: 5,
        parse_errors: 1
      })
    ).toEqual({
      indexedFiles: 12,
      contentFiles: 5,
      parseErrors: 1
    });
  });
});
