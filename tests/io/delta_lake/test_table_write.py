from __future__ import annotations

import contextlib
import datetime
import decimal
from pathlib import Path
from unittest.mock import patch

import pyarrow as pa
import pytest

import daft
from daft.io.object_store_options import io_config_to_storage_options
from daft.logical.schema import Schema
from tests.conftest import get_tests_daft_runner_name

PYARROW_LOWER_BOUND_SKIP = tuple(int(s) for s in pa.__version__.split(".") if s.isnumeric()) < (9, 0, 0)
pytestmark = pytest.mark.skipif(
    PYARROW_LOWER_BOUND_SKIP,
    reason="deltalake not supported on older versions of pyarrow",
)


class _FakeCommitProperties:
    def __init__(self, custom_metadata):
        self.custom_metadata = custom_metadata


@pytest.fixture
def custom_metadata():
    return {"CUSTOM_METADATA": "1"}


@pytest.fixture()
def commit_properties():
    @contextlib.contextmanager
    def _(deltalake):
        setattr(deltalake, "CommitProperties", _FakeCommitProperties)
        try:
            yield
        finally:
            delattr(deltalake, "CommitProperties")

    return _


def test_deltalake_write_basic(tmp_path, base_table):
    deltalake = pytest.importorskip("deltalake")
    path = tmp_path / "some_table"
    df = daft.from_arrow(base_table)
    result = df.write_deltalake(str(path))
    result = result.to_pydict()
    assert result["operation"] == ["ADD"]
    assert result["rows"] == [base_table.num_rows]

    read_delta = deltalake.DeltaTable(str(path))
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df.schema() == expected_schema
    assert read_delta.to_pyarrow_table() == base_table


def test_deltalake_multi_write_basic(tmp_path, base_table):
    deltalake = pytest.importorskip("deltalake")
    path = tmp_path / "some_table"
    df = daft.from_arrow(base_table)
    df.write_deltalake(str(path))

    result = df.write_deltalake(str(path))
    result = result.to_pydict()
    assert result["operation"] == ["ADD"]
    assert result["rows"] == [base_table.num_rows]

    read_delta = deltalake.DeltaTable(str(path))
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df.schema() == expected_schema
    assert read_delta.version() == 1
    assert read_delta.to_pyarrow_table() == pa.concat_tables([base_table, base_table])


def test_deltalake_write_cloud(base_table, cloud_paths):
    deltalake = pytest.importorskip("deltalake")
    path, io_config, _ = cloud_paths
    df = daft.from_arrow(base_table)
    result = df.write_deltalake(str(path), io_config=io_config)
    result = result.to_pydict()
    assert result["operation"] == ["ADD"]
    assert result["rows"] == [base_table.num_rows]
    storage_options = io_config_to_storage_options(io_config, path) if io_config is not None else None
    read_delta = deltalake.DeltaTable(str(path), storage_options=storage_options)
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df.schema() == expected_schema
    assert read_delta.to_pyarrow_table() == base_table


def test_deltalake_write_overwrite_basic(tmp_path):
    deltalake = pytest.importorskip("deltalake")
    path = tmp_path / "some_table"
    df1 = daft.from_pydict({"a": [1, 2]})
    df1.write_deltalake(str(path))

    df2 = daft.from_pydict({"a": [3, 4]})
    result = df2.write_deltalake(str(path), mode="overwrite")
    result = result.to_pydict()
    assert result["operation"] == ["ADD", "DELETE"]
    assert result["rows"] == [2, 2]

    read_delta = deltalake.DeltaTable(str(path))
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df2.schema() == expected_schema
    assert read_delta.to_pyarrow_table() == df2.to_arrow()


def test_deltalake_write_overwrite_cloud(cloud_paths):
    deltalake = pytest.importorskip("deltalake")
    path, io_config, _ = cloud_paths
    df1 = daft.from_pydict({"a": [1, 2]})
    df1.write_deltalake(str(path), io_config=io_config)

    df2 = daft.from_pydict({"a": [3, 4]})
    result = df2.write_deltalake(str(path), mode="overwrite", io_config=io_config)
    result = result.to_pydict()
    assert result["operation"] == ["ADD", "DELETE"]
    assert result["rows"] == [2, 2]

    storage_options = io_config_to_storage_options(io_config, path) if io_config is not None else None
    read_delta = deltalake.DeltaTable(str(path), storage_options=storage_options)
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df2.schema() == expected_schema
    assert read_delta.to_pyarrow_table() == df2.to_arrow()


@pytest.mark.skipif(
    get_tests_daft_runner_name() == "native",
    reason="Native executor does not support repartitioning",
)
def test_deltalake_write_overwrite_multi_partition(tmp_path):
    deltalake = pytest.importorskip("deltalake")
    path = tmp_path / "some_table"
    df1 = daft.from_pydict({"a": [1, 2, 3, 4]})
    df1 = df1.repartition(2)
    df1.write_deltalake(str(path))

    df2 = daft.from_pydict({"a": [5, 6, 7, 8]})
    df2 = df2.repartition(2)
    result = df2.write_deltalake(str(path), mode="overwrite")
    result = result.to_pydict()
    assert result["operation"] == ["ADD", "ADD", "DELETE", "DELETE"]

    read_delta = deltalake.DeltaTable(str(path))
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df2.schema() == expected_schema
    assert read_delta.to_pyarrow_table() == df2.to_arrow()


def test_deltalake_write_overwrite_schema(tmp_path):
    deltalake = pytest.importorskip("deltalake")
    path = tmp_path / "some_table"
    df1 = daft.from_pydict({"a": [1, 2]})
    df1.write_deltalake(str(path))

    df2 = daft.from_pydict({"b": [3, 4]})
    result = df2.write_deltalake(str(path), mode="overwrite", schema_mode="overwrite")
    result = result.to_pydict()
    assert result["operation"] == ["ADD", "DELETE"]

    read_delta = deltalake.DeltaTable(str(path))
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df2.schema() == expected_schema
    assert read_delta.to_pyarrow_table() == df2.to_arrow()


def test_deltalake_write_overwrite_error_schema(tmp_path):
    path = tmp_path / "some_table"
    df1 = daft.from_pydict({"a": [1, 2]})
    df1.write_deltalake(str(path), mode="overwrite")
    df2 = daft.from_pydict({"b": [3, 4]})
    with pytest.raises(ValueError):
        df2.write_deltalake(str(path), mode="overwrite")


def test_deltalake_write_error(tmp_path, base_table):
    path = tmp_path / "some_table"
    df = daft.from_arrow(base_table)
    df.write_deltalake(str(path), mode="error")
    with pytest.raises(AssertionError):
        df.write_deltalake(str(path), mode="error")


def test_deltalake_write_ignore(tmp_path):
    deltalake = pytest.importorskip("deltalake")
    path = tmp_path / "some_table"
    df1 = daft.from_pydict({"a": [1, 2]})
    df1.write_deltalake(str(path), mode="ignore")
    df2 = daft.from_pydict({"a": [3, 4]})
    result = df2.write_deltalake(str(path), mode="ignore")
    result = result.to_arrow()
    assert result.num_rows == 0

    read_delta = deltalake.DeltaTable(str(path))
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df1.schema() == expected_schema
    assert read_delta.to_pyarrow_table() == df1.to_arrow()


@pytest.mark.skipif(
    get_tests_daft_runner_name() == "native",
    reason="Native executor does not support repartitioning",
)
def test_deltalake_write_with_empty_partition(tmp_path, base_table):
    deltalake = pytest.importorskip("deltalake")
    path = tmp_path / "some_table"
    df = daft.from_arrow(base_table).into_partitions(4)
    result = df.write_deltalake(str(path))
    result = result.to_pydict()
    assert result["operation"] == ["ADD", "ADD", "ADD"]
    assert result["rows"] == [1, 1, 1]

    read_delta = deltalake.DeltaTable(str(path))
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df.schema() == expected_schema
    assert read_delta.to_pyarrow_table() == base_table


def check_equal_both_daft_and_delta_rs(df: daft.DataFrame, path: Path, sort_order: list[tuple[str, str]]):
    deltalake = pytest.importorskip("deltalake")

    arrow_df = df.to_arrow().sort_by(sort_order)

    read_daft = daft.read_deltalake(str(path))
    assert read_daft.schema() == df.schema()
    assert read_daft.to_arrow().sort_by(sort_order) == arrow_df

    read_delta = deltalake.DeltaTable(str(path))
    expected_schema = Schema.from_pyarrow_schema(pa.schema(read_delta.schema().to_arrow()))
    assert df.schema() == expected_schema
    assert read_delta.to_pyarrow_table().cast(expected_schema.to_pyarrow_schema()).sort_by(sort_order) == arrow_df


@pytest.mark.parametrize(
    "partition_cols,num_partitions",
    [
        (["int"], 3),
        (["float"], 3),
        (["str"], 3),
        pytest.param(["bin"], 3, marks=pytest.mark.xfail(reason="Binary partitioning is not yet supported")),
        (["bool"], 3),
        (["datetime"], 3),
        (["date"], 3),
        (["decimal"], 3),
        (["int", "float"], 4),
    ],
)
def test_deltalake_write_partitioned(tmp_path, partition_cols, num_partitions):
    path = tmp_path / "some_table"
    df = daft.from_pydict(
        {
            "int": [1, 1, 2, None],
            "float": [1.1, 2.2, 2.2, None],
            "str": ["foo", "foo", "bar", None],
            "bin": [b"foo", b"foo", b"bar", None],
            "bool": [True, True, False, None],
            "datetime": [
                datetime.datetime(2024, 2, 10),
                datetime.datetime(2024, 2, 10),
                datetime.datetime(2024, 2, 11),
                None,
            ],
            "date": [datetime.date(2024, 2, 10), datetime.date(2024, 2, 10), datetime.date(2024, 2, 11), None],
            "decimal": pa.array(
                [decimal.Decimal("1111.111"), decimal.Decimal("1111.111"), decimal.Decimal("2222.222"), None],
                type=pa.decimal128(7, 3),
            ),
        }
    )
    result = df.write_deltalake(str(path), partition_cols=partition_cols)
    result = result.to_pydict()
    assert len(result["operation"]) == num_partitions
    assert all(op == "ADD" for op in result["operation"])
    assert sum(result["rows"]) == len(df)

    sort_order = [("int", "ascending"), ("float", "ascending")]
    check_equal_both_daft_and_delta_rs(df, path, sort_order)


def test_deltalake_write_partitioned_empty(tmp_path):
    path = tmp_path / "some_table"

    df = daft.from_arrow(pa.schema([("int", pa.int64()), ("string", pa.string())]).empty_table())

    df.write_deltalake(str(path), partition_cols=["int"])

    check_equal_both_daft_and_delta_rs(df, path, [("int", "ascending")])


def test_deltalake_write_partitioned_some_empty(tmp_path):
    path = tmp_path / "some_table"

    df = daft.from_pydict({"int": [1, 2, 3, None], "string": ["foo", "foo", "bar", None]}).into_partitions(5)

    df.write_deltalake(str(path), partition_cols=["int"])

    check_equal_both_daft_and_delta_rs(df, path, [("int", "ascending")])


def test_deltalake_write_partitioned_existing_table(tmp_path):
    path = tmp_path / "some_table"

    df1 = daft.from_pydict({"int": [1], "string": ["foo"]})
    result = df1.write_deltalake(str(path), partition_cols=["int"])
    result = result.to_pydict()
    assert result["operation"] == ["ADD"]
    assert result["rows"] == [1]

    df2 = daft.from_pydict({"int": [1, 2], "string": ["bar", "bar"]})
    with pytest.raises(ValueError):
        df2.write_deltalake(str(path), partition_cols=["string"])

    result = df2.write_deltalake(str(path))
    result = result.to_pydict()
    assert result["operation"] == ["ADD", "ADD"]
    assert result["rows"] == [1, 1]

    check_equal_both_daft_and_delta_rs(df1.concat(df2), path, [("int", "ascending"), ("string", "ascending")])


def test_deltalake_write_roundtrip(tmp_path):
    path = tmp_path / "some_table"
    df = daft.from_pydict({"a": [1, 2, 3, 4]})
    df.write_deltalake(str(path))

    read_df = daft.read_deltalake(str(path))
    assert df.schema() == read_df.schema()
    assert df.to_arrow() == read_df.to_arrow()


def test_custom_metadata_added_for_new_table(tmp_path, custom_metadata):
    # import deltalake
    deltalake = pytest.importorskip("deltalake")

    path = tmp_path / "some_table"
    df = daft.from_pydict({"a": [1, 2, 3, 4]})
    df.write_deltalake(str(path), custom_metadata=custom_metadata)

    table = deltalake.DeltaTable(path)
    history = table.history(1)

    assert custom_metadata.items() <= history[0].items()


def test_custom_metadata_updated_for_existing_table(tmp_path, custom_metadata):
    """Tests for deltalake version installed in the current environment (currently 0.19.2)."""
    # import deltalake
    deltalake = pytest.importorskip("deltalake")

    path = tmp_path / "some_table"
    df = daft.from_pydict({"a": [1, 2, 3, 4]})
    df.write_deltalake(str(path))

    df = daft.from_pydict({"a": [5, 6]})
    df.write_deltalake(str(path), custom_metadata=custom_metadata, mode="append")

    table = deltalake.DeltaTable(path)
    history = table.history(1)

    assert custom_metadata.items() <= history[0].items()


def test_custom_metadata_updated_for_existing_table_with_commit_properties(
    tmp_path, custom_metadata, commit_properties
):
    deltalake = pytest.importorskip("deltalake")
    from deltalake._internal import RawDeltaTable

    # write once to get into the table is not None path
    path = tmp_path / "some_table"
    df = daft.from_pydict({"a": [1, 2, 3, 4]})
    df.write_deltalake(str(path))

    # Add mocked CommitProperties class introduced in 0.20.0
    with commit_properties(deltalake), patch.object(RawDeltaTable, "create_write_transaction") as mock_method:
        df = daft.from_pydict({"a": [5, 6]})
        df.write_deltalake(str(path), custom_metadata=custom_metadata, mode="append")

        mock_method.assert_called_once()
        (_, _, _, _, _, custom_metadata_arg) = mock_method.call_args[0]

        assert isinstance(custom_metadata_arg, _FakeCommitProperties)
        assert custom_metadata_arg.custom_metadata == custom_metadata
