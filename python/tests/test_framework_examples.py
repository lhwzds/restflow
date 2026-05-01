import importlib.util
import sys
import unittest
from pathlib import Path


EXAMPLES_ROOT = Path(__file__).resolve().parents[2] / "examples" / "frameworks"


class FrameworkExampleTests(unittest.TestCase):
    def test_framework_examples_import_without_framework_dependencies(self) -> None:
        for path in sorted(EXAMPLES_ROOT.glob("*.py")):
            module_name = f"restflow_example_{path.stem}"
            spec = importlib.util.spec_from_file_location(module_name, path)
            assert spec is not None
            assert spec.loader is not None
            module = importlib.util.module_from_spec(spec)
            sys.modules[module_name] = module
            try:
                spec.loader.exec_module(module)
            finally:
                sys.modules.pop(module_name, None)


if __name__ == "__main__":
    unittest.main()
