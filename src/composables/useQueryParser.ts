export type SearchField = 'text' | string;

export type SearchTerm = {
  field: SearchField;
  value: string;
  valid: boolean;
};

export type SearchError = {
  code: string;
  message: string;
};

export type SearchSyntaxConfig = {
  fieldDirectives: Array<{
    name: string;
    description: string;
  }>;
  fileTypeGroups?: Record<string, string[]>;
  regexDelimiter: string;
  errors: {
    invalidRegex: SearchError;
  };
};

export type ParsedSearchQuery = {
  terms: SearchTerm[];
  errors: SearchError[];
};

export function parseSearchQuery(
  query: string,
  syntax: SearchSyntaxConfig
): ParsedSearchQuery {
  const source = query.trim();

  if (!source) {
    return { terms: [], errors: [] };
  }

  if (looksLikeDelimitedRegex(source, syntax.regexDelimiter)) {
    return parseRegexTerm(source, syntax);
  }

  const fields = new Set(syntax.fieldDirectives.map((field) => field.name));
  const terms = tokenize(source).map<SearchTerm>((token) => {
    const separator = token.indexOf(':');

    if (separator > 0) {
      const field = token.slice(0, separator);
      const value = unquote(token.slice(separator + 1));

      if (fields.has(field)) {
        return { field, value, valid: true };
      }
    }

    return { field: 'text', value: unquote(token), valid: true };
  });

  return { terms, errors: [] };
}

function parseRegexTerm(source: string, syntax: SearchSyntaxConfig): ParsedSearchQuery {
  const body = source.slice(1, -1);

  try {
    new RegExp(body);
    return { terms: [{ field: 'text', value: source, valid: true }], errors: [] };
  } catch {
    return {
      terms: [{ field: 'text', value: source, valid: false }],
      errors: [syntax.errors.invalidRegex]
    };
  }
}

function looksLikeDelimitedRegex(source: string, delimiter: string): boolean {
  return source.startsWith(delimiter);
}

function tokenize(source: string): string[] {
  return source.match(/(?:[^\s"]+|"[^"]*")+/g) ?? [];
}

function unquote(value: string): string {
  if (value.startsWith('"') && value.endsWith('"')) {
    return value.slice(1, -1);
  }

  const separator = value.indexOf(':"');
  if (separator > 0 && value.endsWith('"')) {
    return `${value.slice(0, separator + 1)}${value.slice(separator + 2, -1)}`;
  }

  return value;
}
