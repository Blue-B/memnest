#!/usr/bin/env python3
"""Disposable installer file-effects test; sudo/systemctl and HTTP are test doubles."""
import json
import shutil
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class Probe(BaseHTTPRequestHandler):
    paths = []
    marker = ''

    def do_GET(self):
        self.respond({"status": "ok"})

    def do_POST(self):
        try:
            body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        except (ValueError, TypeError):
            self.send_error(400)
            return
        self.paths.append(self.path)
        if self.path == '/add':
            Probe.marker = body['text']
        self.respond({'id': 'fixture', 'results': [{'document': Probe.marker}]})

    def respond(self, value):
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(value).encode())

    def log_message(self, format, *args):
        pass


with tempfile.TemporaryDirectory(prefix='memnest-install-test-') as directory:
    scratch = Path(directory)
    core = scratch / 'core'
    for name in ('scripts', 'packaging'):
        shutil.copytree(ROOT / name, core / name)
    # Map system paths in this PRIVATE COPY, never run sudo or systemctl on the host.
    for file in [*core.glob('scripts/*.sh'), *core.glob('packaging/systemd/*.service')]:
        text = file.read_text()
        for prefix in ('/etc/systemd/system', '/usr/local', '/var/lib'):
            text = text.replace(prefix, str(scratch / 'system') + prefix)
        file.write_text(text)
    stub = scratch / 'bin'
    stub.mkdir()
    mutations = scratch / 'commands.log'
    for name, text in {
        'systemctl': f'#!/bin/sh\necho "$*" >> "{mutations}"\n',
        'systemd-analyze': f'#!/bin/sh\ncase "$1" in --user) printf "%s\\n" "{scratch}/home/.config/systemd/user" "{scratch}/global-user";; *) printf "%s\\n" "{scratch}/system/etc/systemd/system" "{scratch}/runtime-system" "{scratch}/vendor-system";; esac\n',
        'sudo': f'#!/bin/sh\nfor arg do case "$arg" in /*) case "$arg" in "{scratch}"/*) ;; *) exit 99;; esac;; esac; done\nexec "$@"\n',
        'memnest': '#!/bin/sh\necho fixture-binary\n',
    }.items():
        target = stub / name
        target.write_text(text)
        target.chmod(0o700)
    server = ThreadingHTTPServer(('127.0.0.1', 0), Probe)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = str(server.server_port)
    home = scratch / 'home'
    home.mkdir()
    env = {'HOME': str(home), 'PATH': f'{stub}:/usr/bin:/bin', 'TMPDIR': str(scratch)}
    script = core / 'scripts/install-linux.sh'

    def run(mode='user', extra=None, ok=True, setup=False):
        result = subprocess.run(['bash', str(core / 'scripts/setup.sh' if setup else script),
                                 '--' + mode, '--bin', str(stub / 'memnest')],
                                env=dict(env, **(extra or {})), text=True, capture_output=True, timeout=30)
        assert (result.returncode == 0) == ok, result.stdout + result.stderr
        return result.stdout + result.stderr

    def customized(mode):
        if mode == 'system':
            folder = scratch / 'system/etc/systemd/system'
            template = 'memnest.service'
        else:
            folder = home / '.config/systemd/user'
            template = 'memnest-user.service'
        folder.mkdir(parents=True, exist_ok=True)
        unit = folder / 'memnest.service'
        text = (core / 'packaging/systemd' / template).read_text()
        text = text.replace('MEMNEST_PORT=3111', f'MEMNEST_PORT={port}')
        text = text.replace('Environment=RUST_LOG=info', 'Environment=RUST_LOG=debug\nEnvironment="CUSTOM=words with spaces" "EXTRA=keep"\nEnvironment=CUSTOM=last')
        text = text.replace('RestartSec=5', 'RestartSec=17\nUMask=0077')
        unit.write_text(text)
        unit.chmod(0o600)
        if mode == 'user':
            watch = folder / 'memnest-watch.service'
            watch.write_text((core / 'packaging/systemd/memnest-watch-user.service').read_text().replace('MEMNEST_PORT=3111', f'MEMNEST_PORT={port}'))
        return unit

    try:
        # Fresh install, then byte-exact user/system upgrades without port env.
        run(extra={'MEMNEST_PORT': port})
        for mode in ('user', 'system'):
            unit = customized(mode)
            original = unit.read_bytes()
            run(mode)
            assert unit.read_bytes() == original
            assert unit.stat().st_mode & 0o777 == 0o600
            # Explicit endpoint change (localhost resolves to the same test service).
            output = run(mode, {'MEMNEST_HOST': 'localhost'})
            assert 'Service backup:' in output
            backup = next(unit.parent.glob('memnest.service.backup.*'))
            assert backup.read_bytes() == original
            assert backup.stat().st_mode & 0o777 == 0o600
            assert unit.read_bytes() == original.replace(b'MEMNEST_HOST=127.0.0.1', b'MEMNEST_HOST=localhost')
            # Exercise a changed port as well, retaining the customized settings.
            second = ThreadingHTTPServer(('127.0.0.1', 0), Probe)
            threading.Thread(target=second.serve_forever, daemon=True).start()
            try:
                run(mode, {'MEMNEST_PORT': str(second.server_port)})
                assert f'MEMNEST_PORT={second.server_port}' in unit.read_text()
                assert 'RUST_LOG=debug' in unit.read_text() and 'UMask=0077' in unit.read_text()
            finally:
                second.shutdown()
                second.server_close()
            # Restore from the actual backup and re-run without an override.
            shutil.copyfile(backup, unit)
            if mode == 'user':
                watch = unit.with_name('memnest-watch.service')
                watch.write_text(watch.read_text().replace(f'MEMNEST_PORT={second.server_port}', f'MEMNEST_PORT={port}'))
            run(mode)
            assert unit.read_bytes() == original
        # The top-level setup must configure/probe the preserved port too.
        (home / '.claude').mkdir()
        (home / '.claude.json').write_text('{}')
        run(setup=True)
        client = json.loads((home / '.claude.json').read_text())
        assert client['mcpServers']['memnest']['url'] == f'http://127.0.0.1:{port}/mcp'
        assert Probe.paths[-3:] == ['/add', '/search', '/delete']
        # Ambiguous environments and drop-ins fail BEFORE binary or service writes.
        unit = home / '.config/systemd/user/memnest.service'
        original = unit.read_bytes()
        installed = home / '.local/bin/memnest'
        installed.write_text('keep this installed binary')
        before = mutations.read_bytes()
        for suffix in ('EnvironmentFile=/custom/env\n', 'Environment="MEMNEST_PORT=9999"\n',
                       ' ExecStart=\n ExecStart=/custom/command --port 9999\n',
                       'Environment=\n', 'Environment=""\n', "Environment=''\n",
                       'Environment=CUSTOM=keep \\\n MEMNEST_PORT=9999\n'):
            altered = original.replace(b'Restart=on-failure', suffix.encode() + b'Restart=on-failure')
            unit.write_bytes(altered)
            run(ok=False)
            assert unit.read_bytes() == altered
            assert installed.read_text() == 'keep this installed binary'
            assert mutations.read_bytes() == before
        unit.write_bytes(original)
        dropins = Path(str(unit) + '.d')
        dropins.mkdir()
        (dropins / 'override.conf').write_text('[Service]\nEnvironment=MEMNEST_PORT=9999\n')
        run(ok=False)
        assert mutations.read_bytes() == before
        (dropins / 'override.conf').unlink()
        for mode, folder in [('user', scratch / 'global-user'), ('system', scratch / 'runtime-system'), ('system', scratch / 'vendor-system')]:
            folder.mkdir()
            override = folder / 'memnest.service.d/override.conf'
            override.parent.mkdir()
            override.write_text('[Service]\nEnvironment=MEMNEST_PORT=9999\n')
            run(mode, ok=False)
            assert mutations.read_bytes() == before
            override.unlink()
            fragment = folder / 'memnest.service'
            fragment.write_text('alternative unit')
            run(mode, ok=False)
            assert mutations.read_bytes() == before
            fragment.unlink()
        print('PASS: fresh/user/system file effects, exact settings and port preservation, explicit updates, private backups/restore, setup URL, fail-before-mutation. Service manager and HTTP are test doubles.')
    finally:
        server.shutdown()
        server.server_close()
