#!/usr/bin/env python3
"""Inject real input events through /dev/uinput. Dev-only e2e helper.

The compositor sees these exactly like a physical mouse/keyboard, so they go
through cosmic-comp's normal focus/hit-testing logic.

Commands (each a separate argv item):
  abs X Y        move pointer to (X, Y) in logical px of the *active* output
  rel DX DY      relative move (unaccelerated-ish; used to hop between outputs)
  down / up      left button press / release
  click          down+up
  drag X Y N     with button held, move in N steps from current abs pos to (X, Y)
  type TEXT      type lowercase letters, digits, spaces (US layout)
  key NAME       esc | enter | backspace | tab
  sleep S        sleep S seconds
Output geometry for abs mapping: --size WxH (default 1920x1080).
"""
import fcntl
import os
import struct
import sys
import time

UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_SET_RELBIT = 0x40045566
UI_SET_ABSBIT = 0x40045567
UI_DEV_CREATE = 0x5501
UI_DEV_DESTROY = 0x5502
UI_DEV_SETUP = 0x405C5503
UI_ABS_SETUP = 0x401C5504

EV_SYN, EV_KEY, EV_REL, EV_ABS = 0, 1, 2, 3
BTN_LEFT, BTN_RIGHT, BTN_MIDDLE = 0x110, 0x111, 0x112
REL_X, REL_Y = 0, 1
ABS_X, ABS_Y = 0, 1
ABS_MAX = 65535

KEYS = {
    "esc": 1, "backspace": 14, "tab": 15, "enter": 28, "space": 57, "leftshift": 42,
}
for i, ch in enumerate("1234567890"):
    KEYS[ch] = 2 + i
for row, start in (("qwertyuiop", 16), ("asdfghjkl", 30), ("zxcvbnm", 44)):
    for i, ch in enumerate(row):
        KEYS[ch] = start + i
KEYS[" "] = 57


def ev(fd, t, c, v):
    os.write(fd, struct.pack("llHHi", 0, 0, t, c, v))


def syn(fd):
    ev(fd, EV_SYN, 0, 0)


def make_dev(name, setup):
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    setup(fd)
    # struct uinput_setup { input_id {bustype, vendor, product, version}; char name[80]; u32 ff }
    fcntl.ioctl(fd, UI_DEV_SETUP, struct.pack("HHHH80sI", 0x03, 0x1234, 0x5678, 1, name.encode(), 0))
    fcntl.ioctl(fd, UI_DEV_CREATE)
    return fd


def setup_abs(fd):
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    for b in (BTN_LEFT, BTN_RIGHT, BTN_MIDDLE):
        fcntl.ioctl(fd, UI_SET_KEYBIT, b)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_ABS)
    for a in (ABS_X, ABS_Y):
        fcntl.ioctl(fd, UI_SET_ABSBIT, a)
        # struct uinput_abs_setup { u16 code; (pad) ; input_absinfo {value,min,max,fuzz,flat,res} }
        fcntl.ioctl(fd, UI_ABS_SETUP, struct.pack("Hxxiiiiii", a, 0, 0, ABS_MAX, 0, 0, 0))


def setup_rel(fd):
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    fcntl.ioctl(fd, UI_SET_KEYBIT, BTN_LEFT)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_REL)
    fcntl.ioctl(fd, UI_SET_RELBIT, REL_X)
    fcntl.ioctl(fd, UI_SET_RELBIT, REL_Y)


def setup_kbd(fd):
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    for code in set(KEYS.values()):
        fcntl.ioctl(fd, UI_SET_KEYBIT, code)


def main(argv):
    size = (1920, 1080)
    if argv and argv[0] == "--size":
        w, h = argv[1].split("x")
        size = (int(w), int(h))
        argv = argv[2:]
    absd = make_dev("cosmic-sticky-e2e-abs", setup_abs)
    reld = make_dev("cosmic-sticky-e2e-rel", setup_rel)
    kbd = make_dev("cosmic-sticky-e2e-kbd", setup_kbd)
    time.sleep(2.0)  # let libinput/compositor pick the devices up
    cur = [0.0, 0.0]

    def move_abs(x, y):
        cur[0], cur[1] = x, y
        ev(absd, EV_ABS, ABS_X, int(x * ABS_MAX / (size[0] - 1)))
        ev(absd, EV_ABS, ABS_Y, int(y * ABS_MAX / (size[1] - 1)))
        syn(absd)

    if argv and argv[0] == "--fifo":
        # daemon mode: keep devices alive, read one command per line from the FIFO
        path = argv[1]
        def lines():
            while True:
                with open(path) as f:
                    for line in f:
                        line = line.strip()
                        if line == "quit":
                            return
                        if line:
                            yield line
        argv = lines()
    try:
        for item in argv:
            cmd = item.split(" ", 1)
            op, rest = cmd[0], (cmd[1] if len(cmd) > 1 else "")
            a = rest.split()
            if op == "abs":
                move_abs(float(a[0]), float(a[1]))
            elif op == "rel":
                ev(reld, EV_REL, REL_X, int(a[0]))
                ev(reld, EV_REL, REL_Y, int(a[1]))
                syn(reld)
            elif op in ("down", "up"):
                ev(absd, EV_KEY, BTN_LEFT, 1 if op == "down" else 0)
                syn(absd)
            elif op == "click":
                ev(absd, EV_KEY, BTN_LEFT, 1); syn(absd); time.sleep(0.05)
                ev(absd, EV_KEY, BTN_LEFT, 0); syn(absd)
            elif op == "drag":
                tx, ty, n = float(a[0]), float(a[1]), int(a[2])
                sx, sy = cur
                for k in range(1, n + 1):
                    move_abs(sx + (tx - sx) * k / n, sy + (ty - sy) * k / n)
                    time.sleep(0.016)
            elif op == "type":
                for ch in rest:
                    code = KEYS[ch]
                    ev(kbd, EV_KEY, code, 1); syn(kbd)
                    ev(kbd, EV_KEY, code, 0); syn(kbd)
                    time.sleep(0.03)
            elif op == "key":
                code = KEYS[rest]
                ev(kbd, EV_KEY, code, 1); syn(kbd)
                ev(kbd, EV_KEY, code, 0); syn(kbd)
            elif op == "mark":
                open(rest, "w").close()   # ack: all previous commands done
            elif op == "sleep":
                time.sleep(float(a[0]))
            else:
                raise SystemExit(f"unknown op {op}")
            time.sleep(0.05)
        time.sleep(0.3)
    finally:
        for fd in (absd, reld, kbd):
            fcntl.ioctl(fd, UI_DEV_DESTROY)
            os.close(fd)


if __name__ == "__main__":
    main(sys.argv[1:])
