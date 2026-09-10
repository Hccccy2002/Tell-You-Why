import numpy as np
import pytest

from tellwhy_kb.indexing.keyword import KeywordTokenizer
from tellwhy_kb.indexing.embedding import validate_matrix


def test_keyword_uses_consistent_chinese_terms_and_escapes_query_syntax():
    tokenizer = KeywordTokenizer(["地址总线", "Cache"])
    assert "地址总线" in tokenizer.tokens("地址总线决定寻址范围")
    assert "cache" in tokenizer.tokens("CACHE 缓存")
    query = tokenizer.match_query('地址总线 " OR * :')
    assert '"地址总线"' in query and "*" not in query and ":" not in query
    assert tokenizer.match_query("，。！") is None


def test_vector_shape_finiteness_and_normalization():
    good = np.zeros((2, 512), dtype=np.float32)
    good[:, 0] = 1
    validate_matrix(good, 2)
    for bad in [good.astype(np.float64), good[:1], good * np.nan, good * 2]:
        with pytest.raises(ValueError):
            validate_matrix(bad, 2)
