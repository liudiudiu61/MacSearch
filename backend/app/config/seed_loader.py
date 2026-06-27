from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from pydantic import BaseModel, Field


class SearchStrategySeed(BaseModel):
    rule_id: str
    physical_id: str
    professional_description: str
    target_ext: list[str]
    enable_content_idx: bool
    priority_path: list[str]
    parser_type: str
    max_size_mb: int = Field(gt=0)


class BlacklistSeed(BaseModel):
    rule_id: str
    physical_id: str
    professional_description: str
    path_pattern: str
    match_type: str
    is_enabled: bool


class LocalSeedConfig(BaseModel):
    search_strategies: list[SearchStrategySeed]
    blacklists: list[BlacklistSeed]


def load_seed_config(path: str | Path) -> LocalSeedConfig:
    payload: dict[str, Any] = json.loads(Path(path).read_text(encoding="utf-8"))
    return LocalSeedConfig.model_validate(payload)
