"""Generated numeric and text invariants for Python support tools."""

from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TYPE_CHECKING

import pytest

from tools import hypothesmith_corpus as corpus
from tools import performance_shapes as shapes

if TYPE_CHECKING:
    import hypothesis
    from hypothesis import strategies as st
else:
    hypothesis = pytest.importorskip("hypothesis")
    st = pytest.importorskip("hypothesis.strategies")


@hypothesis.given(rss_kib=st.integers(min_value=1, max_value=2**63 - 1))
@hypothesis.example(rss_kib=12)
def test_observed_rss_converts_positive_kib_to_bytes(rss_kib: int) -> None:
    observation: shapes.ResourceObservation = {
        "reason": "child_exit",
        "peak_rss_kib": rss_kib,
    }

    result = shapes.observed_rss(observation)

    assert result == rss_kib * 1024


@hypothesis.given(size=st.one_of(st.integers(max_value=0), st.integers(min_value=129)))
@hypothesis.example(size=0)
@hypothesis.example(size=-1)
@hypothesis.example(size=129)
@hypothesis.settings(deadline=None)
def test_shape_sizes_outside_bounds_fail_before_writing(size: int) -> None:
    with TemporaryDirectory() as temporary:
        root = Path(temporary) / "project"

        with pytest.raises(ValueError, match="unknown shape or out-of-bounds size"):
            shapes.make_fixture("ast", size, root)

        assert not root.exists()


@hypothesis.given(source=st.text(alphabet=" \n\t\r\f\v", max_size=64))
@hypothesis.example(source="")
@hypothesis.example(source=" \n\t")
@hypothesis.settings(deadline=None)
def test_blank_sources_never_write_corpus_files(source: str) -> None:
    with TemporaryDirectory() as temporary:
        output = Path(temporary)

        result = corpus.store_source(source, output, max_bytes=4096)

        assert result == "empty"
        assert list(output.iterdir()) == []
