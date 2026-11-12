"""Check XML2Forms output filenames without requiring the Oracle testbed."""
import importlib.util
from pathlib import Path
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / 'plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py'
spec = importlib.util.spec_from_file_location('reference_runner', SCRIPT)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)

class OutputNames(unittest.TestCase):
    def test_output_names(self):
        for xml, binary in [('orders_fmb.xml', 'orders.fmb'), ('new_form.xml', 'new_form.fmb'), ('ORDER_FMB.xml', 'ORDER.fmb')]:
            with self.subTest(xml=xml), TemporaryDirectory() as directory:
                root = Path(directory).resolve()
                source = root / xml
                source.write_text('<Module/>')
                (root / binary).write_bytes(b'converted')
                with patch.object(runner.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout='Module saved as')):
                    self.assertEqual(runner.convert_xml_to_fmb(source, root), root / binary)

    def test_missing_binary_is_an_error_even_on_zero_exit(self):
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            source = root / 'broken_fmb.xml'
            source.write_text('<Module/>')
            with patch.object(runner.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout='No output')):
                with self.assertRaisesRegex(RuntimeError, 'did not create'):
                    runner.convert_xml_to_fmb(source, root)

if __name__ == '__main__':
    unittest.main()
