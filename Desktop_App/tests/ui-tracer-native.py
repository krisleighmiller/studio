#!/usr/bin/env python3
"""Bounded Linux helpers: native key selection and exact fixture keyring cleanup.
Never reads secret values; the UUID namespace belongs solely to this tracer.
"""
import ctypes
import ctypes.util
import json
import re
import sys
import time


def keyring(api_id, delete):
    assert re.fullmatch(r"t01-ui-[0-9a-f]{32}", api_id)
    import gi
    gi.require_version("Secret", "1")
    from gi.repository import Secret
    schema = Secret.Schema.new("org.freedesktop.Secret.Generic", Secret.SchemaFlags.DONT_MATCH_NAME,
                               {"service": Secret.SchemaAttributeType.STRING,
                                "username": Secret.SchemaAttributeType.STRING})
    service = Secret.Service.get_sync(Secret.ServiceFlags.OPEN_SESSION, None)
    found = removed = remaining = 0
    errors = []
    for field in ("apiKey", "secretKey"):
        try:
            attrs = {"service": "video-workbench", "username": api_id + "::" + field}
            items = service.search_sync(schema, attrs, Secret.SearchFlags.ALL, None)
            found += len(items)
            for item in items:
                try:
                    assert all(item.get_attributes().get(k) == v for k, v in attrs.items())
                    if delete:
                        item.delete_sync(None)
                        removed += 1
                except Exception as error:
                    errors.append({"field": field, "error": str(error)})
            remaining += len(service.search_sync(schema, attrs, Secret.SearchFlags.ALL, None))
        except Exception as error:
            errors.append({"field": field, "error": str(error)})
    print(json.dumps(dict(apiId=api_id, found=found, removed=removed, remaining=remaining, errors=errors)))
    if errors or (delete and remaining):
        raise SystemExit(1)


def keys(sequence=("Home", "Return")):
    assert sequence and all(key in ("Home", "Return", "Escape") for key in sequence)
    x = ctypes.CDLL(ctypes.util.find_library("X11"))
    xt = ctypes.CDLL(ctypes.util.find_library("Xtst"))
    x.XOpenDisplay.restype = ctypes.c_void_p
    x.XOpenDisplay.argtypes = [ctypes.c_char_p]
    x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
    x.XKeysymToKeycode.restype = ctypes.c_uint
    x.XStringToKeysym.argtypes = [ctypes.c_char_p]
    x.XStringToKeysym.restype = ctypes.c_ulong
    x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
    x.XCloseDisplay.argtypes = [ctypes.c_void_p]
    xt.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
    display = x.XOpenDisplay(None)
    assert display, "XOpenDisplay failed"
    try:
        # menu-will-show precedes native widget mapping; bound the input settling
        # interval and flush each key before sending the activation key.
        time.sleep(0.15)
        for key in sequence:
            code = x.XKeysymToKeycode(display, x.XStringToKeysym(key.encode()))
            assert code
            assert xt.XTestFakeKeyEvent(display, code, 1, 0)
            x.XSync(display, 0)
            time.sleep(0.08)
            assert xt.XTestFakeKeyEvent(display, code, 0, 0)
            x.XSync(display, 0)
            time.sleep(0.1)
        x.XSync(display, 0)
    finally:
        x.XCloseDisplay(display)
    print(json.dumps({"nativeKeys": list(sequence)}))


if __name__ == "__main__":
    if sys.argv[1] == "keys":
        keys(tuple(sys.argv[2:]) or ("Home", "Return"))
    elif sys.argv[1] in ("check", "clear"):
        keyring(sys.argv[2], sys.argv[1] == "clear")
    else:
        raise ValueError("unknown helper operation")
