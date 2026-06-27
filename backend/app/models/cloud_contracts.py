from __future__ import annotations

from enum import StrEnum
from typing import Any

from sqlalchemy import JSON, Column
from sqlmodel import Field, SQLModel


class SubscriptionStatus(StrEnum):
    ACTIVE = "active"
    TRIALING = "trialing"
    PAST_DUE = "past_due"
    CANCELED = "canceled"


class UserSubscription(SQLModel, table=True):
    __tablename__ = "user_subscriptions"

    user_id: str = Field(primary_key=True)
    physical_id: str = Field(index=True, unique=True)
    professional_description: str
    status: SubscriptionStatus
    plan_code: str


class SynonymDictionaryEntry(SQLModel, table=True):
    __tablename__ = "synonym_dictionary"

    synonym_id: str = Field(primary_key=True)
    physical_id: str = Field(index=True, unique=True)
    professional_description: str
    source_term: str = Field(index=True)
    equivalent_terms: list[str] = Field(sa_column=Column(JSON))
    locale: str


class CloudConfigSync(SQLModel, table=True):
    __tablename__ = "cloud_config_sync"

    config_id: str = Field(primary_key=True)
    physical_id: str = Field(index=True, unique=True)
    professional_description: str
    config_payload: dict[str, Any] = Field(sa_column=Column(JSON))
    version: int = Field(gt=0)
