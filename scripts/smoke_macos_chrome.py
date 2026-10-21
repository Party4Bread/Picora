"""Inspect real native paste events in isolated Chrome. No GitHub login or uploads.

Build `cargo build --locked --example clipboard_fixture`, then run:
uv run --with playwright python scripts/smoke_macos_chrome.py
Requires a macOS desktop and changes the system clipboard.
"""
import argparse
import hashlib
import json
import os
import time
from pathlib import Path
import platform
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from playwright.sync_api import sync_playwright


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chrome', default='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome')
    parser.add_argument('--fixture-binary', type=Path, default=Path('target/debug/examples/clipboard_fixture'))
    parser.add_argument('--output', type=Path)
    parser.add_argument('--picora-binary', type=Path, help='Also test actual picker auto-paste; requires Accessibility consent for this executable')
    args = parser.parse_args()
    assert platform.system() == 'Darwin', 'Requires macOS'
    report = {}
    with tempfile.TemporaryDirectory(prefix='picora-chrome-') as cache, sync_playwright() as p:
        browser = p.chromium.launch(executable_path=args.chrome, headless=False)
        try:
            cdp = browser.new_browser_cdp_session()
            pid = next(x['id'] for x in cdp.send('SystemInfo.getProcessInfo')['processInfo'] if x['type'] == 'browser')
            page = browser.new_page()
            html = '''
                <title>Picora clipboard test</title>
                <textarea id="editor" aria-label="Issue body" style="width:80vw;height:60vh"></textarea>
                <script>
                window.pastes = [];
                window.keys = [];
                document.addEventListener('keydown', e => window.keys.push({key:e.key,code:e.code,meta:e.metaKey,ctrl:e.ctrlKey,shift:e.shiftKey}));
                document.addEventListener('paste', async event => {
                    event.preventDefault();
                    const result = {types: [...event.clipboardData.types], files: [], complete: false};
                    window.pastes.push(result);
                    for (const file of event.clipboardData.files) {
                        result.files.push({name: file.name, type: file.type, size: file.size,
                            bytes: [...new Uint8Array(await file.arrayBuffer())]});
                    }
                    result.complete = true;
                });
                </script>
            '''
            class Handler(BaseHTTPRequestHandler):
                def do_GET(self):
                    self.send_response(200)
                    self.send_header('Content-Type', 'text/html; charset=utf-8')
                    self.end_headers()
                    self.wfile.write(html.encode())
                def log_message(self, *args):
                    pass
            server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
            threading.Thread(target=server.serve_forever, daemon=True).start()
            page.goto(f'http://127.0.0.1:{server.server_port}')
            def focus_editor():
                subprocess.run(['osascript', '-e', f'tell application "System Events" to tell (first process whose unix id is {pid}) to set frontmost to true'], check=True)
                page.wait_for_timeout(250)
                page.bring_to_front()
                page.locator('#editor').click()

            for mode, mime in [('auto', 'image/gif'), ('original', 'video/mp4'), ('still', 'image/png')]:
                writer = subprocess.Popen([str(args.fixture_binary.resolve()), mode, cache, '--hold'],
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
                try:
                    source = Path(writer.stdout.readline().strip())
                    page.evaluate('window.pastes = []')
                    focus_editor()
                    page.keyboard.press('Meta+V')
                    page.wait_for_function('window.pastes.length === 1 && window.pastes[0].complete', timeout=5000)
                    result = page.evaluate('window.pastes[0]')
                    assert len(result['files']) == 1, (mode, result)
                    file = result['files'][0]
                    copied = bytes(file.pop('bytes'))
                    assert file['type'] == mime, (mode, file)
                    assert copied == source.read_bytes(), f'{mode}: native clipboard bytes changed'
                    file['sha256'] = hashlib.sha256(copied).hexdigest()
                    report[mode] = result
                finally:
                    writer.communicate(input='\n', timeout=5)
            if args.picora_binary:
                data = Path(cache) / 'app'
                subprocess.run([str(args.fixture_binary.resolve()), 'library', str(data)], check=True, stdout=subprocess.DEVNULL)
                env = dict(os.environ, PICORA_DATA_DIR=str(data))
                with (Path(cache) / 'app.log').open('w') as log:
                    app = subprocess.Popen([str(args.picora_binary.resolve()), '--background'], env=env, stdout=log, stderr=log)
                    try:
                        deadline = time.monotonic() + 10
                        while not (data / 'picora.sock').exists():
                            assert app.poll() is None, 'Picora exited before creating its activation socket'
                            assert time.monotonic() < deadline, 'Picora activation socket timed out'
                            time.sleep(.1)
                        page.wait_for_timeout(800)
                        page.evaluate('window.pastes = []; window.keys = []')
                        focus_editor()
                        subprocess.run([str(args.picora_binary.resolve()), '--show'], env=env, check=True)
                        page.wait_for_timeout(500)
                        subprocess.run(['osascript', '-e', 'tell application "System Events" to key code 36'], check=True)
                        try:
                            page.wait_for_function('window.pastes.length === 1 && window.pastes[0].complete', timeout=5000)
                        except Exception:
                            print('Actual picker diagnostic:', page.evaluate('({focus:document.hasFocus(),active:document.activeElement?.id,keys:window.keys,pastes:window.pastes})'))
                            raise
                        page.wait_for_timeout(350)
                        assert page.evaluate('window.pastes.length') == 1, 'Duplicate automatic paste'
                        keys = page.evaluate('window.keys')
                        assert not any(k['key'] == 'Enter' for k in keys), 'Enter reached the editor'
                        assert any(k['key'].lower() == 'v' and k['meta'] for k in keys), keys
                        result = page.evaluate('window.pastes[0]')
                        result['keys'] = keys
                        assert len(result['files']) == 1, result
                        file = result['files'][0]
                        assert file['type'] == 'image/gif', file
                        copied = bytes(file.pop('bytes'))
                        assert hashlib.sha256(copied).hexdigest() == report['auto']['files'][0]['sha256']
                        assert app.poll() is None, 'Picora crashed after paste'
                        report['picker_auto_paste'] = result
                    finally:
                        app.terminate()
                        try:
                            app.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            app.kill()
                            app.wait()
            report['chrome_version'] = browser.version
        finally:
            # Keep valid image bytes on the clipboard even if a file test failed.
            subprocess.run([str(args.fixture_binary.resolve()), 'still', cache], stdout=subprocess.DEVNULL, check=True)
            browser.close()
            if 'server' in locals():
                server.shutdown()
                server.server_close()
    # Last mode copies image bytes, so no deleted file is left on the clipboard.
    result = json.dumps(report, indent=2)
    print(result)
    if args.output:
        args.output.write_text(result + '\n')


if __name__ == '__main__':
    main()
