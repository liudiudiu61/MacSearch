import pytest

from app.main import create_app
from app.api.nl2dsl import build_nl2dsl_sse_events, router
from app.services.nl2dsl import (
    DeterministicNL2DSLProvider,
    HttpNL2DSLProvider,
    SemanticCache,
    load_prompt_config,
    parse_llm_response,
    parse_nl_query,
    parse_nl_query_with_provider,
    parse_nl_query_with_cache,
)


@pytest.fixture
def anyio_backend():
    return "asyncio"


def test_loads_prompt_config_from_json_file():
    config = load_prompt_config()

    assert config.version == "v1"
    assert "received" in config.sse_events
    assert "generated" in config.sse_events
    assert "llm_validated" in config.sse_events
    assert config.semantic_cache_threshold == 0.92
    assert config.provider_mode == "deterministic"


def test_parse_nl_query_returns_validated_dsl_from_configured_terms():
    dsl = parse_nl_query("找 Markdown 里的架构设计")

    assert dsl.query_text == "找 Markdown 里的架构设计"
    assert any(filter_item.field == "content" for filter_item in dsl.filters)
    assert any(filter_item.field == "extension" for filter_item in dsl.filters)


def test_parse_nl_query_reuses_cached_dsl_above_configured_similarity():
    cached_dsl = parse_nl_query("找 Markdown 里的架构设计")
    cache = SemanticCache()
    cache.store("找 Markdown 里的架构设计", cached_dsl)

    result = parse_nl_query_with_cache("找 Markdown 里的架构设计", cache)

    assert result.cache_hit is True
    assert result.dsl == cached_dsl


def test_parse_nl_query_generates_fresh_dsl_below_configured_similarity():
    cached_dsl = parse_nl_query("找 Markdown 里的架构设计")
    cache = SemanticCache()
    cache.store("找 Markdown 里的架构设计", cached_dsl)

    result = parse_nl_query_with_cache("搜索 Steam 存档目录", cache)

    assert result.cache_hit is False
    assert result.dsl != cached_dsl


def test_semantic_cache_uses_configured_vector_similarity_above_threshold():
    cached_dsl = parse_nl_query("找 Markdown 里的架构设计")
    cache = SemanticCache(
        embed_text=lambda text: [1.0, 0.0] if "架构" in text or "设计" in text else [0.0, 1.0]
    )
    cache.store("找 Markdown 里的架构设计", cached_dsl)

    result = parse_nl_query_with_cache("查 设计 文档", cache)

    assert result.cache_hit is True
    assert result.dsl == cached_dsl
    assert result.similarity == 1.0


class FakeProvider:
    def __init__(self, response_text: str) -> None:
        self.response_text = response_text

    def generate_dsl(self, query_text: str) -> str:
        return self.response_text


def test_parse_llm_response_accepts_valid_configured_dsl_json():
    response_text = """
    {
      "query_text": "找 Markdown 里的架构设计",
      "filters": [
        {"field": "content", "operator": "contains", "value": "架构设计"},
        {"field": "extension", "operator": "equals", "value": ".md"}
      ]
    }
    """

    dsl = parse_llm_response(response_text)

    assert dsl.query_text == "找 Markdown 里的架构设计"
    assert dsl.filters[0].field == "content"


def test_parse_llm_response_rejects_unknown_configured_field():
    response_text = """
    {
      "query_text": "找图片",
      "filters": [
        {"field": "image_kind", "operator": "contains", "value": "png"}
      ]
    }
    """

    with pytest.raises(ValueError, match="Unsupported NL2DSL field"):
        parse_llm_response(response_text)


def test_parse_llm_response_rejects_malformed_json():
    with pytest.raises(ValueError, match="valid JSON"):
        parse_llm_response("content contains architecture")


def test_parse_nl_query_with_provider_returns_validated_llm_dsl():
    provider = FakeProvider(
        """
        {
          "query_text": "找 Markdown 里的架构设计",
          "filters": [
            {"field": "content", "operator": "contains", "value": "架构设计"}
          ]
        }
        """
    )

    result = parse_nl_query_with_provider("找 Markdown 里的架构设计", provider)

    assert result.llm_validated is True
    assert result.dsl.filters[0].value == "架构设计"


def test_deterministic_provider_generates_validated_json_from_prompt_config():
    provider = DeterministicNL2DSLProvider(load_prompt_config())

    result = parse_nl_query_with_provider("找 Markdown 里的架构设计", provider)

    assert result.llm_validated is True
    assert any(filter_item.field == "extension" for filter_item in result.dsl.filters)


def test_http_provider_posts_configured_prompt_and_extracts_response_json():
    captured_request = {}

    def fake_transport(request_payload, headers, timeout_seconds):
        captured_request["payload"] = request_payload
        captured_request["headers"] = headers
        captured_request["timeout_seconds"] = timeout_seconds
        return {
            "choices": [
                {
                    "message": {
                        "content": """
                        {
                          "query_text": "找 Markdown 里的架构设计",
                          "filters": [
                            {"field": "content", "operator": "contains", "value": "架构设计"}
                          ]
                        }
                        """
                    }
                }
            ]
        }

    config = load_prompt_config()
    provider = HttpNL2DSLProvider(
        config=config,
        api_key="test-key",
        transport=fake_transport,
    )

    result = parse_nl_query_with_provider("找 Markdown 里的架构设计", provider, config)

    assert result.llm_validated is True
    assert captured_request["payload"]["model"] == config.llm_provider["model"]
    assert captured_request["headers"]["Authorization"] == "Bearer test-key"
    assert captured_request["timeout_seconds"] == config.llm_provider["timeout_seconds"]


def test_create_app_registers_nl2dsl_stream_route():
    app = create_app()

    assert app is not None
    assert any(
        getattr(route, "path", None) == "/api/nl2dsl/stream" for route in router.routes
    )


@pytest.mark.anyio
async def test_nl2dsl_stream_emits_configured_sse_events():
    cached_dsl = parse_nl_query("找 Markdown 里的架构设计")
    cache = SemanticCache()
    cache.store("找 Markdown 里的架构设计", cached_dsl)
    chunks = [
        chunk
        async for chunk in build_nl2dsl_sse_events("找 Markdown 里的架构设计", cache)
    ]
    body = "".join(chunks)

    assert "event: received" in body
    assert "event: analyzing" in body
    assert "event: cache_hit" in body
    assert "event: generated" in body


@pytest.mark.anyio
async def test_nl2dsl_stream_emits_llm_validated_event_for_provider():
    provider = FakeProvider(
        """
        {
          "query_text": "找 Markdown 里的架构设计",
          "filters": [
            {"field": "content", "operator": "contains", "value": "架构设计"}
          ]
        }
        """
    )
    chunks = [
        chunk
        async for chunk in build_nl2dsl_sse_events(
            "找 Markdown 里的架构设计",
            provider=provider,
        )
    ]
    body = "".join(chunks)

    assert "event: llm_validated" in body
    assert "event: generated" in body


@pytest.mark.anyio
async def test_nl2dsl_stream_uses_shared_cache_between_requests():
    cache = SemanticCache()

    first_chunks = [
        chunk async for chunk in build_nl2dsl_sse_events("找 Markdown 里的架构设计", cache)
    ]
    second_chunks = [
        chunk async for chunk in build_nl2dsl_sse_events("找 Markdown 里的架构设计", cache)
    ]

    assert "event: cache_hit" not in "".join(first_chunks)
    assert "event: cache_hit" in "".join(second_chunks)


@pytest.mark.anyio
async def test_nl2dsl_stream_uses_configured_provider_mode_by_default():
    chunks = [
        chunk
        async for chunk in build_nl2dsl_sse_events(
            "找 Markdown 里的架构设计",
            SemanticCache(),
        )
    ]
    body = "".join(chunks)

    assert "event: llm_validated" in body
    assert "event: generated" in body
