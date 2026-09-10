from reportlab.pdfgen import canvas

from tellwhy_kb.ingest.probe import inspect_pdf
from tellwhy_kb.jobs import initialize_job, JobStore
from tellwhy_kb.pipeline import extract_pages
from tellwhy_kb.schemas import IngestConfig


def test_real_pdf_native_workers_resume_and_page_failures(tmp_path):
    pdf = tmp_path / "source.pdf"
    doc = canvas.Canvas(str(pdf))
    for text in ["A computer contains hardware and software.", "Memory stores instructions and data.", ""]:
        doc.drawString(50, 700, text)
        doc.showPage()
    doc.save()
    folder, _ = initialize_job(
        tmp_path,
        "test",
        inspect_pdf(pdf),
        IngestConfig(ocr_mode="native", native_text_trusted=True),
        {},
        runtime={},
    )
    counts = extract_pages(folder, tmp_path, workers=2)
    assert counts == {"success": 2, "failed": 1}
    assert extract_pages(folder, tmp_path, pages=[1, 2], workers=2) == counts
    with JobStore(folder) as store:
        assert store.cached(1).blocks[0].bbox.y0 > 0
        assert store.db.execute("SELECT attempts FROM pages WHERE page=1").fetchone()[0] == 1
        assert store.cached(3) is None
