#!/usr/bin/env python3
"""Select an exact path in the current mapped SB14 GTK picker only."""
import ctypes as c, ctypes.util, sys, time, json
x=c.CDLL(ctypes.util.find_library('X11')); xt=c.CDLL(ctypes.util.find_library('Xtst'))
D=c.c_void_p; W=c.c_ulong
x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=D
x.XDefaultRootWindow.argtypes=[D];x.XDefaultRootWindow.restype=W
x.XQueryTree.argtypes=[D,W,c.POINTER(W),c.POINTER(W),c.POINTER(c.POINTER(W)),c.POINTER(c.c_uint)]
x.XFetchName.argtypes=[D,W,c.POINTER(c.c_char_p)]
x.XFree.argtypes=[D];x.XSync.argtypes=[D,c.c_int]
x.XGetWindowAttributes.argtypes=[D,W,D]
x.XSetInputFocus.argtypes=[D,W,c.c_int,c.c_ulong]
x.XStringToKeysym.argtypes=[c.c_char_p];x.XStringToKeysym.restype=c.c_ulong
x.XKeysymToKeycode.argtypes=[D,c.c_ulong];x.XKeysymToKeycode.restype=c.c_uint
xt.XTestFakeKeyEvent.argtypes=[D,c.c_uint,c.c_int,c.c_ulong]
class A(c.Structure):
    _fields_=[('x',c.c_int),('y',c.c_int),('width',c.c_int),('height',c.c_int),('border_width',c.c_int),('depth',c.c_int),('visual',D),('root',W),('cls',c.c_int),('bit_gravity',c.c_int),('win_gravity',c.c_int),('backing_store',c.c_int),('backing_planes',c.c_ulong),('backing_pixel',c.c_ulong),('save_under',c.c_int),('colormap',W),('map_installed',c.c_int),('map_state',c.c_int),('all_event_masks',c.c_long),('your_event_mask',c.c_long),('do_not_propagate_mask',c.c_long),('override_redirect',c.c_int),('screen',D)]
d=x.XOpenDisplay(None);assert d
try:
    def windows(w):
        if not w: return
        name=c.c_char_p();title=''
        if x.XFetchName(d,w,c.byref(name)) and name.value: title=name.value.decode(errors='replace');x.XFree(name)
        attrs=A();x.XGetWindowAttributes(d,w,c.byref(attrs))
        if title=='Attach scene look still' and attrs.map_state==2: yield w,title
        r=W();p=W();children=c.POINTER(W)();n=c.c_uint()
        if x.XQueryTree(d,w,c.byref(r),c.byref(p),c.byref(children),c.byref(n)):
            ids=[children[i] for i in range(n.value)]
            if children:x.XFree(children)
            for child in ids:yield from windows(child)
    deadline=time.monotonic()+15; found=[]
    while time.monotonic()<deadline:
        found=list(windows(x.XDefaultRootWindow(d)))
        if found:break
        time.sleep(.1)
    assert len(found)==1,found
    w,title=found[0];x.XSetInputFocus(d,w,1,0);x.XSync(d,0);time.sleep(.15)
    def event(key,down):
        code=x.XKeysymToKeycode(d,x.XStringToKeysym(key.encode()));assert code,key
        assert xt.XTestFakeKeyEvent(d,code,int(down),0);x.XSync(d,0)
    def key(k):event(k,True);event(k,False)
    event('Control_L',True);key('l');event('Control_L',False);time.sleep(.15)
    for char in sys.argv[1]:
        shifted=char.isupper() or char=='_'
        if shifted:event('Shift_L',True)
        key({'/':'slash','.':'period','-':'minus','_':'underscore'}.get(char,char))
        if shifted:event('Shift_L',False)
    key('Return');time.sleep(.8)
    # GTK's location entry can keep Return in path completion; activate its
    # visible bottom-right Open control in this exact mapped picker.
    attrs=A();x.XGetWindowAttributes(d,w,c.byref(attrs))
    x.XTranslateCoordinates.argtypes=[D,W,W,c.c_int,c.c_int,c.POINTER(c.c_int),c.POINTER(c.c_int),c.POINTER(W)]
    px=c.c_int();py=c.c_int();child=W()
    x.XTranslateCoordinates(d,w,x.XDefaultRootWindow(d),attrs.width-50,attrs.height-24,c.byref(px),c.byref(py),c.byref(child))
    xt.XTestFakeMotionEvent.argtypes=[D,c.c_int,c.c_int,c.c_int,c.c_ulong]
    xt.XTestFakeButtonEvent.argtypes=[D,c.c_uint,c.c_int,c.c_ulong]
    xt.XTestFakeMotionEvent(d,-1,px.value,py.value,0);x.XSync(d,0);time.sleep(.1)
    xt.XTestFakeButtonEvent(d,1,1,0);x.XSync(d,0);time.sleep(.08)
    xt.XTestFakeButtonEvent(d,1,0,0);x.XSync(d,0);time.sleep(.5)
    print(json.dumps({'window':w,'title':title,'mapped':True,'selected':sys.argv[1],'openClick':[px.value,py.value]}))
finally:x.XCloseDisplay.argtypes=[D];x.XCloseDisplay(d)
