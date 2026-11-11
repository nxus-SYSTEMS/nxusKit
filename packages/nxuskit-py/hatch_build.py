"""Hatch build hook for shipping canonical nxusKit compliance files."""

from pathlib import Path
from typing import Any

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

COMPLIANCE_FILES = ("LICENSE", "NOTICE")


class CustomBuildHook(BuildHookInterface):
    """Select one complete compliance-file pair without mutating the source tree."""

    PLUGIN_NAME = "custom"

    def initialize(self, version: str, build_data: dict[str, Any]) -> None:
        project_root = Path(self.root).resolve()
        repository_root = project_root.parent.parent
        if (
            project_root.name == "nxuskit-py"
            and project_root.parent.name == "packages"
            and (repository_root / "Cargo.toml").is_file()
        ):
            compliance_root = repository_root
            location = "repository-root"
        else:
            compliance_root = project_root
            location = "project-local"

        sources = {name: (compliance_root / name).resolve() for name in COMPLIANCE_FILES}
        invalid = [
            name for name, path in sources.items() if not path.is_file() or path.stat().st_size == 0
        ]
        if invalid:
            names = ", ".join(invalid)
            raise RuntimeError(
                f"nxuskit-py requires non-empty {location} compliance files: {names}"
            )

        if self.target_name == "wheel":
            shared_data = build_data.setdefault("shared_data", {})
            for name, source in sources.items():
                shared_data[str(source)] = f"nxuskit-py/{name}"
        elif self.target_name == "sdist":
            force_include = build_data.setdefault("force_include", {})
            for source, destination in tuple(force_include.items()):
                if destination in COMPLIANCE_FILES:
                    del force_include[source]
            for name, source in sources.items():
                force_include[str(source)] = name
