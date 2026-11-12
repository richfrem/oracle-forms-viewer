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

class CompileConnection(unittest.TestCase):
    def test_offline_unresolved_objects_explain_connected_compilation(self):
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            fmb = root / 'sales.fmb'
            fmb.write_bytes(b'test-only')
            output = SimpleNamespace(returncode=1, stdout="identifier 'OFW_ORDERS' must be declared")
            with patch.object(runner.subprocess, 'run', return_value=output):
                with self.assertRaisesRegex(RuntimeError, 'Select database-connected compilation'):
                    runner.compile_fmb_to_fmx(fmb, root, 'none', True)

class PreviewCompatibility(unittest.TestCase):
    def test_orders_copy_uses_oracle_schema_names_without_changing_source(self):
        import xml.etree.ElementTree as ET
        with TemporaryDirectory() as directory:
            source = Path(directory) / 'orders_fmb.xml'
            fixture = SCRIPT.parents[6] / 'samples/orders_fmb.xml'
            original = fixture.read_bytes()
            source.write_bytes(original)
            target = runner.prepare_oracle_preview_xml(source)
            self.assertNotEqual(source, target)
            self.assertEqual(source.read_bytes(), original)
            root = ET.parse(target).getroot()
            ns = {'f': 'http://xmlns.oracle.com/Forms'}
            self.assertEqual(root.get('version'), '141020000')
            parameter = root.find('.//f:ModuleParameter', ns)
            self.assertEqual(parameter.get('ParameterDataType'), 'Number')
            self.assertEqual(parameter.get('ParameterInitializeValue'), '0')
            self.assertIsNone(root.find('.//f:FutureUnknownElement', ns))
            self.assertEqual(root.find('.//f:RecordGroupColumn', ns).get('ColumnDataType'), 'Character')
            self.assertIsNotNone(root.find('.//f:AttachedLibrary', ns))
            self.assertEqual(root.find('.//f:Coordinate', ns).get('RealUnit'), 'Point')
            self.assertEqual(root.find('.//f:Canvas', ns).get('Width'), '800')

    def test_explicit_geometry_is_not_replaced_by_preview_defaults(self):
        import xml.etree.ElementTree as ET
        with TemporaryDirectory() as directory:
            source = Path(directory) / 'layout.xml'
            source.write_text('<Module version="12.2.1.19.0"><FormModule Name="F"><Coordinate RealUnit="Pixel"/><Canvas Name="C" Width="321" Height="123"/></FormModule></Module>')
            target = runner.prepare_oracle_preview_xml(source)
            root = ET.parse(target).getroot()
            self.assertEqual(root.find('.//Coordinate').get('RealUnit'), 'Pixel')
            self.assertEqual(root.find('.//Canvas').get('Width'), '321')
            self.assertEqual(root.find('.//Canvas').get('Height'), '123')

    def test_other_unknown_elements_are_preserved(self):
        with TemporaryDirectory() as directory:
            source = Path(directory) / 'other.xml'
            source.write_text('<Module version="12.2.1.19.0"><CustomBusinessNode Value="keep"/></Module>')
            target = runner.prepare_oracle_preview_xml(source)
            self.assertIn('CustomBusinessNode', target.read_text())

    def test_conflicting_aliases_fail_instead_of_discarding_values(self):
        with TemporaryDirectory() as directory:
            source = Path(directory) / 'conflict.xml'
            source.write_text('<Module><ModuleParameter DataType="Char" ParameterDataType="Number"/></Module>')
            with self.assertRaisesRegex(RuntimeError, 'Conflicting'):
                runner.prepare_oracle_preview_xml(source)

if __name__ == '__main__':
    unittest.main()
