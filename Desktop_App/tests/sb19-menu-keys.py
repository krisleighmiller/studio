import ctypes, sys, time
x = ctypes.CDLL('libX11.so.6'); xt = ctypes.CDLL('libXtst.so.6')
x.XOpenDisplay.restype = ctypes.c_void_p
x.XStringToKeysym.argtypes = [ctypes.c_char_p]; x.XStringToKeysym.restype = ctypes.c_ulong
x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]; x.XKeysymToKeycode.restype = ctypes.c_uint
x.XFlush.argtypes = [ctypes.c_void_p]
xt.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
d = x.XOpenDisplay(None)
if not d: raise RuntimeError('No display')
for name in sys.argv[1:]:
    code = x.XKeysymToKeycode(d, x.XStringToKeysym(name.encode()))
    if not code: raise RuntimeError(name)
    xt.XTestFakeKeyEvent(d, code, 1, 0); xt.XTestFakeKeyEvent(d, code, 0, 0); x.XFlush(d); time.sleep(.08)
