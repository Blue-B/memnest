#!/usr/bin/env python3
"""Test the downloader against local release layouts; no network or real service."""
import hashlib
import subprocess
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='memnest-download-test-') as directory:
    scratch = Path(directory)
    home = scratch / 'home'
    home.mkdir()
    bin_dir = scratch / 'bin'
    bin_dir.mkdir()
    payload = scratch / 'payload'
    (payload / 'scripts').mkdir(parents=True)
    (payload / 'memnest').write_text('fixture binary')
    marker = scratch / 'installed'
    archive = scratch / 'release.tar.gz'
    checksum = scratch / 'release.sha256'
    curl = bin_dir / 'curl'
    curl.write_text('#!/bin/sh\ncase "$2" in *.sha256) cp "' + str(checksum) + '" "$4";; *) cp "' + str(archive) + '" "$4";; esac\n')
    curl.chmod(0o700)
    analyze = bin_dir / 'systemd-analyze'
    analyze.write_text(f'#!/bin/sh\ncase "$1" in --user) printf "%s\\n" "{home}/.config/systemd/user" "{scratch}/global-user";; *) printf "%s\\n" "{scratch}/system" "{scratch}/runtime-system" "{scratch}/vendor-system";; esac\n')
    analyze.chmod(0o700)
    installer = scratch / 'install.sh'
    installer.write_text((ROOT / 'install.sh').read_text().replace('/etc/systemd/system', str(scratch / 'system')))
    env = {'HOME': str(home), 'PATH': f'{bin_dir}:/usr/bin:/bin', 'TMPDIR': str(scratch), 'VERSION': 'v0.2.1'}

    def pack(name):
        script = payload / 'scripts' / name
        script.write_text(f'#!/bin/sh\nprintf "%s\\n" "$@" > "{marker}"\n')
        script.chmod(0o700)
        with tarfile.open(archive, 'w:gz') as bundle:
            bundle.add(payload / 'scripts', arcname='scripts')
            bundle.add(payload / 'memnest', arcname='memnest')
        checksum.write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  archive.tar.gz\n')

    def run(*args, ok=True):
        result = subprocess.run(['bash', str(installer), *args], env=env, capture_output=True, text=True, timeout=20)
        assert (result.returncode == 0) == ok, result.stdout + result.stderr
        return result.stdout + result.stderr

    pack('install-linux.sh')
    assert 'Legacy release' in run('--user')
    assert '--user' in marker.read_text()
    marker.unlink()
    run('--autocontext', ok=False)
    assert not marker.exists()
    for folder in (home / '.config/systemd/user', scratch / 'system'):
        folder.mkdir(parents=True)
        unit = folder / 'memnest.service'
        unit.write_text('preserve this customized service')
        mode = '--user' if folder != scratch / 'system' else '--system'
        assert 'cannot preserve' in run(mode, ok=False)
        assert unit.read_text() == 'preserve this customized service'
        assert not marker.exists()
        unit.unlink()
    for mode, folder in [('--user', scratch / 'global-user'), ('--system', scratch / 'runtime-system'), ('--system', scratch / 'vendor-system')]:
        folder.mkdir()
        unit = folder / 'memnest.service'
        unit.write_text('alternative unit must not be shadowed')
        run(mode, ok=False)
        assert not marker.exists()
        unit.unlink()
    # A future archive with setup receives the requested flags instead of the fallback.
    pack('setup.sh')
    run('--user', '--autocontext')
    assert '--autocontext' in marker.read_text()
    marker.unlink()
    checksum.write_text('0' * 64 + '  archive.tar.gz\n')
    assert 'checksum mismatch' in run('--user', ok=False)
    assert not marker.exists()
    print('PASS: legacy fresh install, existing user/system refusal, unsupported options, new setup forwarding, checksum failure; local archive fixtures only.')
