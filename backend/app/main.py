from __future__ import annotations

from fastapi import FastAPI

from app.api.nl2dsl import router as nl2dsl_router


def create_app() -> FastAPI:
    app = FastAPI(title="Maisou API")
    app.include_router(nl2dsl_router)
    return app


app = create_app()
