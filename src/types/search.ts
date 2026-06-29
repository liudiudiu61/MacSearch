import type { ParsedSearchQuery } from '../composables/useQueryParser';

export type SearchResult = {
  id: string;
  name: string;
  path: string;
  extension: string;
  modifiedAt: string;
  modifiedAtUnix: number;
  sizeBytes: number;
  kind: string;
  excerpt: string;
  hitSource: 'filename' | 'content';
  score: number;
  snippet: string | null;
};

export type IndexerStatus = 'Init' | 'Building' | 'Watching' | 'Suspended';

export type SearchStateSnapshot = {
  query: string;
  parsed: ParsedSearchQuery;
  selectedId: string | null;
  status: IndexerStatus;
};

export type SearchSidebarItem = {
  id: string;
  label: string;
  count: number | null;
  tone: 'default' | 'active' | 'muted';
};
