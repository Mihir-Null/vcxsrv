"""Actual source delivery regression: Cargo must use extracted vendored inputs."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from unittest import mock
import zipfile

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / 'packaging/source_archive.py'
REGISTRY = Path.home() / '.cargo/registry/src/index.crates.io-1949cf8c6b5b557f'

def run(args, cwd, env=None):
    result = subprocess.run(args, cwd=cwd, env=env, text=True, capture_output=True)
    if result.returncode:
        raise AssertionError(result.stdout + result.stderr)
    return result.stdout

class SourceArchiveTests(unittest.TestCase):
    def module(self):
        spec = importlib.util.spec_from_file_location('source_archive', SCRIPT)
        module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
        return module

    def test_locked_cached_package_tampering_refused(self):
        parent = Path(tempfile.gettempdir()).resolve()
        with tempfile.TemporaryDirectory(prefix='vcxsrv-package-tamper-', dir=parent) as directory:
            base = Path(directory).resolve(); self.assertEqual(base.parent, parent)
            package = next(p for p in tomllib.loads((REPO / 'manager/Cargo.lock').read_text(encoding='utf-8'))['package'] if p['name'] == 'itoa')
            registry = base / 'registry/src/isolated'; cache = base / 'registry/cache/isolated'; cache.mkdir(parents=True)
            name = package['name'] + '-' + package['version'] + '.crate'
            data = (REGISTRY.parent.parent / 'cache' / REGISTRY.name / name).read_bytes()
            (cache / name).write_bytes(data[:-1] + bytes([data[-1] ^ 1]))
            with self.assertRaisesRegex(ValueError, 'Locked cached package checksum mismatch'):
                with self.module().owned_vendor(REPO, registry, [package]):
                    self.fail('Tampered published package accepted')

    def test_vendored_file_tampering_refused(self):
        module = self.module()
        packages = [p for p in tomllib.loads((REPO / 'manager/Cargo.lock').read_text(encoding='utf-8'))['package'] if 'source' in p]
        original_run = subprocess.run
        def vendor_then_tamper(args, **kwargs):
            result = original_run(args, **kwargs)
            file = Path(args[-1]) / 'itoa-1.0.17/src/lib.rs'
            file.write_bytes(file.read_bytes() + b'\n// injected staged source change\n')
            return result
        with mock.patch.object(module.subprocess, 'run', side_effect=vendor_then_tamper):
            with self.assertRaisesRegex(ValueError, 'Vendor source differs from locked published package'):
                with module.owned_vendor(REPO, REGISTRY, packages):
                    self.fail('Changed vendor source accepted')

    def test_extracted_archive_resolves_with_empty_cargo_home(self):
        parent = Path(tempfile.gettempdir()).resolve()
        with tempfile.TemporaryDirectory(prefix='vcxsrv-source-acceptance-', dir=parent) as directory:
            base = Path(directory).resolve()
            self.assertEqual(base.parent, parent)
            source = base / 'source'; source.mkdir()
            snapshot = base / 'snapshot'; snapshot.mkdir()
            files = run(['git', 'ls-files', 'manager'], REPO).splitlines()
            for relative in files:
                for root in (source, snapshot):
                    destination = root / relative; destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(REPO / relative, destination)
            run(['git', 'init', '-q'], source)
            run(['git', 'add', 'manager'], source)
            run(['git', '-c', 'user.name=Source acceptance', '-c', 'user.email=acceptance@invalid',
                 'commit', '-qm', 'Isolated source fixture'], source)
            revision = run(['git', 'rev-parse', 'HEAD'], source).strip()
            archive = base / 'source.zip'
            run(['python', str(SCRIPT), '--source', str(source), '--workspace', str(snapshot),
                 '--server-revision', revision, '--output', str(archive), '--registry', str(REGISTRY)], REPO)
            extracted = base / 'extracted'; extracted.mkdir()
            with zipfile.ZipFile(archive) as z:
                self.assertIsNone(z.testzip())
                for item in z.infolist():
                    self.assertNotIn('..', Path(item.filename).parts)
                    z.extract(item, extracted)
            manager = extracted / 'product/manager'
            env = os.environ.copy()
            env['CARGO_HOME'] = str(base / 'empty-cargo-home')
            env['CARGO_TARGET_DIR'] = str(base / 'target')
            metadata = json.loads(run(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1'], manager, env))
            locked = [item for item in tomllib.loads((manager / 'Cargo.lock').read_text(encoding='utf-8'))['package'] if 'source' in item]
            self.assertEqual(len(metadata['packages']), len(locked) + 1)
            for package in metadata['packages']:
                if package['source']:
                    self.assertTrue(Path(package['manifest_path']).resolve().is_relative_to(manager / 'vendor'))
            for package in locked:
                vendor = manager / 'vendor' / (package['name'] + '-' + package['version'])
                checksums = json.loads((vendor / '.cargo-checksum.json').read_text(encoding='utf-8'))
                self.assertEqual(checksums['package'], package['checksum'])
                for relative, expected in checksums['files'].items():
                    self.assertEqual(hashlib.sha256((vendor / relative).read_bytes()).hexdigest(), expected)
            self.assertEqual(len(locked), 13)

if __name__ == '__main__':
    unittest.main()
