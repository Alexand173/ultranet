# Send one signed request to the Records Service's /internal/v1/anchor route, implementing the documented
# HMAC-SHA3-256 format independently of the Rust code (see src/games/peer.rs). Usage: peer_post.py <secret file> <json body> <base url>
import hmac, hashlib, sys, time, os, json, urllib.request, urllib.error
secret=open(sys.argv[1],'rb').read(); body=sys.argv[2].encode(); url=sys.argv[3]
ts=int(time.time()); nonce=os.urandom(16).hex(); path="/internal/v1/anchor"
canon=f"games\nPOST\n{path}\n{ts}\n{nonce}\n{hashlib.sha3_256(body).hexdigest()}".encode()
sig=hmac.new(secret,canon,"sha3_256").hexdigest()
req=urllib.request.Request(url+path,data=body,method="POST",headers={"X-Peer-Id":"games","X-Peer-Timestamp":str(ts),"X-Peer-Nonce":nonce,"X-Peer-Signature":sig,"Content-Type":"application/json"})
try:
    r=urllib.request.urlopen(req,timeout=90); print(r.status, r.read().decode())
except urllib.error.HTTPError as e:
    print(e.code, e.read().decode())
