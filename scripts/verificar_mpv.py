#!/usr/bin/env python3
"""Verifica a DLL fixada; --baixar instala somente o arquivo cujo hash foi aprovado."""
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def check(path, expected):
    with path.open('rb') as stream:
        actual = hashlib.file_digest(stream, 'sha256').hexdigest()
    if actual != expected:
        raise ValueError(f'SHA-256 divergente: {path.name}; montagem interrompida')


def main():
    lock = json.loads((ROOT / 'mpv.lock.json').read_text())
    dll = ROOT / 'mpv/libmpv-2.dll'
    if '--baixar' in sys.argv or not dll.exists():
        with tempfile.TemporaryDirectory(prefix='saimo-mpv-') as temp:
            archive = Path(temp) / 'mpv.7z'
            with urllib.request.urlopen(lock['url'], timeout=120) as response, archive.open('wb') as out:
                shutil.copyfileobj(response, out)
            check(archive, lock['archive_sha256'])
            extracted = Path(temp) / 'libmpv-2.dll'
            with extracted.open('wb') as out:
                subprocess.run(['7z', 'e', '-so', str(archive), 'libmpv-2.dll'], stdout=out, check=True)
            check(extracted, lock['dll_sha256'])
            dll.parent.mkdir(exist_ok=True)
            if dll.exists():
                backup = ROOT / 'auditoria/mpv-anterior.dll'
                backup.parent.mkdir(exist_ok=True)
                if not backup.exists():
                    shutil.copy2(dll, backup)
            pending = dll.with_suffix('.tmp')
            shutil.copy2(extracted, pending)
            pending.replace(dll)
    check(dll, lock['dll_sha256'])
    print(f"mpv {lock['version']}: SHA-256 conferido")


if __name__ == '__main__':
    main()
