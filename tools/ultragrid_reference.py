#!/usr/bin/env python3
"""UltraGrid reference port, written from the documentation of `src/ultragrid/mod.rs` alone.

It exists so the Android (Kotlin) port has an independent target: if this file and the Rust engine
agree on every pinned value, the written rules fully determine the puzzles. Run `--check` to compare
against the vectors pinned in the Rust tests; any drift fails loudly.

    python3 tools/ultragrid_reference.py --check
    python3 tools/ultragrid_reference.py            # prints the ten ids, pars and the set commitment
"""
import hashlib, struct, sys
DX=[0,1,0,-1]; DY=[-1,0,1,0]
N,E,S,W=0,1,2,3
WALL,TERM,STRAIGHT,BEND,TEE,CROSS=0,1,2,3,4,5
PERIOD={WALL:1,TERM:1,STRAIGHT:2,BEND:4,TEE:4,CROSS:1}
def bit(d): return 1<<d
def arms(k,o):
    if k==WALL: return 0
    if k in (TERM,CROSS): return 15
    if k==STRAIGHT: return bit(N)|bit(S) if o%2==0 else bit(E)|bit(W)
    if k==BEND: o%=4; return bit(o)|bit((o+1)%4)
    o%=4; return 15 & ~bit((W+o)%4)
class Stream:
    def __init__(s,seed,label): s.seed=seed; s.label=label; s.c=0; s.block=b''; s.pos=32
    def next(s):
        if s.pos+4>32:
            s.block=hashlib.sha3_256(b"UltraGrid/stream/v1"+s.seed+struct.pack('<I',len(s.label))+s.label+struct.pack('<Q',s.c)).digest(); s.c+=1; s.pos=0
        v=struct.unpack('<I',s.block[s.pos:s.pos+4])[0]; s.pos+=4; return v
    def below(s,n):
        limit=(0xFFFFFFFF//n)*n
        while True:
            w=s.next()
            if w<limit: return w%n
def connected(cells,w,h,src,dst):
    seen=[False]*len(cells); st=[src]; seen[src]=True
    while st:
        here=st.pop()
        if here==dst: return True
        x,y=here%w,here//w
        for d in range(4):
            nx,ny=x+DX[d],y+DY[d]
            if nx<0 or ny<0 or nx>=w or ny>=h: continue
            t=ny*w+nx
            if seen[t]: continue
            if arms(*cells[here])&bit(d) and arms(*cells[t])&bit((d+2)%4):
                seen[t]=True; st.append(t)
    return False
def route(w,h,blocked,weights,frm,to):
    n=w*h; dist=[0xFFFFFFFF]*n; prev=[None]*n; done=[False]*n; dist[frm]=0
    while True:
        best=None; bd=0xFFFFFFFF
        for i in range(n):
            if not done[i] and dist[i]<bd: bd=dist[i]; best=i
        if best is None: break
        done[best]=True
        if best==to: break
        x,y=best%w,best//w
        for d in range(4):
            nx,ny=x+DX[d],y+DY[d]
            if nx<0 or ny<0 or nx>=w or ny>=h: continue
            nxt=ny*w+nx
            if blocked[nxt] or done[nxt]: continue
            cand=bd+weights[nxt]
            if cand<dist[nxt]: dist[nxt]=cand; prev[nxt]=best
    if dist[to]==0xFFFFFFFF: return None
    p=[to]
    while p[-1]!=frm: p.append(prev[p[-1]])
    p.reverse(); return p
def dirb(a,b,w):
    ax,ay,bx,by=a%w,a//w,b%w,b//w
    return next(d for d in range(4) if ax+DX[d]==bx and ay+DY[d]==by)
def attempt(st,level):
    w=h=4+level//2; n=w*h
    src=st.below(h)*w; dst=st.below(h)*w+(w-1)
    blocked=[False]*n
    for _ in range(n//8):
        c=st.below(n)
        if c!=src and c!=dst: blocked[c]=True
    weights=[1+st.below(4) for _ in range(n)]
    path=route(w,h,blocked,weights,src,dst)
    if path is None: return None
    cells=[(WALL,0)]*n
    for c in range(n):
        if blocked[c]: continue
        r=st.below(100)
        k=STRAIGHT if r<25 else BEND if r<65 else TEE if r<90 else CROSS
        cells[c]=(k,st.below(PERIOD[k]))
    cells[src]=(TERM,0); cells[dst]=(TERM,0)
    par=0
    for i in range(1,len(path)-1):
        b,hh,a=path[i-1],path[i],path[i+1]
        needed=bit(dirb(hh,b,w))|bit(dirb(hh,a,w))
        opp=needed in (bit(N)|bit(S),bit(E)|bit(W))
        r=st.below(100)
        if opp: k=STRAIGHT if r<60 else CROSS if r<75 else TEE
        else: k=BEND if r<65 else TEE
        per=PERIOD[k]
        valid=[o for o in range(per) if arms(k,o)&needed==needed]
        planted=valid[st.below(len(valid))]
        scr=st.below(per)
        tile=(k,(planted+scr)%per); cells[hh]=tile
        best=min(((o+per-tile[1])%per,o) for o in valid)
        par+=best[0]
    if par<3 or connected(cells,w,h,src,dst): return None
    return w,h,cells,src,dst,par
def generate(seed,level):
    st=Stream(seed,b"puzzle/"+bytes([level]))
    for _ in range(2000):
        g=attempt(st,level)
        if g: return g
def pid(level,g):
    w,h,cells,src,dst,par=g
    b=bytes([1,level,w,h])+struct.pack('<HHH',src,dst,par)+bytes((k<<3)|o for k,o in cells)
    return hashlib.sha3_256(b"UltraGrid/puzzle-id/v1"+b).digest()
seed=bytes([0x42])*32
ids=[];out=[]
for lv in range(10):
    g=generate(seed,lv); i=pid(lv,g); ids.append(i); out.append((i[:8].hex(),g[5]))
h=hashlib.sha3_256(b"UltraGrid/set/v1"+struct.pack('<I',10)+b''.join(ids)).hexdigest()
if "--check" not in sys.argv:
    print(out); print(h)

GOLDEN=[("f65ba372918e9d85",9),("6c668928d256eaae",9),("2ca87b8575c4ed80",8),("bf734a2072d011e2",5),
        ("90d5841a4936de7b",5),("ecc8f7553ed68e99",5),("8f60a599beb109a1",8),("fe1591381a86dd19",5),
        ("0078468086630f4f",3),("f0dea9b16887eae6",10)]
COMMITMENT="3a8539a3bf36d06a85483a40d288b174e111ca42e74012e1081da54e0a91027c"
STREAM_WORDS=[1190497459,3033664823,940410908,2627798237]
if "--check" in sys.argv:
    st=Stream(seed,b"puzzle/\x00")
    assert [st.next() for _ in range(4)]==STREAM_WORDS, "stream words drifted"
    assert out==GOLDEN, "puzzle ids or pars drifted"
    assert h==COMMITMENT, "set commitment drifted"
    print("ok: stream, ten puzzle ids, ten pars and the set commitment all match the Rust pins")
