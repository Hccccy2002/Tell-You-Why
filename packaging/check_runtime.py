"""Offline inference smoke test; writes only to the explicit output directory."""

import ctypes
import argparse
import json
import os
from pathlib import Path
import platform
import sys

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "rag-service/src"))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    os.environ.update(
        HF_HUB_OFFLINE="1",
        TRANSFORMERS_OFFLINE="1",
        PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK="True",
        TELLWHY_MODEL_CACHE=str(out / "cache"),
    )
    result = {
        "python": sys.version,
        "executable": sys.executable,
        "platform": platform.platform(),
        "checks": {},
    }
    try:
        from tellwhy_kb.models import verify_models
        from tellwhy_kb.indexing.reranker import verify, Reranker
        from tellwhy_kb.ingest.probe import inspect_pdf
        from tellwhy_kb.ingest.extract import PdfSource
        from tellwhy_kb.ingest.ocr import OcrEngine
        from tellwhy_kb.schemas import IngestConfig
        from tellwhy_kb.indexing.embedding import Encoder
        from reportlab.pdfgen import canvas
        import numpy as np

        models = ROOT / "models"
        verify_models(models)
        verify(models)
        result["checks"]["model_hashes"] = True
        result["checks"]["windows_code_page"] = ctypes.windll.kernel32.GetACP()
        assert result["checks"]["windows_code_page"] == 65001
        pdf = out / "synthetic-test.pdf"
        c = canvas.Canvas(str(pdf), pagesize=(595, 842))
        c.setFont("Helvetica", 16)
        c.drawString(40, 750, "The CPU processes instructions.")
        c.drawString(40, 710, "Memory stores instructions and data.")
        c.save()
        assert inspect_pdf(pdf)["pages"] == 1
        with PdfSource(pdf) as source:
            assert "CPU" in source.read(1)["text"]
            with source.render(1, 144) as img:
                result["checks"]["pdf_render"] = list(img.size)
            engine = OcrEngine(models, IngestConfig(dpi=160, cpu_threads=2))
            page = engine.process(source, 1, out / "crops")
            text = "\n".join(b.text for b in page.blocks)
            assert "CPU" in text and "Memory" in text, text
            result["checks"]["ocr"] = {
                "passed": True,
                "page_status": page.status,
                "blocks": len(page.blocks),
                "text": text,
            }
        vectors = Encoder(models, threads=2).encode(
            ["中央处理器执行指令。", "内存保存数据。"]
        )
        assert vectors.shape == (2, 512) and np.isfinite(vectors).all()
        result["checks"]["embedding"] = list(vectors.shape)
        scores = Reranker(models).score(
            "CPU的作用是什么？", ["CPU执行程序指令。", "香蕉是一种水果。"]
        )
        assert scores[0] > scores[1], scores
        result["checks"]["reranker"] = scores
        result["ok"] = True
    except Exception as error:
        result.update(ok=False, error=repr(error))
        raise
    finally:
        (out / "runtime-check.json").write_text(
            json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
