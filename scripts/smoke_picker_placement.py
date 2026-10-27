"""Check that the picker opens at the mouse pointer under isolated headless KWin.

Moves the pointer with KWin's fake-input protocol, opens the picker with
`picora --show`, and checks that the window sits next to the pointer, inside the
screen, without covering it. Requires kwin_wayland with Xwayland and dbus-run-session.
Run: cargo build && uv run --with python-xlib scripts/smoke_picker_placement.py
"""
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import time

from Xlib import X, display

REPO = Path(__file__).resolve().parent.parent
BINARY = REPO / 'target' / 'debug' / 'picora'
SCREEN = (1600, 900)


def wayland_message(obj, opcode, payload=b''):
    return struct.pack('<II', obj, (8 + len(payload)) << 16 | opcode) + payload


def wayland_string(text):
    data = text.encode() + b'\0'
    return struct.pack('<I', len(data)) + data + b'\0' * (-len(data) % 4)


def move_pointer(x, y):
    """Moves KWin's pointer with org_kde_kwin_fake_input."""
    path = Path(os.environ['XDG_RUNTIME_DIR']) / os.environ['WAYLAND_DISPLAY']
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as conn:
        conn.connect(str(path))
        # wl_display.get_registry(2), then wl_display.sync(3) to end the global list.
        conn.sendall(wayland_message(1, 1, struct.pack('<I', 2))
                     + wayland_message(1, 0, struct.pack('<I', 3)))
        buffer, name, done = b'', None, False
        while not done:
            buffer += conn.recv(65536)
            while len(buffer) >= 8:
                obj, word = struct.unpack_from('<II', buffer)
                size = word >> 16
                if len(buffer) < size:
                    break
                body, buffer = buffer[8:size], buffer[size:]
                if obj == 2 and word & 0xffff == 0:
                    length, = struct.unpack_from('<I', body, 4)
                    if body[8:8 + length - 1] == b'org_kde_kwin_fake_input':
                        name, = struct.unpack_from('<I', body)
                done |= obj == 3
        if name is None:
            raise RuntimeError('KWin does not offer fake input')
        fixed = lambda value: struct.pack('<i', int(value * 256))
        conn.sendall(
            wayland_message(2, 0, struct.pack('<I', name) + wayland_string('org_kde_kwin_fake_input')
                            + struct.pack('<II', 4, 4))
            + wayland_message(4, 0, wayland_string('Picora smoke test') + wayland_string('placement'))
            + wayland_message(4, 9, fixed(x) + fixed(y))
            + wayland_message(1, 0, struct.pack('<I', 5)))
        conn.recv(65536)


def picker_rect(xdisplay, pid):
    """Client area of the visible Picora window, in root coordinates."""
    root = xdisplay.screen().root
    pid_atom = xdisplay.intern_atom('_NET_WM_PID')
    for window in root.query_tree().children:
        owner = window.get_full_property(pid_atom, X.AnyPropertyType)
        if not owner or owner.value[0] != pid or window.get_wm_name() != 'Picora':
            continue
        if window.get_attributes().map_state != X.IsViewable:
            continue
        geometry = window.get_geometry()
        origin = window.translate_coords(root, 0, 0)
        return -origin.x, -origin.y, geometry.width, geometry.height
    return None


def check(xdisplay, pid, pointer, env):
    move_pointer(*pointer)
    time.sleep(.3)
    subprocess.run([BINARY, '--show'], env=env, check=True, timeout=10)
    deadline = time.time() + 5
    rect = None
    while rect is None and time.time() < deadline:
        time.sleep(.2)
        rect = picker_rect(xdisplay, pid)
    if rect is None:
        raise RuntimeError('Picker did not appear')
    time.sleep(.5)
    x, y, width, height = picker_rect(xdisplay, pid)
    px, py = pointer
    print(f'pointer {pointer}: picker at {x},{y} {width}x{height}')
    assert 0 <= x and x + width <= SCREEN[0] and 0 <= y and y + height <= SCREEN[1], 'off screen'
    assert not (x <= px < x + width and y <= py < y + height), 'picker covers the pointer'
    assert x == min(px, SCREEN[0] - width), 'picker is not aligned with the pointer'
    assert py < y < py + 80 or y + height < py, 'picker is not next to the pointer'
    subprocess.run([BINARY, '--show'], env=env, check=True, timeout=10)
    time.sleep(.5)


def session():
    with tempfile.TemporaryDirectory(prefix='picora-placement-') as temporary:
        root = Path(temporary)
        env = dict(os.environ, PICORA_DATA_DIR=str(root / 'data'), LANG='en_US.UTF-8')
        with (root / 'picora.log').open('w+') as log:
            picora = subprocess.Popen([BINARY, '--background'], env=env, stdout=log, stderr=log)
            try:
                time.sleep(4)
                xdisplay = display.Display()
                for pointer in [(200, 150), (1400, 800), (1500, 300)]:
                    check(xdisplay, picora.pid, pointer, env)
                print('Picker placement passed', flush=True)
            except Exception:
                log.seek(0)
                print(log.read(), file=sys.stderr)
                raise
            finally:
                picora.terminate()
                picora.wait(timeout=10)
    return 0


def main():
    if sys.argv[1:] == ['--session']:
        return session()
    if sys.argv[1:] != ['--private-session']:
        return subprocess.call(['dbus-run-session', '--', sys.executable,
                                str(Path(__file__).resolve()), '--private-session'])
    if not BINARY.exists():
        raise RuntimeError('Run cargo build first')
    with tempfile.TemporaryDirectory(prefix='picora-kwin-') as temporary:
        root = Path(temporary)
        runtime = root / 'runtime'
        runtime.mkdir(mode=0o700)
        env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime),
                   XDG_CONFIG_HOME=str(root / 'config'), XDG_CACHE_HOME=str(root / 'cache'),
                   QT_QPA_PLATFORM='offscreen', LIBGL_ALWAYS_SOFTWARE='1',
                   # Lets the test move the pointer through KWin's fake-input protocol.
                   KWIN_WAYLAND_NO_PERMISSION_CHECKS='1')
        env.pop('DISPLAY', None)
        env.pop('WAYLAND_DISPLAY', None)
        with (root / 'kwin.log').open('w+') as log:
            # KWin sets WAYLAND_DISPLAY and DISPLAY for the session it runs.
            subprocess.run(
                ['kwin_wayland', '--virtual', '--xwayland', '--no-lockscreen',
                 '--no-global-shortcuts', '--no-kactivities', '--socket', 'picora-test',
                 '--width', str(SCREEN[0]), '--height', str(SCREEN[1]),
                 '--exit-with-session', f'{sys.executable} {Path(__file__).resolve()} --session'],
                env=env, stdout=log, stderr=log, timeout=180)
            log.seek(0)
            output = log.read()
            passed = 'Picker placement passed' in output
            print(output[output.find('pointer ('):] if passed else output)
            return 0 if passed else 1


if __name__ == '__main__':
    sys.exit(main())
