"""Bundle nxuskit package into a single Python file.

This script combines all nxuskit source files into a single nxuskit_bundle.py file
that can be used directly without installation. It handles:
- Removing internal imports between nxuskit modules
- Preserving external imports (requests, os, json, typing, etc.)
- Maintaining proper dependency ordering
- Creating a valid single-file Python module
"""

import re
from pathlib import Path
from typing import List, Set, Tuple

# Files to skip in bundling
SKIP_FILES = {
    "__pycache__",
    "*.pyc",
    "__init__.py",  # Will be merged separately
    "bundle_single_file.py",  # Don't include this script
}

# Order of modules - dependencies must come first
MODULE_ORDER = [
    "errors.py",
    "types.py",
    "message.py",
    "providers/base.py",
    "providers/openai_compatible.py",
    "providers/claude.py",
    "providers/openai.py",
    "providers/ollama.py",
    "providers/groq.py",
    "providers/xai.py",
    "providers/mistral.py",
    "providers/fireworks.py",
    "providers/together.py",
    "providers/openrouter.py",
    "providers/perplexity.py",
    "providers/lmstudio.py",
    "providers/factory.py",
    "vision_helpers.py",
    "vision_utilities.py",
]


def get_imports(file_content: str) -> Tuple[Set[str], List[str]]:
    """Extract internal nxuskit imports and external imports from file."""
    internal_imports = set()
    external_imports = []

    lines = file_content.split("\n")
    for line in lines:
        # Match: from nxuskit.xxx import yyy
        match = re.match(r"from nxuskit\.(\S+)\s+import\s+(.+)", line)
        if match:
            module = match.group(1)
            # imports = match.group(2)  # Not used, but kept for reference
            internal_imports.add(module)
            continue

        # Match: import nxuskit.xxx
        match = re.match(r"import nxuskit\.(\S+)", line)
        if match:
            module = match.group(1)
            internal_imports.add(module)
            continue

        # Collect external imports. Relative imports belong to package-only
        # optional modules and cannot appear in the single-file bundle header.
        if line.startswith(("from .", "import .")):
            continue
        if line.startswith(("from ", "import ")) and "nxuskit" not in line:
            if line.strip() and not line.strip().startswith("#"):
                external_imports.append(line)

    return internal_imports, external_imports


def remove_nxuskit_imports(file_content: str) -> str:
    """Remove all nxuskit internal imports from file content."""
    lines = []
    skip_import = False

    for line in file_content.split("\n"):
        # Skip nxuskit imports - start skipping when we see one
        if line.startswith("from __future__ import "):
            continue

        if (
            line.startswith("from nxuskit.")
            or line.startswith("import nxuskit.")
            or line.startswith("from .")
            or line.startswith("import .")
        ):
            skip_import = True
            # Check if import ends on this line (no open paren or has closing paren)
            if "(" not in line or ")" in line:
                skip_import = False  # Single-line import
            continue

        # If we're skipping a multi-line import, skip until we see the closing paren
        if skip_import:
            if ")" in line:
                skip_import = False  # End of multi-line import
            continue

        lines.append(line)

    # Clean up multiple consecutive blank lines
    result = "\n".join(lines)
    result = re.sub(r"\n\n\n+", "\n\n", result)
    return result


def read_and_process_file(file_path: Path) -> str:
    """Read file and remove nxuskit imports."""
    with open(file_path, "r") as f:
        content = f.read()

    # Remove nxuskit imports
    content = remove_nxuskit_imports(content)

    # Remove module docstring if it's just """"Module description.""""
    # But keep important docstrings in classes and functions
    content = re.sub(r'^"""[^"]*"""', "", content)

    return content


def collect_all_imports(src_dir: Path) -> Set[str]:
    """Collect all external imports used across all modules."""
    all_imports = set()

    for py_file in src_dir.rglob("*.py"):
        if py_file.name.startswith("test_") or py_file.name in SKIP_FILES:
            continue

        with open(py_file, "r") as f:
            content = f.read()

        for line in content.split("\n"):
            if line.startswith(("from .", "import .")):
                continue
            if line.startswith(("from ", "import ")) and "nxuskit" not in line:
                if line.strip() and not line.strip().startswith("#"):
                    all_imports.add(line)

    return all_imports


def generate_bundle(src_dir: Path, output_path: Path) -> None:
    """Generate single-file bundle."""
    print(f"Bundling nxuskit from {src_dir} to {output_path}...")

    # Collect all external imports
    all_imports = collect_all_imports(src_dir)
    imports_section = "\n".join(sorted(all_imports))

    # Remove duplicates
    imports_lines = set(imports_section.split("\n"))
    imports_section = "\n".join(sorted(imports_lines))

    # Header
    header = '''"""nxusKit - Single-file bundled Python LLM library.

This is a bundled version of nxusKit containing all source code in a single file.
It provides access to multiple LLM providers (Claude, OpenAI, Ollama, Groq, xAI Grok, Mistral,
Fireworks, Together, OpenRouter, Perplexity, LM Studio) with a unified interface.

Usage:
    from nxuskit import Provider, Message

    provider = Provider.claude(api_key="your-key")
    response = provider.chat([Message.user("Hello")])
    print(response.content)
"""

'''

    # Collect all module content in order
    modules_content = []

    for module_file in MODULE_ORDER:
        file_path = src_dir / module_file

        if not file_path.exists():
            print(f"Warning: {module_file} not found, skipping...")
            continue

        print(f"  Including {module_file}...")
        content = read_and_process_file(file_path)

        # Add separator comment
        modules_content.append(
            "\n\n# ============================================================================\n"
        )
        modules_content.append(f"# {module_file}\n")
        modules_content.append(
            "# ============================================================================\n\n"
        )
        modules_content.append(content)

    # Add __init__.py exports at the end
    init_file = src_dir / "__init__.py"
    if init_file.exists():
        print("  Including __init__.py exports...")
        with open(init_file, "r") as f:
            init_content = f.read()

        # Extract __all__ and create exports
        match = re.search(r"__all__\s*=\s*\[(.*?)\]", init_content, re.DOTALL)
        if match:
            exports_section = f"\n# Public API exports\n{match.group(0)}\n"
            modules_content.append(exports_section)

    # Combine everything
    full_content = header + imports_section + "\n\n" + "".join(modules_content)

    # Write to output file
    with open(output_path, "w") as f:
        f.write(full_content)

    # Report statistics
    lines = full_content.split("\n")
    print("\n✓ Bundle created successfully!")
    print(f"  Location: {output_path}")
    print(f"  Size: {len(full_content):,} bytes ({len(full_content) / 1024:.1f} KB)")
    print(f"  Lines: {len(lines):,}")


def main():
    """Main entry point."""
    script_dir = Path(__file__).parent
    project_root = script_dir.parent
    src_dir = project_root / "src" / "nxuskit"
    output_path = project_root / "nxuskit_bundle.py"

    if not src_dir.exists():
        print(f"Error: Source directory not found: {src_dir}")
        return 1

    generate_bundle(src_dir, output_path)
    return 0


if __name__ == "__main__":
    exit(main())
