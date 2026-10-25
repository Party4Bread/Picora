"""Exercise shipped updater binaries and replacement app startup in an isolated install."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import secrets
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import zipfile


def unpack(archive, destination):
    destination.mkdir()
    if archive.name.endswith('.tar.gz'):
        with tarfile.open(archive) as source:
            source.extractall(destination, filter='data')
    elif platform.system() == 'Darwin':
        subprocess.run(['ditto', '-x', '-k', str(archive), str(destination)], check=True)
    else:
        with zipfile.ZipFile(archive) as source:
            source.extractall(destination)
    return destination / 'Picora'


def stop_app(executable):
    if platform.system() == 'Windows':
        env = dict(os.environ, PICORA_SMOKE_EXE=str(executable))
        subprocess.run(['powershell', '-NoProfile', '-Command',
                        '$apps = @(Get-Process picora -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $env:PICORA_SMOKE_EXE }); $apps | Stop-Process -Force; $apps | ForEach-Object { $_.WaitForExit() }'],
                       env=env, check=False)
    else:
        import re
        result = subprocess.run(['pgrep', '-f', '^' + re.escape(str(executable)) + r'( |$)'],
                                capture_output=True, text=True)
        for pid in result.stdout.split():
            try:
                os.kill(int(pid), 15)
            except ProcessLookupError:
                pass


def verify(baseline, candidate):
    system = platform.system()
    target, suffix, relative = {
        'Linux': ('linux-x86_64', '*-linux-x86_64.tar.gz', 'picora'),
        'Windows': ('windows-x86_64', '*-windows-x86_64.zip', 'picora.exe'),
        'Darwin': ('macos-aarch64', '*-macos-aarch64.zip', 'Contents/MacOS/picora'),
    }[system]
    old, new = list(baseline.glob(suffix)), list(candidate.glob(suffix))
    assert len(old) == len(new) == 1, 'Expected one baseline and candidate archive'
    with tempfile.TemporaryDirectory(prefix='picora-packaged-update-') as directory:
        root = Path(directory).resolve()
        installed = unpack(old[0].resolve(), root / 'installed')
        work = root / 'work'
        work.mkdir()
        staged = unpack(new[0].resolve(), root / 'unpacked')
        shutil.move(str(staged), work / 'staged')
        staged = work / 'staged'
        if system == 'Darwin':
            installed /= 'Picora.app'
        executable = installed / relative
        replacement = staged / ('Picora.app' if system == 'Darwin' else '') / relative
        expected = hashlib.sha256(replacement.read_bytes()).digest()
        helper = work / ('updater.exe' if system == 'Windows' else 'updater')
        shutil.copy2(executable, helper)
        state = installed.parent / ('.' + installed.name + '.picora-update')
        state.mkdir()
        (state / 'last-error.txt').write_text('Previous attempt failed')
        data = root / 'data'
        data.mkdir()
        sentinel = data / 'preserve-me.txt'
        sentinel.write_text('library data stays outside the update transaction')
        env = dict(os.environ, PICORA_DATA_DIR=str(data))
        parent = subprocess.Popen([sys.executable, '-c', 'import sys; sys.stdin.buffer.read(1)'], stdin=subprocess.PIPE)
        process = None
        try:
            with socket.socket() as listener, (root / 'helper.log').open('w+') as log:
                listener.bind(('127.0.0.1', 0))
                listener.listen()
                listener.settimeout(30)
                token = secrets.token_hex(32)
                install_path = str(installed)
                if system == 'Windows' and not install_path.startswith('\\\\?\\'):
                    install_path = ('\\\\?\\UNC\\' + install_path[2:] if install_path.startswith('\\\\')
                                    else '\\\\?\\' + install_path)
                job = dict(layout=dict(root=install_path, target=target),
                           staged=dict(root=str(staged), target=target,
                                       files=[p.relative_to(staged).as_posix() for p in staged.rglob('*') if p.is_file()]),
                           parent=parent.pid, ready_address=f'127.0.0.1:{listener.getsockname()[1]}', nonce=token)
                job_file = work / 'job.json'
                job_file.write_text(json.dumps(job))
                process = subprocess.Popen([str(helper), '--update-helper', str(job_file)],
                                           stdin=subprocess.PIPE, stdout=log, stderr=log, env=env)
                with listener.accept()[0] as connection:
                    connection.settimeout(10)
                    received = b''
                    while len(received) < len(token):
                        chunk = connection.recv(len(token) - len(received))
                        assert chunk, 'Helper closed the readiness handshake'
                        received += chunk
                    assert received.decode() == token, 'Wrong helper handshake'
                    connection.sendall(b'ok')
                parent.stdin.close()
                parent.wait(timeout=10)
                process.stdin.close()
                code = process.wait(timeout=100)
                log.seek(0)
                output = log.read()
                assert code == 0, f'Packaged update failed ({code}): {output}'
                assert not (state / 'last-error.txt').exists(), 'Update left a failure notice'
                assert not (state / 'journal.json').exists(), 'Update did not commit'
                assert hashlib.sha256(executable.read_bytes()).digest() == expected, 'Wrong replacement binary'
                assert sentinel.read_text() == 'library data stays outside the update transaction'
                assert (data / 'library.sqlite3').exists(), 'Replacement did not initialize the app'
                if system == 'Darwin':
                    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(installed)], check=True)
                print(f'{target}: published baseline → candidate replacement, app health handshake, and data preservation passed')
        except Exception:
            log_path = root / 'helper.log'
            if log_path.exists():
                print(log_path.read_text(errors='replace'), file=sys.stderr)
            raise
        finally:
            if parent.poll() is None:
                parent.kill()
                parent.wait()
            if process is not None and process.poll() is None:
                process.kill()
                process.wait()
            stop_app(executable)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, default=Path('dist'))
    args = parser.parse_args()
    verify(args.baseline, args.candidate)
