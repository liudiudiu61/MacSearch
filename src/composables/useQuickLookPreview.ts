export type QuickLookHighlight = {
  start: number;
  end: number;
};

export type QuickLookPreview = {
  text: string;
  highlight: QuickLookHighlight | null;
  wasTruncatedBefore: boolean;
  wasTruncatedAfter: boolean;
};

export function buildQuickLookPreview(
  content: string,
  matchText: string,
  contextChars: number
): QuickLookPreview {
  const normalizedContext = Math.max(0, contextChars);

  if (!matchText.trim()) {
    const text = content.slice(0, normalizedContext * 2 || content.length);
    return {
      text,
      highlight: null,
      wasTruncatedBefore: false,
      wasTruncatedAfter: text.length < content.length
    };
  }

  const matchStart = content.toLowerCase().indexOf(matchText.toLowerCase());
  if (matchStart < 0) {
    const text = content.slice(0, normalizedContext * 2 || content.length);
    return {
      text,
      highlight: null,
      wasTruncatedBefore: false,
      wasTruncatedAfter: text.length < content.length
    };
  }

  const matchEnd = matchStart + matchText.length;
  const sliceStart = Math.max(0, matchStart - normalizedContext);
  const sliceEnd = Math.min(content.length, matchEnd + normalizedContext);

  return {
    text: content.slice(sliceStart, sliceEnd),
    highlight: {
      start: matchStart - sliceStart,
      end: matchEnd - sliceStart
    },
    wasTruncatedBefore: sliceStart > 0,
    wasTruncatedAfter: sliceEnd < content.length
  };
}
