import { invoke } from '@tauri-apps/api/core';

export type PreviewContentRequest = {
  path: string;
};

export type PreviewContent = {
  content: string;
  source: string;
};

export function buildPreviewContentRequest(path: string): PreviewContentRequest {
  return { path };
}

export async function readPreviewContent(path: string): Promise<PreviewContent> {
  return invoke<PreviewContent>('read_preview_content_command', {
    request: buildPreviewContentRequest(path)
  });
}
