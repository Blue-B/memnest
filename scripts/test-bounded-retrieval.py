#!/usr/bin/env python3
"""Disposable real HTTP + MCP regression; requires a built daemon and cached embedder."""
import json
import os
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

root = Path(__file__).resolve().parents[1]
binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else root / 'core/target/debug/memnest').resolve())
cache = root / 'core/target/test-model-cache'
assert (cache / 'models--intfloat--multilingual-e5-base/refs/main').is_file(), 'cached model required'
with tempfile.TemporaryDirectory(prefix='memnest-bounded-') as data:
    Path(data, 'models').symlink_to(cache, target_is_directory=True)
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    base = f'http://127.0.0.1:{port}'
    def request(path, body=None) -> tuple[int, Any]:
        req = urllib.request.Request(base + path,  # noqa: S310 - fixed loopback HTTP base
                                     data=None if body is None else json.dumps(body).encode(),
                                     headers={'Content-Type': 'application/json'})
        try:
            with urllib.request.urlopen(req, timeout=90) as r:  # noqa: S310 - disposable loopback daemon
                return r.status, json.load(r)
        except urllib.error.HTTPError as e:
            return e.code, e.read().decode()
    def mcp(name, args):
        status, result = request('/mcp', {'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call',
                                         'params': {'name': name, 'arguments': args}})
        assert status == 200, f'{status}: {result!r}'
        return result['result']
    with tempfile.TemporaryFile(mode='w+') as log:
        env = dict(os.environ)
        env.pop('MEMNEST_TOKEN', None)
        child = subprocess.Popen([binary, '--data-dir', data, '--port', str(port)], stdout=log, stderr=log, env=env)
        try:
            last_error = 'service not ready'
            for _ in range(180):
                try:
                    if request('/health')[0] == 200:
                        break
                except (OSError, TimeoutError) as exc:
                    last_error = str(exc)
                if child.poll() is not None:
                    raise RuntimeError('daemon exited')
                time.sleep(1)
            else:
                raise RuntimeError(f'daemon startup timed out: {last_error}')
            # Controlled captures: raw secrets bypass write redaction to test read-time redaction.
            conn = sqlite3.connect(Path(data) / 'memory.db')
            text = '😀한é' * 20 + ' password=supersecret123 '
            fixtures = [('anchor', 'p', 's', 'pi.transcript', '/a'),
                        ('before', 'p', 's', 'pi.transcript', '/a'),
                        ('after', 'p', 's', 'pi.transcript', '/a'),
                        ('other-project', 'q', 's', 'pi.transcript', '/a'),
                        ('other-source', 'p', 's', 'codex.transcript', '/a'),
                        ('other-cwd', 'p', 's', 'pi.transcript', '/b'),
                        ('empty', 'p', '', 'pi.transcript', '/a'),
                        ('empty2', 'p', '', 'pi.transcript', '/a'),
                        ('missing-session', 'p', None, 'pi.transcript', '/a'),
                        ('trash', '_trash', 's', 'pi.transcript', '/a'),
                        ('trash2', '_trash', 's', 'pi.transcript', '/a'),
                        ('superseded', '_superseded', 's', 'pi.transcript', '/a'),
                        ('manual', 'p', 's', 'manual', '/a'),
                        ('metadata-heavy', 'p', '😀한' * 3000, 'pi.transcript', '/a')]
            for id, project, session, source, cwd in fixtures:
                timestamp = '2026-01-01T00:00:' + ('00' if id == 'before' else '01' if id == 'anchor' else '02') + '+00:00'
                metadata = {"session_id": session, "source": source, "cwd": cwd, "sequence": 999, "raw_chunk": 'never expose raw'}
                if session is None:
                    metadata.pop('session_id')
                if id == 'metadata-heavy':
                    metadata['source_ids'] = ['password=supersecret123'] * 40
                conn.execute('INSERT INTO chunks VALUES (?, ?, ?, ?, ?, ?, ?)',
                             (id, project, text, b'', json.dumps(metadata), timestamp, timestamp))
            conn.commit()
            conn.close()
            _, full = request('/chunk/anchor')
            assert full['document'].startswith('😀한é') and 'supersecret123' not in json.dumps(full)
            assert 'never expose raw' not in json.dumps(full)
            assert not full['provenance_truncated']
            _, heavy = request('/chunk/metadata-heavy?max_chars=1')
            assert heavy['total_returned_chars'] == 1 and heavy['next_offset'] == 1
            assert heavy['provenance_truncated']
            assert len(heavy['provenance']['session_id']) == 2048
            assert len(heavy['provenance']['source_ids']) == 16
            assert len(json.dumps(heavy, ensure_ascii=False)) < 3500
            assert 'supersecret123' not in json.dumps(heavy)
            assert json.loads(mcp('memory_get', {'id': 'metadata-heavy', 'max_chars': 1})['content'][0]['text']) == heavy
            _, page = request('/chunk/anchor?offset=1&max_chars=7&before=5&after=5')
            assert page['document'] == full['document'][1:8] and page['next_offset'] == 8
            assert page['has_more'] and page['total_returned_chars'] == 7
            assert [c['id'] for c in page['before']] == ['before']
            assert [c['id'] for c in page['after']] == ['after']
            assert all(c['has_more'] and c['next_offset'] == 0 for c in page['before'] + page['after'])
            _, expanded = request('/chunk/anchor?max_chars=30000&before=5&after=5')
            assert expanded['total_returned_chars'] == 3 * full['doc_len']
            assert all('supersecret123' not in c['document'] for c in expanded['before'] + expanded['after'])
            for id in ['empty', 'missing-session', 'trash', 'superseded', 'manual']:
                _, c = request(f'/chunk/{id}?before=5&after=5')
                assert not c['before'] and not c['after'], c
            _, end = request('/chunk/anchor?offset=9999&max_chars=1')
            assert end['document'] == '' and not end['has_more'] and end['next_offset'] is None
            for query in ['max_chars=0', 'max_chars=30001', 'max_chars=-1', 'before=6', 'after=-1', 'offset=-1', 'offset=1.5']:
                assert 400 <= request('/chunk/anchor?' + query)[0] < 500, query
            got = mcp('memory_get', {"id": 'anchor', "offset": 1, "max_chars": 7, "before": 5, "after": 5})
            assert json.loads(got['content'][0]['text']) == page
            for args in [{"max_chars": 0}, {"max_chars": -1}, {"max_chars": 1.5}, {"before": 6}, {"offset": -1}]:
                assert mcp('memory_get', dict(id='anchor', **args))['isError']
            # Deliberately diverse searchable rows through the real save/index path.
            for subject in ['apples orchard harvest', 'database postgres connection', 'satellite orbit telescope']:
                status, saved = request('/add', {'project': 'budget', 'text': ('budgetprobe ' + subject + ' ') * 90})
                assert status in {200, 201, 202}, f'{status}: {saved!r}'
            search = {"query": 'budgetprobe', "project": 'budget', "n_results": 3}
            old: Any = {}
            for _ in range(90):
                _, old = request('/search', search)
                if len(old['results']) == 3:
                    break
                time.sleep(1)
            assert len(old['results']) == 3, old
            # The removed experimental parameter is an ignored unknown field,
            # like other unknown inputs; it must never empty later excerpts.
            _, unchanged = request('/search', dict(search, max_chars=125))
            assert [c['id'] for c in old['results']] == [c['id'] for c in unchanged['results']]
            assert [len(c['document']) for c in unchanged['results']] == [600, 600, 600]
            result = mcp('memory_search', dict(search, max_chars=125))['content'][0]['text']
            for item in old['results']:
                assert item['document'] in result
            _, listed = request('/mcp', {'jsonrpc': '2.0', 'id': 2, 'method': 'tools/list'})
            schemas = {t['name']: t['inputSchema']['properties'] for t in listed['result']['tools']}
            assert 'max_chars' not in schemas['memory_search']
            assert 'max_chars' in schemas['memory_get']
            print('PASS real HTTP + MCP: Unicode pagination, get page/provenance caps, isolation, internal buckets, redaction, validation; all three 600-character search excerpts preserved.')
        except Exception:
            log.flush()
            log.seek(0)
            print(log.read()[-3000:], file=sys.stderr)
            raise
        finally:
            child.terminate()
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
