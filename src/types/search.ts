import type { ParsedSearchQuery } from '../composables/useQueryParser';

export type SearchResult = {
  id: string;
  name: string;
  path: string;
  extension: string;
  modifiedAt: string;
  excerpt: string;
};

export type IndexerStatus = 'Init' | 'Building' | 'Watching' | 'Suspended';

export type SearchStateSnapshot = {
  query: string;
  parsed: ParsedSearchQuery;
  selectedId: string | null;
  status: IndexerStatus;
};
