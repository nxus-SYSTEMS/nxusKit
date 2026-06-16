import importlib.metadata
import tomllib
from pathlib import Path

import nxuskit


def _pyproject_metadata() -> dict:
    root = Path(__file__).resolve().parents[1]
    with (root / "pyproject.toml").open("rb") as fh:
        return tomllib.load(fh)["project"]


def test_runtime_version_matches_package_metadata():
    metadata = _pyproject_metadata()
    assert metadata["version"] == "1.0.4"
    assert nxuskit.__version__ == metadata["version"]


def test_installed_metadata_when_available_matches_v104():
    try:
        distribution = importlib.metadata.distribution("nxuskit-py")
    except importlib.metadata.PackageNotFoundError:
        assert _pyproject_metadata()["version"] == "1.0.4"
        return

    project_root = Path(__file__).resolve().parents[1]
    dist_root = Path(distribution.locate_file("")).resolve()
    if not dist_root.is_relative_to(project_root):
        # A globally installed nxuskit-py can be present on developer machines;
        # this source-tree test validates the local package metadata instead.
        assert _pyproject_metadata()["version"] == "1.0.4"
        return

    assert distribution.version == "1.0.4"
