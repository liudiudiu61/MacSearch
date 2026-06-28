from __future__ import annotations

import json
import os
from collections.abc import AsyncIterator

from fastapi import APIRouter
from fastapi.responses import StreamingResponse

from app.models.nl2dsl import NL2DSLRequest
from app.services.nl2dsl import (
    DeterministicNL2DSLProvider,
    HttpNL2DSLProvider,
    NL2DSLProvider,
    SemanticCache,
    load_prompt_config,
    parse_nl_query_with_provider,
    parse_nl_query_with_cache,
)


router = APIRouter(prefix="/api/nl2dsl", tags=["nl2dsl"])
SHARED_SEMANTIC_CACHE = SemanticCache()


@router.post("/stream")
async def stream_nl2dsl(request: NL2DSLRequest) -> StreamingResponse:
    return StreamingResponse(
        build_nl2dsl_sse_events(request.query_text, SHARED_SEMANTIC_CACHE),
        media_type="text/event-stream",
    )


async def build_nl2dsl_sse_events(
    query_text: str,
    cache: SemanticCache | None = None,
    provider: NL2DSLProvider | None = None,
) -> AsyncIterator[str]:
    config = load_prompt_config()
    semantic_cache = cache or SHARED_SEMANTIC_CACHE
    cache_result = semantic_cache.find(query_text, config.semantic_cache_threshold)
    if cache_result is not None:
        cached_dsl, similarity = cache_result
        dsl = cached_dsl
        provider_result = None
        cache_hit = True
    else:
        selected_provider = provider or _provider_from_config(config)
        cache_hit = False
        similarity = 0.0
        if selected_provider is None:
            provider_result = None
            dsl = parse_nl_query_with_cache(query_text, semantic_cache, config).dsl
        else:
            provider_result = parse_nl_query_with_provider(
                query_text, selected_provider, config
            )
            dsl = provider_result.dsl
            semantic_cache.store(query_text, dsl)

    for event_name in config.sse_events:
        if event_name == "cache_hit" and not cache_hit:
            continue
        if event_name == "llm_validated" and provider_result is None:
            continue
        if event_name == "generated":
            payload = dsl.model_dump()
        elif event_name == "cache_hit":
            payload = {
                "query_text": query_text,
                "similarity": similarity,
            }
        elif event_name == "llm_validated":
            payload = {
                "query_text": query_text,
                "validated": provider_result.llm_validated,
            }
        else:
            payload = {"query_text": query_text}
        yield _format_sse_event(event_name, payload)


def _provider_from_config(config) -> NL2DSLProvider | None:
    if config.provider_mode == "deterministic":
        return DeterministicNL2DSLProvider(config)
    if config.provider_mode == "http":
        api_key_env = str(config.llm_provider["api_key_env"])
        api_key = os.environ.get(api_key_env)
        if not api_key:
            raise ValueError(f"Missing LLM API key environment variable: {api_key_env}")
        return HttpNL2DSLProvider(config, api_key)
    if config.provider_mode == "disabled":
        return None
    raise ValueError(f"Unsupported NL2DSL provider mode: {config.provider_mode}")


def _format_sse_event(event_name: str, payload: dict) -> str:
    data = json.dumps(payload, ensure_ascii=False)
    return f"event: {event_name}\ndata: {data}\n\n"
