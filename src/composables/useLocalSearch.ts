import { invoke } from '@tauri-apps/api/core';
import syntaxConfig from '../../config/search_syntax.json';
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
  mode: SearchMode;
};

export type SearchMode = 'filename' | 'content' | 'mixed';

export type SearchIndexStatus = {
  indexed_files: number;
  content_files: number;
  parse_errors: number;
  content_queue_depth: number;
};

export type SearchIndexStatusView = {
  indexedFiles: number;
  contentFiles: number;
  parseErrors: number;
  contentQueueDepth: number;
};

export type IndexerPolicySettings = {
  scan_roots: string[];
  watch_roots: string[];
  exclude_path_fragments: string[];
  max_parse_size_bytes: number;
  text_extensions: string[];
  content_index: {
    enabled: boolean;
    batch_size: number;
    snippet_radius: number;
    default_limit: number;
  };
};

export type SearchFileNameHit = {
  id: string;
  name: string;
  path: string;
  extension: string;
  modified_at: number;
  size_bytes: number;
  kind: string | null;
  hit_source: 'filename' | 'content';
  score: number;
  snippet: string | null;
};

export function buildSearchFileNamesRequest(
  parsed: ParsedSearchQuery,
  limit: number,
  mode: SearchMode = 'mixed'
): SearchFileNamesRequest {
  const request: SearchFileNamesRequest = {
    query: {
      name: null,
      content: null,
      extension: null,
      text: null
    },
    limit,
    mode
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
  limit: number,
  mode: SearchMode = 'mixed'
): Promise<SearchResult[]> {
  const hits = await invoke<SearchFileNameHit[]>('search_file_names_command', {
    request: buildSearchFileNamesRequest(parsed, limit, mode)
  });

  return mapSearchFileNameHits(hits);
}

export function mapSearchFileNameHits(hits: SearchFileNameHit[]): SearchResult[] {
  return hits.map((hit) => ({
    id: hit.id,
    name: hit.name,
    path: hit.path,
    extension: hit.extension,
    modifiedAt: formatUnixSeconds(hit.modified_at),
    modifiedAtUnix: hit.modified_at,
    sizeBytes: hit.size_bytes,
    kind: hit.kind ?? kindForExtension(hit.extension),
    excerpt: hit.hit_source === 'content' && hit.snippet ? hit.snippet : hit.path,
    hitSource: hit.hit_source,
    score: hit.score,
    snippet: hit.snippet
  }));
}

export async function rebuildSearchIndex(): Promise<SearchIndexStatusView> {
  return normalizeIndexStatus(await invoke<SearchIndexStatus>('rebuild_search_index_command'));
}

export async function getSearchIndexStatus(): Promise<SearchIndexStatusView> {
  return normalizeIndexStatus(await invoke<SearchIndexStatus>('get_search_index_status_command'));
}

export async function getIndexerPolicySettings(): Promise<IndexerPolicySettings> {
  return invoke<IndexerPolicySettings>('get_indexer_policy_settings_command');
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
    parseErrors: status.parse_errors,
    contentQueueDepth: status.content_queue_depth
  };
}

function formatUnixSeconds(value: number): string {
  if (value <= 0) {
    return '';
  }

  return new Date(value * 1000).toISOString().slice(0, 16).replace('T', ' ');
}

export function formatFileSize(sizeBytes: number): string {
  if (sizeBytes < 1024) {
    return `${sizeBytes} B`;
  }

  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = sizeBytes / 1024;
  let unitIndex = 0;

  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }

  const rounded = value >= 10 ? Math.round(value).toString() : value.toFixed(1);
  return `${rounded} ${units[unitIndex]}`;
}

function kindForExtension(extension: string): string {
  const normalized = extension.trim().replace(/^\./, '').toLowerCase();
  const group = syntaxConfig.extensionKindGroups.find((item) =>
    item.extensions.some((candidate) => candidate.toLowerCase() === normalized)
  );

  return group?.kind ?? '文件';
}
