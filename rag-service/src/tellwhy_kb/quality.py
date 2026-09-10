from __future__ import annotations

import unicodedata


def comparison_text(text: str) -> str:
    """Evaluation only: ignore whitespace and punctuation, retain digits/math symbols."""
    return "".join(c for c in text if not c.isspace() and not unicodedata.category(c).startswith("P"))


def snippet_distance(reference: str, page_text: str) -> dict:
    """Semi-global Levenshtein: score a fixed gold snippet against its best page substring.

    Free page prefix/suffix, but all reference characters are charged. This measures
    sampled prose only; it does not measure omission elsewhere on the page.
    """
    ref, hyp = comparison_text(reference), comparison_text(page_text)
    if not ref:
        raise ValueError("Empty reference")
    previous = [0] * (len(hyp) + 1)
    for i, char in enumerate(ref, 1):
        current = [i]
        for j, other in enumerate(hyp, 1):
            current.append(min(previous[j] + 1, current[j - 1] + 1, previous[j - 1] + (char != other)))
        previous = current
    errors = min(previous)
    return {"errors": errors, "reference_characters": len(ref), "cer": errors / len(ref)}
