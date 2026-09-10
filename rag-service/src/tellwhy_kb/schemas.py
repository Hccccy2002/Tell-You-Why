from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", allow_inf_nan=False)


class Box(StrictModel):
    """Coordinates in displayed PDF points, top-left origin, after PDF page rotation."""

    x0: float = Field(ge=0)
    y0: float = Field(ge=0)
    x1: float = Field(ge=0)
    y1: float = Field(ge=0)

    @model_validator(mode="after")
    def ordered(self):
        if self.x1 < self.x0 or self.y1 < self.y0:
            raise ValueError("Inverted bounding box")
        return self


class Block(StrictModel):
    id: str
    page: int = Field(ge=1)
    kind: str
    text: str
    bbox: Box
    order: int = Field(ge=0)
    confidence: float | None = Field(default=None, ge=0, le=1)
    eligible: bool = False
    limitations: list[str] = Field(default_factory=list)
    asset: str | None = None
    lines: list[dict] = Field(default_factory=list)


class PageResult(StrictModel):
    number: int = Field(ge=1)
    width: float = Field(gt=0)
    height: float = Field(gt=0)
    status: Literal["success", "needs_review", "blank", "excluded", "failed"]
    method: Literal["native", "ocr", "none"]
    native_text: str = ""
    printed_label: str | None = None
    blocks: list[Block] = Field(default_factory=list)
    limitations: list[str] = Field(default_factory=list)
    elapsed_seconds: float = Field(default=0, ge=0)


class IngestConfig(StrictModel):
    dpi: int = Field(default=220, ge=100, le=400)
    ocr_mode: Literal["always", "auto", "native"] = "always"
    min_confidence: float = Field(default=0.85, ge=0, le=1)
    cpu_threads: int = Field(default=4, ge=1, le=16)
    chunk_tokens: int = Field(default=320, ge=64, le=440)
    overlap_tokens: int = Field(default=40, ge=0, le=128)
    embedding_batch_size: int = Field(default=16, ge=1, le=128)
    exclude_before_page: int = Field(default=1, ge=1)
    native_text_trusted: bool = False

    @model_validator(mode="after")
    def overlap_smaller(self):
        if self.overlap_tokens >= self.chunk_tokens:
            raise ValueError("Overlap must be smaller than chunk size")
        return self
