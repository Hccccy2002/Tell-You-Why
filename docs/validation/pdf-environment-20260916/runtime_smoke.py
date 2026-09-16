import importlib.metadata as md
import json
import os
import platform
import sys
from pathlib import Path

ROOT = Path(r"D:\Tell-You-Why")
OUT = ROOT / "tmp/pdf-environment-20260916"
OUT.mkdir(parents=True, exist_ok=True)
os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK"] = "True"
report = {"python": sys.version, "executable": sys.executable, "platform": platform.platform(), "checks": {}}

def save():
    (OUT / "runtime-smoke.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")

def main():
    import numpy as np
    from reportlab.pdfgen import canvas
    from tellwhy_kb.models import verify_models
    from tellwhy_kb.ingest.probe import inspect_pdf
    from tellwhy_kb.ingest.extract import PdfSource
    from tellwhy_kb.ingest.ocr import OcrEngine
    from tellwhy_kb.schemas import IngestConfig
    from tellwhy_kb.indexing.embedding import Encoder

    locked = {}
    for line in (ROOT / "rag-service/requirements.lock.txt").read_text(encoding="utf-8").splitlines():
        if line.strip() and not line.startswith("#"):
            name, wanted = line.strip().split("==")
            actual = md.version(name)
            assert actual == wanted, (name, wanted, actual)
            locked[name] = actual
    report["checks"]["locked_dependencies"] = {"passed": True, "count": len(locked)}
    report["packages"] = locked
    models = ROOT / "data/models"
    verified = verify_models(models)
    report["checks"]["base_models"] = {"passed": True, "models": list(verified)}
    save()

    pdf = OUT / "environment-smoke.pdf"
    doc = canvas.Canvas(str(pdf), pagesize=(595, 842))
    doc.setTitle("Isolated PDF environment smoke test")
    doc.setFont("Helvetica-Bold", 20)
    doc.drawString(50, 770, "Computer hardware")
    doc.setFont("Helvetica", 14)
    for i, line in enumerate([
        "The CPU processes instructions and controls operations.",
        "Memory stores instructions and data for the computer.",
        "A system bus connects the processor, memory and devices.",
        "This is fictional test content for local environment validation."
    ]):
        doc.drawString(50, 710 - i * 28, line)
    doc.save()
    probe = inspect_pdf(pdf)
    assert probe["pages"] == 1
    with PdfSource(pdf) as source:
        native = source.read(1)
        assert "CPU" in native["text"]
        with source.render(1, 144) as img:
            img.save(OUT / "environment-smoke.png")
            report["checks"]["pdf_read_render"] = {"passed": True, "pages": 1, "pixels": list(img.size)}
        engine = OcrEngine(models, IngestConfig(dpi=160, cpu_threads=2))
        result = engine.process(source, 1, OUT / "crops")
        text = "\n".join(b.text for b in result.blocks)
        assert "CPU" in text and "Memory" in text, text
        report["checks"]["ocr_inference"] = {"passed": True, "page_status": result.status, "blocks": len(result.blocks), "seconds": result.elapsed_seconds, "recognized_text": text}
    save()
    encoder = Encoder(models, threads=2)
    vectors = encoder.encode(["中央处理器由运算器和控制器组成。", "内存用于保存指令和数据。"])
    assert vectors.shape == (2, 512) and np.isfinite(vectors).all()
    report["checks"]["embedding_inference"] = {"passed": True, "shape": list(vectors.shape), "norms": np.linalg.norm(vectors, axis=1).tolist()}
    save()
    print(json.dumps(report["checks"], ensure_ascii=False, indent=2))

if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        report["error"] = repr(error)
        save()
        raise
