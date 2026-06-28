import type { ParsedSearchQuery, SearchTerm } from './useQueryParser';
import type { SearchResult } from '../types/search';

export function filterSearchResults(
  results: SearchResult[],
  parsed: ParsedSearchQuery
): SearchResult[] {
  const validTerms = parsed.terms.filter((term) => term.valid && term.value.trim());

  if (validTerms.length === 0) {
    return results;
  }

  return results.filter((result) => validTerms.every((term) => matchesTerm(result, term)));
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
