import { invoke } from '@tauri-apps/api/core';
import type { ParsedSearchQuery } from './useQueryParser';
import type { SearchResult } from '../types/search';

export type SearchFileNamesRequest = {
  query: {
    name: string | null;
    content: string | null;
    extension: string | null;
    text: string | null;
  };
  limit: number;
};

export type SearchIndexStatus = {
  indexed_files: number;
  content_files: number;
  parse_errors: number;
};

export type SearchIndexStatusView = {
  indexedFiles: number;
  contentFiles: number;
  parseErrors: number;
};

type SearchFileNameHit = {
  id: string;
  name: string;
  path: string;
  extension: string;
  modified_at: number;
};

export function buildSearchFileNamesRequest(
  parsed: ParsedSearchQuery,
  limit: number
): SearchFileNamesRequest {
  const request: SearchFileNamesRequest = {
    query: {
      name: null,
      content: null,
      extension: null,
      text: null
    },
    limit
  };

  for (const term of parsed.terms) {
    if (!term.valid) {
      continue;
    }

    if (term.field === 'name') {
      request.query.name = term.value;
    } else if (term.field === 'content') {
      request.query.content = term.value;
    } else if (term.field === 'ext') {
      request.query.extension = term.value;
    } else if (term.field === 'text') {
      request.query.text = term.value;
    }
  }

  return request;
}

export async function searchFileNames(
  parsed: ParsedSearchQuery,
  limit: number
): Promise<SearchResult[]> {
  const hits = await invoke<SearchFileNameHit[]>('search_file_names_command', {
    request: buildSearchFileNamesRequest(parsed, limit)
  });

  return hits.map((hit) => ({
    id: hit.id,
    name: hit.name,
    path: hit.path,
    extension: hit.extension,
    modifiedAt: formatUnixSeconds(hit.modified_at),
    excerpt: hit.path
  }));
}

export async function rebuildSearchIndex(): Promise<SearchIndexStatusView> {
  return normalizeIndexStatus(await invoke<SearchIndexStatus>('rebuild_search_index_command'));
}

export async function getSearchIndexStatus(): Promise<SearchIndexStatusView> {
  return normalizeIndexStatus(await invoke<SearchIndexStatus>('get_search_index_status_command'));
}

export async function openSearchResult(path: string): Promise<void> {
  await invoke('open_file_command', {
    request: { path }
  });
}

export async function revealSearchResult(path: string): Promise<void> {
  await invoke('reveal_file_command', {
    request: { path }
  });
}

export function normalizeIndexStatus(status: SearchIndexStatus): SearchIndexStatusView {
  return {
    indexedFiles: status.indexed_files,
    contentFiles: status.content_files,
    parseErrors: status.parse_errors
  };
}

function formatUnixSeconds(value: number): string {
  if (value <= 0) {
    return '';
  }

  return new Date(value * 1000).toISOString().slice(0, 16).replace('T', ' ');
}
