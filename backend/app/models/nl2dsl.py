from __future__ import annotations

from pydantic import BaseModel, Field


class NL2DSLRequest(BaseModel):
    query_text: str = Field(min_length=1)


class DSLFilter(BaseModel):
    field: str
    operator: str
    value: str


class QueryDSL(BaseModel):
    query_text: str
    filters: list[DSLFilter]


class CachedQueryDSL(BaseModel):
    dsl: QueryDSL
    cache_hit: bool
    similarity: float


class ProviderQueryDSL(BaseModel):
    dsl: QueryDSL
    llm_validated: bool


class PromptConfig(BaseModel):
    version: str
    physical_id: str
    professional_description: str
    sse_events: list[str]
    semantic_cache_threshold: float = Field(ge=0, le=1)
    provider_mode: str
    llm_provider: dict[str, str | int]
    allowed_fields: list[str]
    allowed_operators: list[str]
    field_terms: dict[str, list[str]]
    synonyms: dict[str, str]
    default_field: str
    default_operator: str
