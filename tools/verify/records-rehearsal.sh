#!/bin/bash
# Rehearse the Records Service against the repository's REAL node code, with the REAL appchain-cli.
#
# Everything here runs on this machine, on loopback, on a scratch database, with a throwaway owner key and
# throwaway funds. It touches nothing real: `records-rehearsal-node` refuses to start unless it is told it is
# disposable, binds only to 127.0.0.1, uses a database under the temp directory, and starts no peer networking.
#
# What it proves that a fake ledger cannot: how the actual node treats pending transactions, nonces, the address
# history and the anchors index, and that the Records Service behaves correctly against exactly that.
#
#   tools/verify/records-rehearsal.sh                       the scenarios below
#   REHEARSAL_HISTORY_LIMIT=1 tools/verify/records-rehearsal.sh   also fill the owner's history to the node's limit
#   SKIP_BUILD=1 ...                                        use binaries already built
#
# The scratch directory is left in place and printed at the end; remove it yourself when you are done.
# The node holds the FHE keys in memory (about 4-5 GB), so run it on a machine with room.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
B=$ROOT/target/debug
NODE_PORT=${NODE_PORT:-18081}
RECS_PORT=${RECS_PORT:-18096}
N=http://127.0.0.1:$NODE_PORT
RECS=http://127.0.0.1:$RECS_PORT
R=$(mktemp -d /tmp/rehearsal.XXXXXX)
PASS=0; FAIL=0

ok()  { if [ "$2" = "$3" ]; then echo "  ok   $1"; PASS=$((PASS+1)); else echo "  FAIL $1 (got '$2', want '$3')"; FAIL=$((FAIL+1)); fi; }
has() { if echo "$2" | grep -qF -- "$3"; then echo "  ok   $1"; PASS=$((PASS+1)); else echo "  FAIL $1 (no '$3' in: $(echo "$2" | head -c 300))"; FAIL=$((FAIL+1)); fi; }
lacks() { if echo "$2" | grep -qF -- "$3"; then echo "  FAIL $1 ('$3' present in: $(echo "$2" | head -c 300))"; FAIL=$((FAIL+1)); else echo "  ok   $1"; PASS=$((PASS+1)); fi; }

cleanup() {
  for pid in "$R/records.pid" "$R/node.pid"; do [ -f "$pid" ] && kill "$(cat "$pid")" 2>/dev/null; done
}
trap cleanup EXIT

if [ -z "${SKIP_BUILD:-}" ]; then
  (cd "$ROOT" && cargo build --bins >/dev/null 2>&1) || { echo "build failed"; exit 2; }
fi

# ---- throwaway owner key, funded scratch node ---------------------------------------------------------------
printf 'throwaway-rehearsal-passphrase-0001' > "$R/pw"; chmod 600 "$R/pw"
ULTRANET_APPCHAIN_KEY_PASSWORD="$(cat "$R/pw")" "$B/appchain-cli" generate-key --out "$R/owner.enc" >/dev/null 2>&1
OWNER=$(ULTRANET_APPCHAIN_KEY_PASSWORD="$(cat "$R/pw")" "$B/appchain-cli" address --key-file "$R/owner.enc")
mkdir -p "$R/ctl" "$R/rcred" "$R/records"
(nohup "$B/records-rehearsal-node" --disposable --db "$R/db" --control "$R/ctl" --port "$NODE_PORT" \
   --fund "$OWNER:${FUND:-6000000}" > "$R/node.log" 2>&1 & echo $! > "$R/node.pid")
for _ in $(seq 1 90); do curl -s -m 2 "$N/api/account/$OWNER" >/dev/null 2>&1 && break; sleep 1; done

mine()  { rm -f "$R/ctl/mined"; touch "$R/ctl/mine-now"; for _ in $(seq 1 240); do [ -e "$R/ctl/mined" ] && return; sleep 0.5; done; }
hist()  { curl -s "$N/api/address/$OWNER/transactions?limit=${1:-100}"; }
pending_count() { hist | python3 -c "import json,sys; print(sum(1 for t in json.load(sys.stdin)['transactions'] if t['status']=='pending'))"; }
tx_count()      { hist | python3 -c "import json,sys; print(len(json.load(sys.stdin)['transactions']))"; }
newest_pending(){ hist | python3 -c "import json,sys; p=[t for t in json.load(sys.stdin)['transactions'] if t['status']=='pending']; print(p[0]['hash'] if p else '')"; }
cli()   { "$B/appchain-cli" "$@" --key-file "$R/owner.enc" --password-file "$R/pw" --node "$N" 2>&1; }
post()  { python3 "$HERE/peer_post.py" "$R/rcred/records-peer-games.key" "{\"chain_id\":$1,\"root\":\"$2\"}" "$RECS"; }
code()  { echo "$1" | cut -c1-3; }
# The account nonce counts PENDING transactions; the newest confirmed transaction the owner SENT is always listed.
# Their difference is therefore exactly the number of pending transactions, even when the node hides them.
acct_nonce()   { curl -s "$N/api/account/$OWNER" | python3 -c "import json,sys; print(json.load(sys.stdin)['data']['nonce'])"; }
confirmed_next() { hist | python3 -c "import json,sys; o=sys.argv[1]; c=[t['nonce'] for t in json.load(sys.stdin)['transactions'] if t['status']=='confirmed' and t['sender']==o]; print(max(c)+1 if c else 0)" "$OWNER"; }
pending_by_nonce() { echo $(( $(acct_nonce) - $(confirmed_next) )); }

echo "== 0. a funded scratch chain, a throwaway owner ($OWNER)"
has "the owner is funded on the scratch node" "$(curl -s "$N/api/account/$OWNER")" '"balance":'

echo "== 1. what the real node does (assumptions tested, not assumed)"
out1=$(cli register --name "Rehearsal Peer Chain"); has "the real CLI's first transaction is accepted as pending" "$out1" "accepted, pending a block"
out2=$(cli register --name "Rehearsal Registry Chain")
has "a SECOND transaction while the first is pending is ACCEPTED, with the next nonce: the node does not refuse it" "$out2" "nonce 1"
ok "the address history lists both as pending" "$(pending_count)" 2
TX1=$(echo "$out1" | grep -o 'pending a block: [0-9a-f]*' | awk '{print $4}')
has "the transaction is visible by hash while pending" "$(curl -s "$N/api/transaction/$TX1")" '"status":"pending"'
mine
ok "after mining nothing is pending" "$(pending_count)" 0
ok "and the account nonce equals the newest confirmed nonce + 1 (so no pending, measured by nonce)" "$(pending_by_nonce)" 0
has "and the transaction is confirmed by hash" "$(curl -s "$N/api/transaction/$TX1")" '"status":"confirmed"'
has "the node registered chains 1 and 2" "$(curl -s "$N/api/appchain/ledger" | python3 -c "import json,sys; print([c['id'] for c in json.load(sys.stdin)['chains']])")" "[1, 2]"

# ---- the Records Service, with the real CLI and the real node ------------------------------------------------
cp "$R/owner.enc" "$R/rcred/records-anchor-owner.key.enc"; cp "$R/pw" "$R/rcred/records-anchor.pass"
head -c 48 /dev/urandom | base64 | tr -d '\n' > "$R/rcred/records-peer-games.key"; chmod 600 "$R/rcred/"*
cat > "$R/records.env" <<EOF
RECORDS_BIND=127.0.0.1:$RECS_PORT
RECORDS_DB_PATH=$R/records/records.db
RECORDS_NODE_API_BASE_URL=$N
RECORDS_REGISTRY_CHAIN_ID=2
RECORDS_PEER_ANCHOR_CHAINS=1
RECORDS_REQUIRE_CHAIN_PRESENCE=false
RECORDS_ANCHOR_CLI=$B/appchain-cli
RECORDS_PEER_CREDENTIAL=records-peer-games.key
RECORDS_OWNER_ADDRESS=$OWNER
CREDENTIALS_DIRECTORY=$R/rcred
EOF
cat > "$R/lingering-cli.sh" <<EOF
#!/bin/bash
# Runs the REAL appchain-cli, then (while the linger flag exists) holds its output, so a test can kill the caller
# after the node has accepted the transaction and before the caller has saved its hash.
out=\$("$B/appchain-cli" "\$@" 2>&1); status=\$?
[ -e "$R/linger" ] && sleep 6
echo "\$out"; exit \$status
EOF
chmod +x "$R/lingering-cli.sh"
use_cli() { sed -i "s#^RECORDS_ANCHOR_CLI=.*#RECORDS_ANCHOR_CLI=$1#" "$R/records.env"; }
rec()     { (set -a; . "$R/records.env"; "$B/ultranet-records" "$@" 2>&1); }
rec_start() {
  [ -f "$R/records.pid" ] && kill "$(cat "$R/records.pid")" 2>/dev/null; sleep 1
  (set -a; . "$R/records.env"; nohup "$B/ultranet-records" serve >> "$R/records.log" 2>&1 & echo $! > "$R/records.pid"); sleep 3
}

echo "== 2. check-config with the REAL appchain-cli (the owner-address check reads the real key)"
out=$(rec check-config); has "check-config passes" "$out" "UltraNet records configuration is valid."
has "and verifies the owner address against the real key" "$out" "(matches the owner key)"
rec_start

echo "== 3. the owner's own earlier transactions must be acknowledged before anything is anchored"
R1=$(python3 -c "print('a1'*32)"); R2=$(python3 -c "print('a2'*32)")
ok "refused: two owner transactions the service did not make" "$(code "$(post 1 "$R1")")" 503
out=$(rec peer-reconcile --acknowledge-owner-transactions); has "reconcile reports and acknowledges them" "$out" "acknowledged"

echo "== 4. anchoring through the Records Service, pending then mined"
r=$(post 1 "$R1"); ok "R1 is anchored (real CLI, real node)" "$(code "$r")" 200; has "as submitted" "$r" '"state":"submitted"'
ok "R1 is pending on the node" "$(pending_count)" 1
ok "and the nonce measure counts exactly that one pending transaction" "$(pending_by_nonce)" 1
r=$(post 1 "$R2"); ok "a second distinct root is refused while R1 is pending" "$(code "$r")" 502; has "with the reason" "$r" "still pending"
ok "the node saw no second transaction" "$(pending_count)" 1
mine
has "the anchors index shows R1 immediately after mining" "$(curl -s "$N/api/appchain/ledger/1/anchors")" "a1a1a1a1"
r=$(post 1 "$R1"); has "asking for R1 again is answered from the ledger" "$r" '"state":"confirmed"'
r=$(post 1 "$R2"); ok "R2 is now anchored" "$(code "$r")" 200; mine

echo "== 5. kill -9 after the node accepted the transaction and before the hash was saved"
R3=$(python3 -c "print('a3'*32)"); before=$(tx_count)
use_cli "$R/lingering-cli.sh"; rec_start; touch "$R/linger"
( post 1 "$R3" > "$R/r3.out" 2>&1 ) & BG=$!
sleep 3
ok "the node has accepted it: exactly one new pending transaction" "$(pending_count)" 1
kill -9 "$(cat "$R/records.pid")"; rm -f "$R/linger"; wait $BG 2>/dev/null; sleep 1
use_cli "$B/appchain-cli"; rec_start
r=$(post 1 "$R3"); ok "after the restart the root is refused as in doubt" "$(code "$r")" 502; has "with that reason" "$r" "in doubt"
ok "no second transaction was made" "$(( $(tx_count) - before ))" 1
mine
r=$(post 1 "$R3"); has "once mined, the ledger confirms it" "$r" '"state":"confirmed"'
ok "still exactly one transaction for that root" "$(( $(tx_count) - before ))" 1

echo "== 6. the registry day, with the real CLI"
rec game-register --id regapp --name "Reg App" --owner "$OWNER" >/dev/null; rec game-status --id regapp --status registered >/dev/null
Y1=$(date -u -d '10 days ago' +%F); Y2=$(date -u -d '11 days ago' +%F)
python3 "$HERE/mkday.py" "$R/records/records.db" "$Y1" regapp reg-y1
out=$(rec close-day --date "$Y1"); has "a registry day is anchored through the real CLI" "$out" "anchored"
ok "it is pending on the node" "$(pending_count)" 1
out=$(rec close-day --date "$Y1"); lacks "asking again while it is pending submits nothing more and raises no error" "$out" "last error"; ok "still one pending" "$(pending_count)" 1
mine
out=$(rec close-day --date "$Y1"); has "once mined the day is confirmed from the ledger" "$out" "confirmed"

echo "== 7. kill -9 on a registry day, then a pending claim verified only when mined"
python3 "$HERE/mkday.py" "$R/records/records.db" "$Y2" regapp reg-y2
before=$(tx_count); use_cli "$R/lingering-cli.sh"; touch "$R/linger"
(set -a; . "$R/records.env"; exec "$B/ultranet-records" close-day --date "$Y2" > "$R/cd2.out" 2>&1) & BG=$!
sleep 4; ok "the node accepted the registry anchor" "$(pending_count)" 1
kill -9 $BG; rm -f "$R/linger"; wait $BG 2>/dev/null; sleep 1; use_cli "$B/appchain-cli"
out=$(rec close-day --date "$Y2"); has "the restarted process finds the day in doubt" "$out" "in doubt"
ok "no second transaction" "$(( $(tx_count) - before ))" 1
TXP=$(newest_pending)
out=$(rec registry-resolve --date "$Y2" --landed "$TXP" --note "found pending on the node"); has "a pending transaction is refused without consent" "$out" "still pending"
out=$(rec registry-resolve --date "$Y2" --landed "$TXP" --note "found pending on the node" --pending-unverified-ok); has "with consent it is recorded as a CLAIM only" "$out" "recorded as a CLAIM"
out=$(rec close-day --date "$Y2"); has "the day is not anchored by a claim" "$out" "closed"; has "and says why" "$out" "still pending"
mine
out=$(rec close-day --date "$Y2"); has "once mined and indexed the ledger confirms the day" "$out" "confirmed"
ok "still exactly one transaction for it" "$(( $(tx_count) - before ))" 1

echo "== 8. a restored database: detected, held, reconciled against the real ledger"
rec backup --to "$R/backup.db" >/dev/null
R4=$(python3 -c "print('a4'*32)"); R5=$(python3 -c "print('a5'*32)")
ok "R4 anchored while live" "$(code "$(post 1 "$R4")")" 200; mine
ok "R5 anchored while live" "$(code "$(post 1 "$R5")")" 200; mine
kill "$(cat "$R/records.pid")"; sleep 1
rm -f "$R/records/records.db-wal" "$R/records/records.db-shm"; cp "$R/backup.db" "$R/records/records.db"
out=$(rec peer-usage); has "the fence notices the replaced database" "$out" "FENCE REGRESSED"
rec_start; R6=$(python3 -c "print('a6'*32)")
r=$(post 1 "$R6"); ok "anchoring is refused (503)" "$(code "$r")" 503; has "because the database looks restored" "$r" "restored from a backup"
out=$(rec peer-reconcile); ok "reconcile finds both anchors the backup forgot, on the real ledger" "$(echo "$out" | grep -c '^UNKNOWN to this database')" 2
out=$(rec peer-reconcile --apply --clear-hold); has "adopting them releases the hold" "$out" "the hold is released"
ok "anchoring resumes" "$(code "$(post 1 "$R6")")" 200; mine

if [ -n "${REHEARSAL_HISTORY_LIMIT:-}" ]; then
  echo "== 9. the node's history limit: pending transactions vanish from the list once 100 confirmed fill it"
  n=$(tx_count)
  while [ "$n" -lt 100 ]; do
    cli register --name "filler-$n" >/dev/null; n=$((n+1))
    if [ $((n % 40)) -eq 0 ]; then mine; fi
  done
  mine; ok "the owner's confirmed history now fills the node's list" "$(tx_count)" 100
  cli register --name "the-pending-one" >/dev/null
  ok "a pending transaction exists, and the node's history does NOT show it" "$(pending_count)" 0
  ok "yet the nonce measure still counts it: account nonce - (newest confirmed + 1) == 1" "$(pending_by_nonce)" 1
  r=$(post 1 "$(python3 -c "print('b1'*32)")"); ok "the service refuses to anchor (it cannot see pending transactions, and says so)" "$(code "$r")" 503
fi

echo
echo "RESULT: $PASS passed, $FAIL failed   (scratch directory left in place: $R)"
[ "$FAIL" -eq 0 ]
