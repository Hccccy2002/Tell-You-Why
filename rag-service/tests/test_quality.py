from tellwhy_kb.quality import snippet_distance, comparison_text


def test_snippet_is_scored_with_missing_and_wrong_characters():
    assert snippet_distance("内存储器", "页眉内存堵器页脚")["errors"] == 1
    assert snippet_distance("内存储器", "页眉内存器页脚")["errors"] == 1
    assert snippet_distance("内存储器", "")["cer"] == 1
    assert snippet_distance("存储器", "页眉存储器页脚")["cer"] == 0


def test_math_is_not_normalized_to_plain_digits():
    assert comparison_text(" 2²⁰，2−1 ") == "2²⁰2−1"
