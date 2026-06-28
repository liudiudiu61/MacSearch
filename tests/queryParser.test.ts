import { describe, expect, it } from 'vitest';
import { parseSearchQuery } from '../src/composables/useQueryParser';
import syntaxConfig from '../config/search_syntax.json';

describe('parseSearchQuery', () => {
  it('classifies configured field directives and keeps quoted content intact', () => {
    const parsed = parseSearchQuery('name:roadmap ext:md content:"Phase 3 plan"', syntaxConfig);

    expect(parsed.errors).toEqual([]);
    expect(parsed.terms).toEqual([
      { field: 'name', value: 'roadmap', valid: true },
      { field: 'ext', value: 'md', valid: true },
      { field: 'content', value: 'Phase 3 plan', valid: true }
    ]);
  });

  it('downgrades an invalid regex to a plain text term with an error', () => {
    const parsed = parseSearchQuery('/(phase-3/', syntaxConfig);

    expect(parsed.terms).toEqual([{ field: 'text', value: '/(phase-3/', valid: false }]);
    expect(parsed.errors).toEqual([
      { code: 'invalid_regex', message: 'Regular expression is incomplete.' }
    ]);
  });
});
