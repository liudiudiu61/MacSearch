import type { ParsedSearchQuery, SearchTerm } from './useQueryParser';
import type { SearchResult } from '../types/search';

export type SearchModeFilter = 'all' | string;

export type SearchResultFilterOptions = {
  searchMode?: SearchModeFilter;
  fileTypeExtensions?: string[];
};

export function filterSearchResults(
  results: SearchResult[],
  parsed: ParsedSearchQuery,
  options: SearchResultFilterOptions = {}
): SearchResult[] {
  const validTerms = parsed.terms.filter((term) => term.valid && term.value.trim());
  const searchMode = options.searchMode ?? 'all';
  const fileTypeExtensions = new Set(
    (options.fileTypeExtensions ?? []).map((extension) => normalizeExtension(extension))
  );

  return results.filter((result) => {
    if (searchMode !== 'all' && result.hitSource !== searchMode) {
      return false;
    }

    if (fileTypeExtensions.size > 0 && !fileTypeExtensions.has(normalizeExtension(result.extension))) {
      return false;
    }

    return validTerms.every((term) => matchesTerm(result, term));
  });
}

function matchesTerm(result: SearchResult, term: SearchTerm): boolean {
  const value = normalize(term.value);

  if (term.field === 'name') {
    return normalize(result.name).includes(value);
  }

  if (term.field === 'content') {
    return normalize(result.excerpt).includes(value);
  }

  if (term.field === 'ext') {
    return normalize(result.extension) === value.replace(/^\./, '');
  }

  return normalize([result.name, result.path, result.extension, result.excerpt].join(' ')).includes(
    value
  );
}

function normalize(value: string): string {
  return value.trim().toLowerCase();
}

function normalizeExtension(value: string): string {
  return normalize(value).replace(/^\./, '');
}
