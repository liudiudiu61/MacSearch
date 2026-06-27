from __future__ import annotations

import json
import sqlite3
from pathlib import Path

from app.config.seed_loader import LocalSeedConfig


def initialize_local_database(
    db_path: str | Path, seed: LocalSeedConfig | None = None
) -> sqlite3.Connection:
    connection = sqlite3.connect(db_path)
    connection.row_factory = sqlite3.Row
    connection.execute("PRAGMA foreign_keys = ON")
    create_schema(connection)
    if seed is not None:
        seed_local_config(connection, seed)
    return connection


def create_schema(connection: sqlite3.Connection) -> None:
    connection.executescript(
        """
        CREATE TABLE IF NOT EXISTS rule_catalog (
            rule_id TEXT PRIMARY KEY,
            physical_id TEXT NOT NULL UNIQUE,
            professional_description TEXT NOT NULL,
            rule_kind TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS search_strategies (
            rule_id TEXT PRIMARY KEY,
            physical_id TEXT NOT NULL UNIQUE,
            professional_description TEXT NOT NULL,
            target_ext TEXT NOT NULL,
            enable_content_idx INTEGER NOT NULL CHECK (enable_content_idx IN (0, 1)),
            priority_path TEXT NOT NULL,
            parser_type TEXT NOT NULL,
            max_size_mb INTEGER NOT NULL CHECK (max_size_mb > 0),
            FOREIGN KEY (rule_id)
                REFERENCES rule_catalog(rule_id)
                ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS blacklists (
            rule_id TEXT PRIMARY KEY,
            physical_id TEXT NOT NULL UNIQUE,
            professional_description TEXT NOT NULL,
            path_pattern TEXT NOT NULL,
            match_type TEXT NOT NULL,
            is_enabled INTEGER NOT NULL CHECK (is_enabled IN (0, 1)),
            FOREIGN KEY (rule_id)
                REFERENCES rule_catalog(rule_id)
                ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS file_index (
            file_id TEXT PRIMARY KEY,
            parent_file_id TEXT,
            physical_path TEXT NOT NULL UNIQUE,
            professional_description TEXT NOT NULL,
            file_name TEXT NOT NULL,
            extension TEXT NOT NULL,
            modified_at INTEGER NOT NULL,
            size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
            content_status TEXT NOT NULL,
            strategy_rule_id TEXT,
            FOREIGN KEY (parent_file_id)
                REFERENCES file_index(file_id)
                ON DELETE CASCADE,
            FOREIGN KEY (strategy_rule_id)
                REFERENCES search_strategies(rule_id)
                ON DELETE SET NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS file_index_fts USING fts5(
            file_name,
            content_text,
            content='file_index',
            content_rowid='rowid'
        );
        """
    )


def seed_local_config(connection: sqlite3.Connection, seed: LocalSeedConfig) -> None:
    for strategy in seed.search_strategies:
        connection.execute(
            """
            INSERT OR REPLACE INTO rule_catalog
            (rule_id, physical_id, professional_description, rule_kind)
            VALUES (?, ?, ?, ?)
            """,
            (
                strategy.rule_id,
                strategy.physical_id,
                strategy.professional_description,
                "search_strategy",
            ),
        )
        connection.execute(
            """
            INSERT OR REPLACE INTO search_strategies
            (rule_id, physical_id, professional_description, target_ext,
             enable_content_idx, priority_path, parser_type, max_size_mb)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                strategy.rule_id,
                strategy.physical_id,
                strategy.professional_description,
                json.dumps(strategy.target_ext, ensure_ascii=False),
                int(strategy.enable_content_idx),
                json.dumps(strategy.priority_path, ensure_ascii=False),
                strategy.parser_type,
                strategy.max_size_mb,
            ),
        )

    for blacklist in seed.blacklists:
        connection.execute(
            """
            INSERT OR REPLACE INTO rule_catalog
            (rule_id, physical_id, professional_description, rule_kind)
            VALUES (?, ?, ?, ?)
            """,
            (
                blacklist.rule_id,
                blacklist.physical_id,
                blacklist.professional_description,
                "blacklist",
            ),
        )
        connection.execute(
            """
            INSERT OR REPLACE INTO blacklists
            (rule_id, physical_id, professional_description, path_pattern,
             match_type, is_enabled)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            (
                blacklist.rule_id,
                blacklist.physical_id,
                blacklist.professional_description,
                blacklist.path_pattern,
                blacklist.match_type,
                int(blacklist.is_enabled),
            ),
        )
    connection.commit()
