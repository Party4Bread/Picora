"""Run the clipboard regression against isolated headless KWin, without X11.

Requires kwin_wayland, dbus-run-session, and a working software EGL renderer.
Run: python scripts/smoke_wayland_clipboard.py
"""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def main():
    if sys.argv[1:] != ['--private-session']:
        return subprocess.call(['dbus-run-session', '--', sys.executable,
                                str(Path(__file__).resolve()), '--private-session'])
    with tempfile.TemporaryDirectory(prefix='picora-kwin-') as temporary:
        root = Path(temporary)
        runtime = root / 'runtime'
        runtime.mkdir(mode=0o700)
        env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime),
                   XDG_CONFIG_HOME=str(root / 'config'), XDG_CACHE_HOME=str(root / 'cache'),
                   WAYLAND_DISPLAY='picora-test', QT_QPA_PLATFORM='offscreen',
                   LIBGL_ALWAYS_SOFTWARE='1')
        env.pop('DISPLAY', None)
        with (root / 'kwin.log').open('w+') as log:
            kwin = subprocess.Popen(
                ['kwin_wayland', '--virtual', '--no-lockscreen', '--no-global-shortcuts',
                 '--no-kactivities', '--socket', 'picora-test'],
                env=env, stdout=log, stderr=log)
            try:
                for _ in range(100):
                    if (runtime / 'picora-test').exists():
                        break
                    if kwin.poll() is not None:
                        raise RuntimeError('KWin exited before creating its socket')
                    time.sleep(.1)
                else:
                    raise RuntimeError('KWin socket did not become ready')
                subprocess.run(
                    ['cargo', 'test', '--locked', '--test', 'wayland_clipboard',
                     '--', '--ignored', '--nocapture'],
                    cwd=Path(__file__).resolve().parent.parent, env=env,
                    check=True, timeout=300)
            except Exception:
                log.seek(0)
                print(log.read(), file=sys.stderr)
                raise
            finally:
                kwin.terminate()
                try:
                    kwin.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    kwin.kill()
                    kwin.wait()
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
