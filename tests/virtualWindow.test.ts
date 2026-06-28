import { describe, expect, it } from 'vitest';
import { calculateVirtualWindow } from '../src/composables/useVirtualWindow';

describe('calculateVirtualWindow', () => {
  it('returns a stable overscanned render window for a scrolled list', () => {
    const window = calculateVirtualWindow({
      total: 120,
      rowHeight: 44,
      viewportHeight: 220,
      scrollTop: 198,
      overscan: 2
    });

    expect(window).toEqual({
      start: 2,
      end: 12,
      offsetTop: 88,
      totalHeight: 5280
    });
  });
});
