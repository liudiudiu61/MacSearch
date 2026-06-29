import { describe, expect, it } from 'vitest';
import { filterSearchResults } from '../src/composables/useSearchFilter';
import { parseSearchQuery } from '../src/composables/useQueryParser';
import syntaxConfig from '../config/search_syntax.json';
import type { SearchResult } from '../src/types/search';

const results: SearchResult[] = [
  {
    id: 'plan',
    name: '高保真开发计划清单.md',
    path: '~/My_Project/麦搜/高保真开发计划清单.md',
    extension: 'md',
    modifiedAt: '2026-06-27 21:40',
    modifiedAtUnix: 1782577200,
    sizeBytes: 4096,
    kind: '文档',
    excerpt: 'Phase 3 front-end baseline.',
    hitSource: 'filename',
    score: 0,
    snippet: null
  },
  {
    id: 'prompt',
    name: 'v1.json',
    path: '~/My_Project/麦搜/prompts/nl2dsl/v1.json',
    extension: 'json',
    modifiedAt: '2026-06-27 18:52',
    modifiedAtUnix: 1782564720,
    sizeBytes: 1024,
    kind: '代码',
    excerpt: 'Strict DSL prompt.',
    hitSource: 'filename',
    score: 0,
    snippet: null
  }
];

describe('filterSearchResults', () => {
  it('uses configured field directives instead of matching directive text literally', () => {
    const parsed = parseSearchQuery('ext:md', syntaxConfig);

    expect(filterSearchResults(results, parsed).map((result) => result.id)).toEqual(['plan']);
  });
});
