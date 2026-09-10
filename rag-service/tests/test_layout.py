from tellwhy_kb.ingest.ocr import assign_lines, reading_order


def region(label, box):
    return {"label": label, "coordinate": box}


def test_two_columns_follow_heading_in_reading_order():
    regions = [
        region("text", [550, 180, 950, 280]),
        region("text", [50, 180, 450, 280]),
        region("doc_title", [50, 0, 950, 30]),
        region("text", [550, 70, 950, 170]),
        region("text", [50, 70, 450, 170]),
    ]
    result = reading_order(regions, 1000)
    assert [r["coordinate"][:2] for r in result] == [[50, 0], [50, 70], [50, 180], [550, 70], [550, 180]]


def test_nested_formula_wins_and_unknown_text_is_preserved():
    regions = [region("text", [0, 0, 100, 100]), region("formula", [10, 10, 40, 30])]
    lines = [
        {"text": "x=2", "box": [12, 12, 30, 22], "confidence": 0.99},
        {"text": "outside", "box": [120, 120, 150, 150], "confidence": 0.99},
    ]
    result = assign_lines(regions, lines)
    assert result[0]["lines"] == []
    assert result[1]["lines"][0]["text"] == "x=2"
    assert result[2]["label"] == "unassigned"
