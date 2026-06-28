import { describe, expect, it } from 'vitest';
import { buildQuickLookPreview } from '../src/composables/useQuickLookPreview';

describe('buildQuickLookPreview', () => {
  it('extracts 500 characters around the first match and returns local highlight offsets', () => {
    const before = 'a'.repeat(620);
    const after = 'b'.repeat(620);
    const content = `${before}Phase 3 target${after}`;

    const preview = buildQuickLookPreview(content, 'Phase 3 target', 500);

    expect(preview.text.length).toBe(1014);
    expect(preview.wasTruncatedBefore).toBe(true);
    expect(preview.wasTruncatedAfter).toBe(true);
    expect(preview.highlight).toEqual({ start: 500, end: 514 });
    expect(preview.highlight).not.toBeNull();
    if (preview.highlight) {
      expect(preview.text.slice(preview.highlight.start, preview.highlight.end)).toBe(
        'Phase 3 target'
      );
    }
  });

  it('falls back to the beginning of content when no match is available', () => {
    const preview = buildQuickLookPreview('short content', '', 500);

    expect(preview).toEqual({
      text: 'short content',
      highlight: null,
      wasTruncatedBefore: false,
      wasTruncatedAfter: false
    });
  });
});
