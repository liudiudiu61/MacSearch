from __future__ import annotations

import json
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, Field


class IndexerSuspensionRules(BaseModel):
    physical_id: str
    professional_description: str
    battery_percent_below: int = Field(ge=0, le=100)
    cpu_load_percent_above: int = Field(ge=0, le=100)
    cpu_temperature_celsius_above: int = Field(gt=0)
    unknown_resource_state_policy: Literal["suspend_content_indexing"]


class PermissionGuidanceRules(BaseModel):
    physical_id: str
    professional_description: str
    unauthorized_scan_scope: str
    authorized_scan_scope: str


class ControlRules(BaseModel):
    indexer_suspension: IndexerSuspensionRules
    permission_guidance: PermissionGuidanceRules


def load_control_rules(path: str | Path) -> ControlRules:
    payload = json.loads(Path(path).read_text(encoding="utf-8"))
    return ControlRules.model_validate(payload)
