"""Integration check against an unpacked/installed runtime; isolated synthetic data only."""
import argparse
import json
import os
from pathlib import Path
import sys


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--runtime", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    runtime = args.runtime.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    sys.path.insert(0, str(runtime / "rag-service/src"))
    os.environ.update(HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1",
                      PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK="True", TELLWHY_MODEL_CACHE=str(out / "cache"))
    from reportlab.pdfgen import canvas
    from tellwhy_kb.desktop import DesktopLibrary

    pdf = out / "synthetic-import.pdf"
    c = canvas.Canvas(str(pdf), pagesize=(595, 842))
    c.setFont("Helvetica-Bold", 20)
    c.drawString(40, 780, "Computer hardware")
    c.setFont("Helvetica", 14)
    for i, line in enumerate([
        "The CPU executes program instructions and performs calculations.",
        "The control unit coordinates the operations of the processor.",
        "Memory stores instructions and data needed by the CPU.",
        "A system bus connects the processor, memory and input devices.",
        "Input devices send data to the computer for processing.",
        "Output devices present the processing results to the user.",
    ]):
        c.drawString(40, 730-i*30, line)
    c.save()
    library = DesktopLibrary(out / "data", runtime / "models")
    report = {"runtime": str(runtime)}
    try:
        job = library.prepare(str(pdf), 1)
        library.launch_ready(job["kb"], job["job"])
        publication = library.run_import(job["kb"], job["job"])
        results = library.search(job["kb"], "CPU", mode="hybrid")
        assert results["results"], results
        repeat = library.prepare(str(pdf), 1)
        assert repeat["reused"] is True
        report.update(ok=True, job=job, publication=publication, search=results,
                      repeat_reused=True, catalog=library.catalog())
    except Exception as error:
        report.update(ok=False, error=repr(error))
        raise
    finally:
        (out / "import-check.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
        print(json.dumps({"ok":report.get("ok"),"output":str(out / "import-check.json")},ensure_ascii=False))


if __name__ == "__main__":
    main()
