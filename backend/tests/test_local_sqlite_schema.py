import json
import sqlite3
from pathlib import Path

from app.config.seed_loader import load_seed_config
from app.db.local_sqlite import initialize_local_database


PROJECT_ROOT = Path(__file__).resolve().parents[2]


def table_names(connection: sqlite3.Connection) -> set[str]:
    rows = connection.execute(
        "SELECT name FROM sqlite_master WHERE type IN ('table', 'virtual')"
    ).fetchall()
    return {row[0] for row in rows}


def foreign_keys(connection: sqlite3.Connection, table_name: str) -> list[sqlite3.Row]:
    return connection.execute(f"PRAGMA foreign_key_list({table_name})").fetchall()


def test_initializes_phase_one_local_schema_with_fts5_and_cascades(tmp_path):
    db_path = tmp_path / "maisou.sqlite"

    connection = initialize_local_database(db_path)

    names = table_names(connection)
    assert {"search_strategies", "blacklists", "file_index", "file_index_fts"} <= names
    assert any(row["table"] == "rule_catalog" for row in foreign_keys(connection, "search_strategies"))
    assert any(row["table"] == "file_index" and row["on_delete"] == "CASCADE" for row in foreign_keys(connection, "file_index"))

    strategy_columns = {
        row["name"] for row in connection.execute("PRAGMA table_info(search_strategies)")
    }
    assert {"physical_id", "professional_description", "target_ext", "parser_type"} <= strategy_columns


def test_loads_seed_rules_from_configuration_file(tmp_path):
    seed_path = tmp_path / "seed.json"
    seed_path.write_text(
        json.dumps(
            {
                "search_strategies": [
                    {
                        "rule_id": "rule_test_docs",
                        "physical_id": "local.rule.test_docs",
                        "professional_description": "Test document workspace",
                        "target_ext": [".md", ".txt"],
                        "enable_content_idx": True,
                        "priority_path": ["~/Documents/TestNotes"],
                        "parser_type": "text_parser",
                        "max_size_mb": 50,
                    }
                ],
                "blacklists": [
                    {
                        "rule_id": "blacklist_test_build",
                        "physical_id": "local.blacklist.test_build",
                        "professional_description": "Generated build output",
                        "path_pattern": "~/Documents/TestNotes/build",
                        "match_type": "path_prefix",
                        "is_enabled": True,
                    }
                ],
            }
        ),
        encoding="utf-8",
    )

    seed = load_seed_config(seed_path)
    connection = initialize_local_database(tmp_path / "seeded.sqlite", seed)

    strategy = connection.execute(
        "SELECT rule_id, target_ext, max_size_mb FROM search_strategies"
    ).fetchone()
    blacklist = connection.execute(
        "SELECT rule_id, path_pattern, match_type FROM blacklists"
    ).fetchone()

    assert strategy["rule_id"] == "rule_test_docs"
    assert json.loads(strategy["target_ext"]) == [".md", ".txt"]
    assert strategy["max_size_mb"] == 50
    assert blacklist["rule_id"] == "blacklist_test_build"
    assert blacklist["match_type"] == "path_prefix"


def test_default_seed_configuration_can_initialize_database(tmp_path):
    seed = load_seed_config(PROJECT_ROOT / "config" / "local_seed_rules.json")
    connection = initialize_local_database(tmp_path / "default_seed.sqlite", seed)

    strategy_count = connection.execute("SELECT COUNT(*) FROM search_strategies").fetchone()[0]
    blacklist_count = connection.execute("SELECT COUNT(*) FROM blacklists").fetchone()[0]

    assert strategy_count == 2
    assert blacklist_count == 4
