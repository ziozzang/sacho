#!/usr/bin/env python3
"""Qualify portable persistence against an owned PostgreSQL container only."""
import json
import os
import pathlib
import subprocess
import time
import tempfile
import sqlite3
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]

def main():
    subprocess.run(['cargo', 'build', '--locked'], cwd=ROOT, check=True)
    name = 'sacho-storage-test-' + uuid.uuid4().hex[:12]
    environment = {**os.environ, 'PGPASSWORD': 'fixture-only-password', 'PGCONNECT_TIMEOUT': '2'}
    try:
        subprocess.run(['docker', 'run', '-d', '--rm', '--name', name,
                        '--tmpfs', '/var/lib/postgresql/data:rw',
                        '-p', '127.0.0.1::5432',
                        '-e', 'POSTGRES_PASSWORD=fixture-only-password',
                        '-e', 'POSTGRES_DB=sacho_test_storage', 'postgres:17'],
                       check=True, stdout=subprocess.DEVNULL)
        info = json.loads(subprocess.check_output(['docker', 'inspect', name]))[0]
        port = info['NetworkSettings']['Ports']['5432/tcp'][0]['HostPort']
        deadline = time.monotonic() + 45
        while True:
            ready = subprocess.run(['psql', '-X', '-w', '-h', '127.0.0.1', '-p', port,
                                    '-U', 'postgres', '-d', 'sacho_test_storage', '-c', 'SELECT 1'],
                                   env=environment, capture_output=True)
            if ready.returncode == 0:
                break
            if time.monotonic() >= deadline:
                raise RuntimeError('fixture PostgreSQL TCP readiness timed out')
            time.sleep(0.2)
        environment['SACHO_TEST_DATABASE_URL'] = (
            f'postgres://postgres:fixture-only-password@127.0.0.1:{port}/sacho_test_storage?sslmode=disable')
        subprocess.run(['cargo', 'test', '--locked', '--test', 'storage', '--', '--include-ignored'],
                       cwd=ROOT, env=environment, check=True)
        subprocess.run(['docker', 'exec', name, 'createdb', '-U', 'postgres', 'sacho_test_cli'], check=True)
        environment['SACHO_DATABASE_URL'] = (
            f'postgres://postgres:fixture-only-password@127.0.0.1:{port}/sacho_test_cli?sslmode=disable')
        binary = ROOT / 'target/debug/sacho'
        def cli(*args):
            return subprocess.check_output([str(binary), *map(str, args)], env=environment, cwd=ROOT)
        with tempfile.TemporaryDirectory() as temporary:
            directory = pathlib.Path(temporary)
            source = directory / 'source.db'
            config = directory / 'config.toml'
            config.write_text('database=' + json.dumps(str(source)) + '\n')
            cli('doctor', '--config', config)
            with sqlite3.connect(source) as conn:
                conn.execute("INSERT INTO settings VALUES('fixture','CLI roundtrip')")
            before = json.loads(cli('database', 'export', '--database', source,
                                    '--output', directory / 'before.json'))
            cli('database', 'verify', '--input', directory / 'before.json')
            cli('database', 'import-postgres', '--input', directory / 'before.json')
            after = json.loads(cli('database', 'export-postgres', '--output', directory / 'after.json'))
            assert before == after
            cli('database', 'import', '--input', directory / 'after.json',
                '--database', directory / 'restored.db')
            assert cli('help', 'storage') == (ROOT / 'docs/DATABASE.md').read_bytes()
            assert cli('help', 'storage', '--language', 'ko') == (ROOT / 'docs/DATABASE.ko.md').read_bytes()
        print('PASS: SQLite → PostgreSQL → SQLite, canonical fingerprints, sequence continuation, '
              'transactional rollback, existing-target rejection, actual CLI and embedded bilingual guide')
    finally:
        subprocess.run(['docker', 'rm', '-f', name], stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL)

if __name__ == '__main__':
    main()
