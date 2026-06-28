from __future__ import annotations

import json
import math
import urllib.request
from pathlib import Path
from collections.abc import Callable
from typing import Any
from typing import Protocol

from pydantic import ValidationError

from app.models.nl2dsl import (
    CachedQueryDSL,
    DSLFilter,
    PromptConfig,
    ProviderQueryDSL,
    QueryDSL,
)


PROJECT_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_PROMPT_PATH = PROJECT_ROOT / "prompts" / "nl2dsl" / "v1.json"


class SemanticCache:
    def __init__(self, embed_text: Callable[[str], list[float]] | None = None) -> None:
        self._entries: list[tuple[str, QueryDSL]] = []
        self._embed_text = embed_text or _default_embed_text

    def store(self, query_text: str, dsl: QueryDSL) -> None:
        self._entries.append((query_text, dsl))

    def find(self, query_text: str, threshold: float) -> tuple[QueryDSL, float] | None:
        best_entry: tuple[QueryDSL, float] | None = None
        for cached_query, dsl in self._entries:
            similarity = _cosine_similarity(
                self._embed_text(query_text), self._embed_text(cached_query)
            )
            if similarity >= threshold and (
                best_entry is None or similarity > best_entry[1]
            ):
                best_entry = (dsl, similarity)
        return best_entry


class NL2DSLProvider(Protocol):
    def generate_dsl(self, query_text: str) -> str:
        """Return a JSON string matching the QueryDSL contract."""


class DeterministicNL2DSLProvider:
    def __init__(self, config: PromptConfig) -> None:
        self._config = config

    def generate_dsl(self, query_text: str) -> str:
        dsl = parse_nl_query(query_text, self._config)
        return dsl.model_dump_json()


class HttpNL2DSLProvider:
    def __init__(
        self,
        config: PromptConfig,
        api_key: str,
        transport: Callable[[dict[str, Any], dict[str, str], int], dict[str, Any]]
        | None = None,
    ) -> None:
        self._config = config
        self._api_key = api_key
        self._transport = transport or self._post_json

    def generate_dsl(self, query_text: str) -> str:
        provider_config = self._config.llm_provider
        request_payload = {
            "model": provider_config["model"],
            "messages": [
                {
                    "role": "system",
                    "content": provider_config["system_prompt"],
                },
                {
                    "role": "user",
                    "content": query_text,
                },
            ],
            "temperature": 0,
        }
        headers = {
            "Authorization": f"Bearer {self._api_key}",
            "Content-Type": "application/json",
        }
        response_payload = self._transport(
            request_payload,
            headers,
            int(provider_config["timeout_seconds"]),
        )
        return _extract_provider_content(response_payload)

    def _post_json(
        self,
        request_payload: dict[str, Any],
        headers: dict[str, str],
        timeout_seconds: int,
    ) -> dict[str, Any]:
        endpoint = str(self._config.llm_provider["endpoint"])
        request = urllib.request.Request(
            endpoint,
            data=json.dumps(request_payload).encode("utf-8"),
            headers=headers,
            method="POST",
        )
        with urllib.request.urlopen(request, timeout=timeout_seconds) as response:
            return json.loads(response.read().decode("utf-8"))


def load_prompt_config(path: str | Path = DEFAULT_PROMPT_PATH) -> PromptConfig:
    payload = json.loads(Path(path).read_text(encoding="utf-8"))
    return PromptConfig.model_validate(payload)


def parse_nl_query(query_text: str, config: PromptConfig | None = None) -> QueryDSL:
    prompt_config = config or load_prompt_config()
    filters: list[DSLFilter] = []

    for field, terms in prompt_config.field_terms.items():
        if any(term in query_text for term in terms):
            value = _value_for_field(field, query_text, prompt_config)
            filters.append(
                _validated_filter(
                    field=field,
                    operator=prompt_config.default_operator,
                    value=value,
                    config=prompt_config,
                )
            )

    if not filters:
        filters.append(
            _validated_filter(
                field=prompt_config.default_field,
                operator=prompt_config.default_operator,
                value=query_text,
                config=prompt_config,
            )
        )

    return QueryDSL(query_text=query_text, filters=filters)


def parse_nl_query_with_cache(
    query_text: str,
    cache: SemanticCache,
    config: PromptConfig | None = None,
) -> CachedQueryDSL:
    prompt_config = config or load_prompt_config()
    cached = cache.find(query_text, prompt_config.semantic_cache_threshold)
    if cached is not None:
        dsl, similarity = cached
        return CachedQueryDSL(dsl=dsl, cache_hit=True, similarity=similarity)

    dsl = parse_nl_query(query_text, prompt_config)
    cache.store(query_text, dsl)
    return CachedQueryDSL(dsl=dsl, cache_hit=False, similarity=0)


def parse_llm_response(
    response_text: str, config: PromptConfig | None = None
) -> QueryDSL:
    prompt_config = config or load_prompt_config()
    try:
        payload = json.loads(response_text)
    except json.JSONDecodeError as exc:
        raise ValueError("LLM response must be valid JSON") from exc

    try:
        dsl = QueryDSL.model_validate(payload)
    except ValidationError as exc:
        raise ValueError("LLM response does not match QueryDSL schema") from exc

    for filter_item in dsl.filters:
        _validated_filter(
            field=filter_item.field,
            operator=filter_item.operator,
            value=filter_item.value,
            config=prompt_config,
        )
    return dsl


def parse_nl_query_with_provider(
    query_text: str,
    provider: NL2DSLProvider,
    config: PromptConfig | None = None,
) -> ProviderQueryDSL:
    prompt_config = config or load_prompt_config()
    response_text = provider.generate_dsl(query_text)
    dsl = parse_llm_response(response_text, prompt_config)
    return ProviderQueryDSL(dsl=dsl, llm_validated=True)


def _extract_provider_content(response_payload: dict[str, Any]) -> str:
    try:
        return str(response_payload["choices"][0]["message"]["content"])
    except (KeyError, IndexError, TypeError) as exc:
        raise ValueError("LLM provider response is missing message content") from exc


def _value_for_field(field: str, query_text: str, config: PromptConfig) -> str:
    if field == "extension":
        for term, replacement in config.synonyms.items():
            if term in query_text:
                return replacement
    return query_text


def _validated_filter(
    field: str, operator: str, value: str, config: PromptConfig
) -> DSLFilter:
    if field not in config.allowed_fields:
        raise ValueError(f"Unsupported NL2DSL field: {field}")
    if operator not in config.allowed_operators:
        raise ValueError(f"Unsupported NL2DSL operator: {operator}")
    return DSLFilter(field=field, operator=operator, value=value)


def _default_embed_text(text: str) -> list[float]:
    tokens = _tokenize_text(text)
    dimensions = ["架构", "设计", "markdown", "steam", "存档", "目录"]
    return [1.0 if dimension in tokens else 0.0 for dimension in dimensions]


def _tokenize_text(text: str) -> str:
    return text.lower()


def _cosine_similarity(left: list[float], right: list[float]) -> float:
    if len(left) != len(right):
        raise ValueError("Embedding vectors must have matching dimensions")

    dot_product = sum(left_value * right_value for left_value, right_value in zip(left, right))
    left_norm = math.sqrt(sum(value * value for value in left))
    right_norm = math.sqrt(sum(value * value for value in right))
    if left_norm == 0 or right_norm == 0:
        return 0
    return dot_product / (left_norm * right_norm)
